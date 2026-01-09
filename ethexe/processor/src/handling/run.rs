// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! # Chunked parallel program execution
//!
//! ## Overview
//!
//! The main idea is to split programs into chunks based on their queue sizes or, in the future, another computation weight metric.
//!
//! The *chunk* is defined as a subset of programs that are executed in parallel and grouped by their queue sizes.
//!
//! This approach should speed up the processing of multiple programs in parallel.
//! Processing in chunks helps reduce wasted CPU time, as it minimizes the delay caused by the slowest message among all concurrently executed messages.
//! This works because, in sorted chunks, the computation time for each chunk element (queue messages) should be approximately equal.
//!
//! The second part of the approach is executing an entire program queue (ideally) in one go within a single runtime instance.
//! This reduces overhead by minimizing calls within the WASM runtime.
//!
//! Due to this approach, we must handle journals deterministically in two stages:
//! - The first stage occurs in the runtime, where memory allocations, pages, and related resources are managed.
//! - The second stage is the native part, where the remaining journal entries are processed.
//!
//! ---
//!
//! ## How It Works:
//!
//! For example, we have program states with the following queue sizes,
//! where in parentheses we denote the queue size of each program and N is the total number of programs:
//!
//! |                          Programs                        |
//! |------------:|-----------:|-----------:|----:|-----------:|
//! |     P_1(10) |     P_5(2) |     P_9(5) | ... |   P_N-3(7) |
//! |     P_2(3)  |     P_6(1) |    P_10(1) | ... |   P_N-2(1) |
//! |     P_3(3)  |     P_7(2) |    P_11(1) | ... |   P_N-1(2) |
//! |     P_4(1)  |     P_8(1) |    P_12(2) | ... |     P_N(3) |
//!
//! Before executing the programs, we need to split them into chunks.
//! The maximum chunk size is equal to the number of *chunk processing threads*.
//! The number of chunks is calculated as the total number of programs divided by the maximum chunk size.
//!
//! For example, given N programs and M chunks, a sorted chunk structure will look like this:
//!
//! |     chunk 0 |    chunk 1 |    chunk 2 | ... |    chunk M |
//! |------------:|-----------:|-----------:|----:|-----------:|
//! |     P_1(10) |     P_2(3) |     P_N(3) | ... |    P_4(1)  |
//! |     P_6(5)  |     P_3(3) |    P_12(2) | ... |    P_8(1)  |
//! |     P_N-3(7)|     P_5(2) |    P_11(1) | ... |    P_10(1) |
//! |     P_9(5)  |     P_7(2) |   P_N-1(2) | ... |    P_N-2(1)|
//!
//! As you can see, the chunk contents are not strictly sorted, but this is not an issue.
//! We only need chunks with approximately equal queue sizes to ensure efficient parallel execution.
//!
//! Chunks are sorted in reverse order (descending), so the first chunk contains the largest queue size.
//! In hypothetic high-load scenarios, this measure may prevent (arguably) starvation of programs with large queue sizes.
//! Also as the entire queue is processed in a single runtime instance in one go, so prioritizing larger queues improves efficiency.
//!
//! Once all program queues have been processed, we deterministically merge journals and handle them.
//! After that, we repeat the process until no more messages remain
//! or we run out of processing time/gas allowance (to be implemented).
//!
//! **High-level overview of the algorithm:**
//!
//!   1. Split programs into chunks based on their queue sizes.
//!   2. Execute program queues in parallel using runtime instances per program.
//!   3. Merge journals and handle them deterministically.
//!   4. Repeat steps 1-3 until no messages are processed, or we run out of processing time/gas allowance.
//!
//! ---
//!
//! ## Simplest Chunk Splitting Algorithm
//!
//! A basic chunk-splitting algorithm is implemented as follows:
//!
//!   1. First, calculate a temporary chunk index based on the program queue size.
//!   2. Next, store the required data in a temporary chunk list.
//!   3. Repeat this process for all programs with non-empty queues.
//!   4. Since the number of elements in each temporary chunk is random, they need to be redistributed
//!      to ensure all final chunks contain an equal number of elements.
//!      To achieve this, we first merge all temporary chunk lists sequentially
//!      and then redistribute the data according to the expected chunk size.
//!   5. Reverse the order of the chunks to ensure that the first chunk contains the largest queue size.
//!   6. Finally, we return the final chunk list.
//!
//! ---
//!
//! ## Future Improvements
//!
//! Currently, the chunk partitioning algorithm is simple and does not consider a program’s execution time.
//! In the future, we could introduce a weight multiplier to the queue size to improve partitioning efficiency.
//! This weight multiplier could be calculated based on program execution time statistics.

use crate::{
    handling::overlaid::OverlaidState,
    host::{InstanceCreator, InstanceWrapper},
};
use chunk_execution_processing::ChunkJournalsProcessingOutput;
use core_processor::common::JournalNote;
use ethexe_common::{
    StateHashWithQueueSize,
    db::CodesStorageRO,
    gear::{CHUNK_PROCESSING_GAS_LIMIT, MessageType},
};
use ethexe_db::Database;
use ethexe_runtime_common::{
    InBlockTransitions, JournalHandler, ProgramJournals, TransitionController,
};
use gear_core::gas::GasAllowanceCounter;
use gprimitives::{ActorId, H256};
use itertools::Itertools;
use tokio::task::JoinSet;

#[derive(Debug, Clone)]
pub struct RunnerConfig {
    chunk_processing_threads: usize,
    block_gas_limit: u64,
}

impl RunnerConfig {
    pub fn common(chunk_processing_threads: usize, block_gas_limit: u64) -> Self {
        Self {
            chunk_processing_threads,
            block_gas_limit,
        }
    }

    pub fn overlay(
        chunk_processing_threads: usize,
        block_gas_limit: u64,
        gas_multiplier: u64,
    ) -> Self {
        Self {
            chunk_processing_threads,
            block_gas_limit: block_gas_limit.saturating_mul(gas_multiplier),
        }
    }

    pub fn chunk_processing_threads(&self) -> usize {
        self.chunk_processing_threads
    }
}

// Run all program queues
pub async fn run(
    run_ctx: impl RunContext,
    db: Database,
    instance_creator: InstanceCreator,
    config: RunnerConfig,
) {
    let mut allowance_counter = GasAllowanceCounter::new(config.block_gas_limit);
    let chunk_size = config.chunk_processing_threads;

    // Start with injected queues processing.
    let (is_out_of_gas_for_block, run_ctx) = run_inner(
        run_ctx,
        db.clone(),
        instance_creator.clone(),
        &mut allowance_counter,
        chunk_size,
        MessageType::Injected,
    )
    .await;

    // If gas is still left in block, process canonical (Ethereum) queues
    if !is_out_of_gas_for_block {
        let _ = run_inner(
            run_ctx,
            db,
            instance_creator,
            &mut allowance_counter,
            chunk_size,
            MessageType::Canonical,
        )
        .await;
    }
}

// Convenience function to run overlaid execution
pub async fn run_overlaid(
    run_ctx: impl RunContext,
    db: Database,
    instance_creator: InstanceCreator,
    config: RunnerConfig,
) {
    let mut allowance_counter = GasAllowanceCounter::new(config.block_gas_limit);
    let chunk_size = config.chunk_processing_threads;

    // TODO: Use injected queues for overlaid execution
    let _ = run_inner(
        run_ctx,
        db,
        instance_creator,
        &mut allowance_counter,
        chunk_size,
        MessageType::Canonical,
    )
    .await;
}

// Process chosen queue type in chunks
async fn run_inner<C: RunContext>(
    mut run_ctx: C,
    db: Database,
    instance_creator: InstanceCreator,
    allowance_counter: &mut GasAllowanceCounter,
    chunk_size: usize,
    processing_queue_type: MessageType,
) -> (bool, C) {
    let mut join_set = JoinSet::new();
    let mut is_out_of_gas_for_block = false;

    loop {
        // Get actual states from transitions, stored in `run_ctx`.
        let states = run_ctx.states(processing_queue_type);

        // Prepare chunks for execution, by splitting states into chunks of the specified size.
        let chunks = chunks_splitting::prepare_execution_chunks(
            chunk_size,
            states,
            &run_ctx,
            processing_queue_type,
        );

        if chunks.is_empty() {
            // No more chunks to process. Stopping.
            break;
        }

        for chunk in chunks {
            // Spawn on a separate thread an execution of each program (it's queue) in the chunk.
            chunk_execution_spawn::spawn_chunk_execution(
                chunk,
                db.clone(),
                &instance_creator,
                allowance_counter,
                &mut join_set,
                &mut run_ctx,
                processing_queue_type,
            );

            // Collect journals from all executed programs in the chunk.
            let (chunk_journals, max_gas_spent_in_chunk) =
                chunk_execution_processing::collect_chunk_journals(&mut join_set, &mut run_ctx)
                    .await;

            // Process journals of all executed programs in the chunk.
            let output = chunk_execution_processing::process_chunk_execution_journals(
                chunk_journals,
                &db,
                allowance_counter,
                &mut is_out_of_gas_for_block,
                &mut run_ctx,
            );
            match output {
                ChunkJournalsProcessingOutput::Processed => {}
                ChunkJournalsProcessingOutput::EarlyBreak => break,
            }

            // Charge global gas allowance counter with the maximum gas spent in the chunk.
            allowance_counter.charge(max_gas_spent_in_chunk);
        }

        if is_out_of_gas_for_block {
            // Ran out of gas for the block, stop processing.
            break;
        }
    }

    (is_out_of_gas_for_block, run_ctx)
}

/// Context for running program queues in chunks.
///
/// Main responsibility of the trait is to maintain DRY principle
/// between common and overlaid execution contexts. It's not meant
/// to emphasize any particular trait/feature/abstraction.
pub(crate) trait RunContext {
    fn transitions(&mut self) -> &mut InBlockTransitions;
    fn states(
        &self,
        processing_queue_type: MessageType,
    ) -> Vec<chunks_splitting::ActorStateHashWithQueueSize>;

    /// Handle chunk data for a specific actor state.
    ///
    /// In common execution, the actor state is inserted into the chunks collection based
    /// on its queue size.
    /// In overlaid execution, the base program is always inserted into the heaviest chunk.
    ///
    /// The trait method provides a default implementation for a common execution.
    fn handle_chunk_data(
        &self,
        execution_chunks: &mut chunks_splitting::ExecutionChunks,
        actor_state: chunks_splitting::ActorStateHashWithQueueSize,
        queue_type: MessageType,
    ) {
        let chunks_splitting::ActorStateHashWithQueueSize {
            actor_id,
            hash,
            canonical_queue_size,
            injected_queue_size,
        } = actor_state;

        let queue_size = match queue_type {
            MessageType::Canonical => canonical_queue_size,
            MessageType::Injected => injected_queue_size,
        };

        let chunk_idx = execution_chunks.chunk_idx(queue_size);
        execution_chunks.insert_into(chunk_idx, actor_id, hash);
    }

    /// Checks whether queues for specified program must not be executed in the current run.
    ///
    /// In common execution, all program queues are executed as usual.
    /// In overlaid execution, the method is intended to nullify queues of programs and
    /// skip spawning jobs for them if their queues were newly nullified.
    fn check_task_no_run(&mut self, _program_id: ActorId) -> bool {
        false
    }

    /// Checks whether the run must be stopped early without executing the rest chunks.
    ///
    /// In common execution, the run is never stopped early.
    /// In overlaid execution, the method stops the run early if the expected reply is found in the journal.
    fn break_early(&mut self, _journal: &[JournalNote]) -> bool {
        false
    }
}

/// Common run context.
pub(crate) struct CommonRunContext<'a> {
    in_block_transitions: &'a mut InBlockTransitions,
}

impl<'a> CommonRunContext<'a> {
    pub(crate) fn new(in_block_transitions: &'a mut InBlockTransitions) -> Self {
        CommonRunContext {
            in_block_transitions,
        }
    }
}

impl<'a> RunContext for CommonRunContext<'a> {
    fn transitions(&mut self) -> &mut InBlockTransitions {
        self.in_block_transitions
    }

    fn states(
        &self,
        processing_queue_type: MessageType,
    ) -> Vec<chunks_splitting::ActorStateHashWithQueueSize> {
        states(&*self.in_block_transitions, processing_queue_type)
    }
}

/// Overlaid run context.
pub(crate) struct OverlaidRunContext<'a> {
    overlaid_ctx: OverlaidState,
    in_block_transitions: &'a mut InBlockTransitions,
}

impl<'a> OverlaidRunContext<'a> {
    pub(crate) fn new(
        base_program: ActorId,
        db: Database,
        in_block_transitions: &'a mut InBlockTransitions,
    ) -> Self {
        Self {
            overlaid_ctx: OverlaidState::new(base_program, db, in_block_transitions),
            in_block_transitions,
        }
    }
}

impl<'a> RunContext for OverlaidRunContext<'a> {
    fn transitions(&mut self) -> &mut InBlockTransitions {
        self.in_block_transitions
    }

    fn states(
        &self,
        processing_queue_type: MessageType,
    ) -> Vec<chunks_splitting::ActorStateHashWithQueueSize> {
        states(&*self.in_block_transitions, processing_queue_type)
    }

    fn handle_chunk_data(
        &self,
        execution_chunks: &mut chunks_splitting::ExecutionChunks,
        actor_state: chunks_splitting::ActorStateHashWithQueueSize,
        queue_type: MessageType,
    ) {
        let chunks_splitting::ActorStateHashWithQueueSize {
            actor_id,
            hash,
            canonical_queue_size,
            injected_queue_size,
        } = actor_state;

        let queue_size = match queue_type {
            MessageType::Canonical => canonical_queue_size,
            MessageType::Injected => injected_queue_size,
        };

        if self.overlaid_ctx.base_program() == actor_id {
            // Insert base program into heaviest chunk, which is going to be executed first.
            // This is done to get faster reply from the target dispatch for which overlaid
            // executor was created.
            execution_chunks.insert_into_heaviest(actor_id, hash);
        } else {
            let chunk_idx = execution_chunks.chunk_idx(queue_size);
            execution_chunks.insert_into(chunk_idx, actor_id, hash);
        }
    }

    fn check_task_no_run(&mut self, program_id: ActorId) -> bool {
        // If the queue wasn't nullified, the following call will nullify it and skip job spawning.
        self.overlaid_ctx
            .nullify_queue(program_id, self.in_block_transitions)
    }

    fn break_early(&mut self, journal: &[JournalNote]) -> bool {
        self.overlaid_ctx
            .nullify_or_break_early(journal, self.in_block_transitions)
    }
}

fn states(
    in_block_transitions: &InBlockTransitions,
    processing_queue_type: MessageType,
) -> Vec<chunks_splitting::ActorStateHashWithQueueSize> {
    in_block_transitions
        .states_iter()
        .filter_map(|(&actor_id, &state)| {
            let queue_size = match processing_queue_type {
                MessageType::Canonical => state.canonical_queue_size,
                MessageType::Injected => state.injected_queue_size,
            };

            if queue_size == 0 {
                return None;
            }
            let actor_state = chunks_splitting::ActorStateHashWithQueueSize::new(actor_id, state);

            Some(actor_state)
        })
        .collect()
}

mod chunks_splitting {
    use super::*;

    // An alias introduced for better readability of the chunks splitting steps.
    type Chunks = Vec<Vec<(ActorId, H256)>>;

    // `prepare_execution_chunks` is not exactly sorting (sorting usually `n*log(n)` this one is `O(n)`),
    // but rather partitioning into subsets (chunks) of programs with approximately similar queue sizes.
    pub(super) fn prepare_execution_chunks<R: RunContext>(
        chunk_size: usize,
        states: Vec<ActorStateHashWithQueueSize>,
        run_ctx: &R,
        processing_queue_type: MessageType,
    ) -> Chunks {
        let mut execution_chunks = ExecutionChunks::new(chunk_size, states.len());

        for state in states {
            run_ctx.handle_chunk_data(&mut execution_chunks, state, processing_queue_type);
        }

        execution_chunks.arrange_execution_chunks()
    }

    /// A helper  struct to bundle actor id, state hash and queue size together
    /// for easier handling in chunk preparation.
    pub(crate) struct ActorStateHashWithQueueSize {
        pub(crate) actor_id: ActorId,
        pub(crate) hash: H256,
        pub(crate) canonical_queue_size: usize,
        pub(crate) injected_queue_size: usize,
    }

    impl ActorStateHashWithQueueSize {
        pub(super) fn new(actor_id: ActorId, state: StateHashWithQueueSize) -> Self {
            Self {
                actor_id,
                hash: state.hash,
                canonical_queue_size: state.canonical_queue_size as usize,
                injected_queue_size: state.injected_queue_size as usize,
            }
        }
    }

    /// A helper struct to manage execution chunks during their preparation.
    pub(crate) struct ExecutionChunks {
        chunk_size: usize,
        chunks: Chunks,
    }

    impl ExecutionChunks {
        fn new(chunk_size: usize, tasks_len: usize) -> Self {
            let number_of_chunks = tasks_len.div_ceil(chunk_size);

            Self {
                chunk_size,
                chunks: vec![vec![]; number_of_chunks],
            }
        }

        /// Gets chunk index in chunks tasks queue.
        pub(super) fn chunk_idx(&self, mq_size: usize) -> usize {
            // Simplest implementation of chunk partitioning '| 1 | 2 | 3 | 4 | ..'
            debug_assert_ne!(mq_size, 0);
            mq_size.min(self.chunks.len()) - 1
        }

        /// Inserts chunk execution data into the specified chunk index.
        pub(super) fn insert_into(&mut self, idx: usize, actor_id: ActorId, hash: H256) {
            if let Some(chunk) = self.chunks.get_mut(idx) {
                chunk.push((actor_id, hash));
            } else {
                panic!(
                    "Chunk index {idx} out of bounds: chunks number - {}",
                    self.chunks.len()
                );
            }
        }

        /// Insert chunk execution data into the heaviest chunk (most prior, the last one).
        pub(super) fn insert_into_heaviest(&mut self, actor_id: ActorId, hash: H256) {
            if let Some(chunk) = self.chunks.last_mut() {
                chunk.push((actor_id, hash));
            } else {
                panic!("Chunks are empty, cannot insert into heaviest chunk");
            }
        }

        /// Arranges execution chunks by merging uneven chunks and reversing their order,
        /// so the heaviest chunks are processed first.
        fn arrange_execution_chunks(self) -> Chunks {
            self.chunks
                .into_iter()
                // Merge uneven chunks
                .flatten()
                // Repartition chunks in reverse order to ensure all chunks have an equal number of elements
                .rev()
                .chunks(self.chunk_size)
                // Convert into vector of vectors
                .into_iter()
                .map(|c| c.into_iter().collect())
                .collect()
        }
    }
}

mod chunk_execution_spawn {
    use super::*;
    use futures::prelude::*;
    use tokio::sync::oneshot;

    // An alias introduced for better readability of the chunks execution steps.
    pub(super) type ChunksJoinSet = JoinSet<(usize, ActorId, H256, ProgramJournals, u64)>;

    /// Spawns in the `join_set` tasks for each program in the chunk remembering position of the program in the chunk.
    ///
    /// Each program receives one (same copy) value of gas allowance, because all programs in the chunk are executed in parallel.
    /// It means that in the same time unit (!) all programs simultaneously charge gas allowance. If programs were to be
    /// executed concurrently, then each of the program should have received a reference to the global gas allowance counter
    /// and charge gas from it concurrently.
    ///
    /// `check_task_no_run` closure isn't used in a common execution, but it's used only for an overlaid one.
    /// The closure is intended to nullify program queues only once before execution. If the queue wasn't nullified
    /// earlier the closure will nullify it and skip spawning the job for the program queue as it's empty. If the queue
    /// was already nullified, the closure will return `false` and the job will be spawned as usual.
    /// For more info, see impl of the [`OverlaidContext`].
    pub(super) fn spawn_chunk_execution<R: RunContext>(
        chunk: Vec<(ActorId, H256)>,
        db: Database,
        instance_creator: &InstanceCreator,
        allowance_counter: &mut GasAllowanceCounter,
        join_set: &mut ChunksJoinSet,
        run_ctx: &mut R,
        processing_queue_type: MessageType,
    ) {
        for (chunk_pos, (program_id, state_hash)) in chunk.into_iter().enumerate() {
            if run_ctx.check_task_no_run(program_id) {
                continue;
            }

            let db = db.clone();
            let mut executor = instance_creator
                .instantiate()
                .expect("Failed to instantiate executor");
            let gas_allowance_for_chunk = allowance_counter.left().min(CHUNK_PROCESSING_GAS_LIMIT);

            let (tx, rx) = oneshot::channel();
            rayon::spawn(move || {
                let (jn, new_state_hash, gas_spent) = run_runtime(
                    db,
                    &mut executor,
                    program_id,
                    state_hash,
                    processing_queue_type,
                    gas_allowance_for_chunk,
                );
                tx.send((chunk_pos, program_id, new_state_hash, jn, gas_spent))
                    .expect("the main thread should not drop the receiver");
            });
            join_set.spawn(rx.unwrap_or_else(|_| panic!("worker thread has died")));
        }
    }

    fn run_runtime(
        db: Database,
        executor: &mut InstanceWrapper,
        program_id: ActorId,
        state_hash: H256,
        queue_type: MessageType,
        gas_allowance: u64,
    ) -> (ProgramJournals, H256, u64) {
        let code_id = db.program_code_id(program_id).expect("Code ID must be set");

        let instrumented_code = db.instrumented_code(ethexe_runtime_common::VERSION, code_id);
        let code_metadata = db.code_metadata(code_id);

        executor
            .run(
                db,
                program_id,
                state_hash,
                queue_type,
                instrumented_code,
                code_metadata,
                gas_allowance,
            )
            .expect("Some error occurs while running program in instance")
    }
}

mod chunk_execution_processing {
    use super::*;

    // Aliases introduced for better readability of the chunk journals processing steps.
    type MaybeProgramChunkJournals = Option<(ActorId, ChunkJournals)>;
    type ChunkJournals = Vec<ExtendedJournal>;
    type ExtendedJournal = (Vec<JournalNote>, MessageType, bool);

    /// Output of the chunk journals processing step.
    ///
    /// Chunk journals processing is actually a loop, which can break early.
    /// The early break must also stop other steps of the caller chunk processing
    /// function. So to expose the logic in a clear way, the enum is introduced.
    pub(super) enum ChunkJournalsProcessingOutput {
        Processed,
        EarlyBreak,
    }

    /// Collects journals from all executed programs in the chunk.
    ///
    /// The [`chunk_execution_spawn::spawn_chunk_execution`] step adds to the `join_set` tasks for each program in the chunk.
    /// The loop in the functions handles the output of each task:
    /// - modifies the state by setting a new state hash calculated by the [`ethexe_runtime_common::RuntimeJournalHandler`]
    /// - collects journals for later processing
    /// - tracks the maximum gas spent among all programs in the chunk
    ///
    /// Due to the nature of the parallel program queues execution (see [`chunk_execution_spawn::spawn_chunk_execution`] gas allowance clarifications),
    /// the actual gas allowance spent is actually the maximum among all programs in the chunk, not the sum.
    pub(super) async fn collect_chunk_journals<R: RunContext>(
        join_set: &mut chunk_execution_spawn::ChunksJoinSet,
        run_ctx: &mut R,
    ) -> (Vec<MaybeProgramChunkJournals>, u64) {
        let mut max_gas_spent_in_chunk = 0u64;
        let mut chunk_journals = vec![None; join_set.len()];

        let in_block_transitions = run_ctx.transitions();
        while let Some(result) = join_set
            .join_next()
            .await
            .transpose()
            .expect("Failed to join task")
        {
            let (chunk_pos, program_id, new_state_hash, program_journals, gas_spent) = result;

            // Handle state updates that occurred during journal processing within the runtime (allocations, pages).
            // This should happen before processing the journal notes because `send_dispatch` from another program can modify the state.
            in_block_transitions.modify(program_id, |state, _| {
                state.hash = new_state_hash;
            });

            chunk_journals[chunk_pos] = Some((program_id, program_journals));
            max_gas_spent_in_chunk = max_gas_spent_in_chunk.max(gas_spent);
        }

        (chunk_journals, max_gas_spent_in_chunk)
    }

    /// Processes journal of each program in the chunk.
    ///
    /// The processing is done with [`ethexe_runtime_common::JournalHandler`], which actually sends messages
    /// generated after executing program queues.
    ///
    /// The journals are processed sequentially in the order of programs in the chunk.
    ///
    /// The `early_break` closure is intended for overlaid execution mode. The closure is intended to
    /// nullify queues of receiver programs (if not nullified) until the expected reply is found.
    /// If it's found, no nullification is done and the processing breaks early.
    pub(super) fn process_chunk_execution_journals<R: RunContext>(
        chunk_journals: Vec<MaybeProgramChunkJournals>,
        db: &Database,
        allowance_counter: &GasAllowanceCounter,
        is_out_of_gas_for_block: &mut bool,
        run_ctx: &mut R,
    ) -> ChunkJournalsProcessingOutput {
        for program_journals in chunk_journals {
            let Some((program_id, program_journals)) = program_journals else {
                unreachable!(
                    "Program journal is `None`, this should never happen in a common execution"
                );
            };

            for (journal, message_type, call_reply) in program_journals {
                let break_flag = run_ctx.break_early(&journal);

                let mut journal_handler = JournalHandler {
                    program_id,
                    message_type,
                    call_reply,
                    controller: TransitionController {
                        transitions: run_ctx.transitions(),
                        storage: db,
                    },
                    gas_allowance_counter: allowance_counter,
                    chunk_gas_limit: CHUNK_PROCESSING_GAS_LIMIT,
                    out_of_gas_for_block: is_out_of_gas_for_block,
                };
                core_processor::handle_journal(journal, &mut journal_handler);

                if break_flag {
                    return ChunkJournalsProcessingOutput::EarlyBreak;
                }
            }
        }

        ChunkJournalsProcessingOutput::Processed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ethexe_common::{BlockHeader, MaybeHashOf, StateHashWithQueueSize, gear::MessageType};
    use ethexe_runtime_common::state::{
        ActiveProgram, Dispatch, MessageQueueHashWithSize, Program, ProgramState, Storage,
    };
    use gprimitives::{ActorId, MessageId};
    use std::collections::{BTreeMap, HashMap};

    #[test]
    fn chunk_partitioning() {
        const STATE_SIZE: usize = 1_000;
        const CHUNK_PROCESSING_THREADS: usize = 16;
        const MAX_QUEUE_SIZE: u8 = 20;

        let mut i = 0;
        let mut states_to_queue_size = HashMap::new();

        let states = Vec::from_iter(
            std::iter::repeat_with(|| {
                i += 1;
                let hash = H256::from_low_u64_le(i);
                let canonical_queue_size = rand::random::<u8>() % MAX_QUEUE_SIZE + 1;
                states_to_queue_size.insert(hash, canonical_queue_size as usize);

                chunks_splitting::ActorStateHashWithQueueSize::new(
                    ActorId::from(i),
                    StateHashWithQueueSize {
                        hash,
                        canonical_queue_size,
                        injected_queue_size: 0,
                    },
                )
            })
            .take(STATE_SIZE),
        );

        let common_run_context = CommonRunContext {
            in_block_transitions: &mut InBlockTransitions::default(),
        };
        let chunks = chunks_splitting::prepare_execution_chunks(
            CHUNK_PROCESSING_THREADS,
            states,
            &common_run_context,
            MessageType::Canonical,
        );

        // Checking chunks partitioning
        let accum_chunks = chunks
            .into_iter()
            .map(|chunk| {
                chunk
                    .into_iter()
                    .map(|(_, hash)| {
                        states_to_queue_size
                            .get(&hash)
                            .expect("State hash must be in the map")
                    })
                    .sum::<usize>()
            })
            .collect::<Vec<_>>();

        for i in 0..accum_chunks.len() - 1 {
            assert!(
                accum_chunks[i] >= accum_chunks[i + 1],
                "Chunks are not sorted"
            );
        }
    }

    #[test]
    fn nullification() {
        let mem_db = ethexe_db::MemDb::default();
        let db = Database::from_one(&mem_db);

        let source = ActorId::from(H256::random());
        let pid1 = ActorId::from(H256::random());
        let pid2 = ActorId::from(H256::random());

        let create_pid_state = |messages: Vec<MessageId>| {
            let mut pid_state = ProgramState {
                program: Program::Active(ActiveProgram {
                    allocations_hash: MaybeHashOf::empty(),
                    pages_hash: MaybeHashOf::empty(),
                    memory_infix: Default::default(),
                    initialized: true,
                }),
                canonical_queue: MessageQueueHashWithSize {
                    hash: MaybeHashOf::empty(),
                    cached_queue_size: 0,
                },
                injected_queue: MessageQueueHashWithSize {
                    hash: MaybeHashOf::empty(),
                    cached_queue_size: 0,
                },
                waitlist_hash: MaybeHashOf::empty(),
                stash_hash: MaybeHashOf::empty(),
                mailbox_hash: MaybeHashOf::empty(),
                balance: 1_000_000_000_000,
                executable_balance: 100_000_000_000_000,
            };

            pid_state.canonical_queue.modify_queue(&db, |queue| {
                for id in messages {
                    let dispatch = Dispatch::new(
                        &db,
                        id,
                        source,
                        vec![],
                        0,
                        false,
                        MessageType::Canonical,
                        false,
                    )
                    .expect("Failed to create dispatch");
                    queue.queue(dispatch);
                }
            });

            pid_state
        };

        fn access_state<F>(
            pid: ActorId,
            in_block_transitions: &mut InBlockTransitions,
            db: &Database,
            mut f: F,
        ) where
            F: FnMut(&mut ProgramState, &Database, &mut InBlockTransitions),
        {
            let mut tc = TransitionController {
                storage: db,
                transitions: in_block_transitions,
            };

            tc.update_state(pid, |state, storage, transitions| {
                f(state, storage, transitions)
            });
        }

        // Create a proper state for pid1
        let pid1_state = create_pid_state(vec![
            MessageId::from(H256::random()),
            MessageId::from(H256::random()),
            MessageId::from(H256::random()),
        ]);
        let pid1_state_hash = db.write_program_state(pid1_state);
        let pid1_state_hash_with_queue_size = StateHashWithQueueSize {
            hash: pid1_state_hash,
            canonical_queue_size: 0,
            injected_queue_size: 0,
        };

        // Create a proper state for pid2
        let pid2_overlay_mid2 = MessageId::from(H256::random());
        let pid2_state = create_pid_state(vec![MessageId::from(H256::random()), pid2_overlay_mid2]);
        let pid2_state_hash = db.write_program_state(pid2_state);
        let pid2_state_hash_with_queue_size = StateHashWithQueueSize {
            hash: pid2_state_hash,
            canonical_queue_size: 0,
            injected_queue_size: 0,
        };

        // Create in block transitions
        let states = BTreeMap::from([
            (pid1, pid1_state_hash_with_queue_size),
            (pid2, pid2_state_hash_with_queue_size),
        ]);
        let block_header = BlockHeader {
            height: 3,
            timestamp: 10000,
            parent_hash: H256::random(),
        };

        let mut in_block_transitions =
            InBlockTransitions::new(block_header, states, Default::default(), Default::default());

        let base_program = pid2;

        access_state(pid2, &mut in_block_transitions, &db, |state, storage, _| {
            let mut queue = state
                .canonical_queue
                .query(storage)
                .expect("Failed to read queue for pid2");
            assert_eq!(queue.len(), 2);

            let dispatch = queue
                .pop_back()
                .expect("pid2 queue has at least 2 dispatches");
            assert_eq!(dispatch.id, pid2_overlay_mid2);
        });
        access_state(pid1, &mut in_block_transitions, &db, |state, storage, _| {
            let queue = state
                .canonical_queue
                .query(storage)
                .expect("Failed to read queue for pid1");
            assert_eq!(queue.len(), 3);
        });

        let mut overlaid_ctx =
            OverlaidState::new(base_program, db.clone(), &mut in_block_transitions);
        access_state(pid2, &mut in_block_transitions, &db, |state, storage, _| {
            let mut queue = state
                .canonical_queue
                .query(storage)
                .expect("Failed to read queue for pid2");
            assert_eq!(queue.len(), 1);

            let dispatch = queue.dequeue().expect("pid2 queue has 1 dispatch");
            assert_eq!(dispatch.id, pid2_overlay_mid2);
        });

        assert!(overlaid_ctx.nullify_queue(pid1, &mut in_block_transitions));
        access_state(pid1, &mut in_block_transitions, &db, |state, storage, _| {
            let queue = state
                .canonical_queue
                .query(storage)
                .expect("Failed to read queue for pid1");
            assert_eq!(queue.len(), 0);
        });
    }
}
