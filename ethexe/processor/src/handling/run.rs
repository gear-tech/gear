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
    handling::overlaid::OverlaidContext,
    host::{InstanceCreator, InstanceWrapper},
};
use chunk_execution_processing::ChunkJournalsProcessingOutput;
use core_processor::common::JournalNote;
use ethexe_common::{
    StateHashWithQueueSize,
    db::CodesStorageRead,
    gear::{CHUNK_PROCESSING_GAS_LIMIT, Origin},
};
use ethexe_db::Database;
use ethexe_runtime_common::{
    InBlockTransitions, JournalHandler, ProgramJournals, TransitionController, state::Storage,
};
use gear_core::gas::GasAllowanceCounter;
use gprimitives::{ActorId, H256};
use itertools::Itertools;
use tokio::task::JoinSet;

pub struct RunnerConfig {
    pub chunk_processing_threads: usize,
    pub block_gas_limit: u64,
    pub gas_limit_multiplier: u64,
}

mod chunks_splitting {
    use super::*;

    // An alias introduced for better readability of the chunks splitting steps.
    type Chunks = Vec<Vec<(ActorId, H256)>>;

    pub(super) fn prepare_execution_chunks<F>(
        chunk_size: usize,
        states: Vec<ActorStateHashWithQueueSize>,
        mut handle_chunk_data: F,
    ) -> Chunks
    where
        F: FnMut(&mut ExecutionChunks, ActorStateHashWithQueueSize),
    {
        let mut execution_chunks = ExecutionChunks::new(chunk_size, states.len());

        for state in states {
            handle_chunk_data(&mut execution_chunks, state);
        }

        execution_chunks.arrange_execution_chunks()
    }

    /// A helper  struct to bundle actor id, state hash and queue size together
    /// for easier handling in chunk splitting.
    pub(super) struct ActorStateHashWithQueueSize {
        actor_id: ActorId,
        hash: H256,
        cached_queue_size: usize,
    }

    impl ActorStateHashWithQueueSize {
        pub(super) fn new(actor_id: ActorId, state: StateHashWithQueueSize) -> Self {
            Self {
                actor_id,
                hash: state.hash,
                cached_queue_size: state.cached_queue_size as usize,
            }
        }

        pub(super) fn into_inner(self) -> (ActorId, H256, usize) {
            (self.actor_id, self.hash, self.cached_queue_size)
        }
    }

    pub(super) struct ExecutionChunks {
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

        pub(super) fn chunk_idx(&self, queue_size: usize) -> usize {
            // Simplest implementation of chunk partitioning '| 1 | 2 | 3 | 4 | ..'
            debug_assert_ne!(queue_size, 0);
            queue_size.min(self.chunks.len()) - 1
        }

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

        pub(super) fn insert_into_heaviest(&mut self, actor_id: ActorId, hash: H256) {
            if let Some(chunk) = self.chunks.last_mut() {
                chunk.push((actor_id, hash));
            } else {
                panic!("Chunks are empty, cannot insert into heaviest chunk");
            }
        }

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

    // An alias introduced for better readability of the chunks execution steps.
    pub(super) type ChunksJoinSet = JoinSet<(usize, ActorId, H256, ProgramJournals, u64)>;

    pub(super) fn spawn_chunk_execution<F>(
        chunk: Vec<(ActorId, H256)>,
        db: Database,
        instance_creator: &InstanceCreator,
        allowance_counter: &mut GasAllowanceCounter,
        join_set: &mut ChunksJoinSet,
        mut check_task_no_run: Option<F>,
    ) where
        F: FnMut(ActorId) -> bool,
    {
        for (chunk_pos, (program_id, state_hash)) in chunk.into_iter().enumerate() {
            if let Some(checker) = check_task_no_run.as_mut()
                && checker(program_id)
            {
                continue;
            }

            let db = db.clone();
            let mut executor = instance_creator
                .instantiate()
                .expect("Failed to instantiate executor");
            let gas_allowance_for_chunk = allowance_counter.left().min(CHUNK_PROCESSING_GAS_LIMIT);

            join_set.spawn_blocking(move || {
                let (jn, new_state_hash, gas_spent) = run_runtime(
                    db,
                    &mut executor,
                    program_id,
                    state_hash,
                    gas_allowance_for_chunk,
                );
                (chunk_pos, program_id, new_state_hash, jn, gas_spent)
            });
        }
    }

    fn run_runtime(
        db: Database,
        executor: &mut InstanceWrapper,
        program_id: ActorId,
        state_hash: H256,
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
                instrumented_code,
                code_metadata,
                gas_allowance,
            )
            .expect("Some error occurs while running program in instance")
    }
}

mod chunk_execution_processing {
    use super::*;

    type MaybeProgramChunkJournals = Option<(ActorId, ChunkJournals)>;
    type ChunkJournals = Vec<ExtendedJournal>;
    type ExtendedJournal = (Vec<JournalNote>, Origin, bool);

    /// Output of the chunk journals processing step.
    ///
    /// Chunk journals processing is actually a loop, which can break early.
    /// The early break must also stop other steps of the caller chunk processing
    /// function. So to expose the logic in a clear way, the enum is introduced.
    pub(super) enum ChunkJournalsProcessingOutput {
        Processed,
        EarlyBreak,
    }

    pub(super) async fn collect_chunk_journals(
        join_set: &mut chunk_execution_spawn::ChunksJoinSet,
        in_block_transitions: &mut InBlockTransitions,
    ) -> (Vec<MaybeProgramChunkJournals>, u64) {
        let mut max_gas_spent_in_chunk = 0u64;
        let mut chunk_journals = vec![None; join_set.len()];

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

    pub(super) fn process_chunk_execution_journals(
        chunk_journals: Vec<MaybeProgramChunkJournals>,
        db: &Database,
        allowance_counter: &GasAllowanceCounter,
        in_block_transitions: &mut InBlockTransitions,
        is_out_of_gas_for_block: &mut bool,
        mut early_break: Option<impl FnMut(&Vec<JournalNote>, &mut InBlockTransitions) -> bool>,
    ) -> ChunkJournalsProcessingOutput {
        for program_journals in chunk_journals {
            let Some((program_id, program_journals)) = program_journals else {
                unreachable!(
                    "Program journal is `None`, this should never happen in a common execution"
                );
            };

            for (journal, dispatch_origin, call_reply) in program_journals {
                log::warn!("Checking journal {journal:#?}");
                let break_flag = early_break
                    .as_mut()
                    .map(|f| f(&journal, in_block_transitions));

                let mut journal_handler = JournalHandler {
                    program_id,
                    dispatch_origin,
                    call_reply,
                    controller: TransitionController {
                        transitions: in_block_transitions,
                        storage: db,
                    },
                    gas_allowance_counter: allowance_counter,
                    chunk_gas_limit: CHUNK_PROCESSING_GAS_LIMIT,
                    out_of_gas_for_block: is_out_of_gas_for_block,
                };
                core_processor::handle_journal(journal, &mut journal_handler);

                if break_flag == Some(true) {
                    return ChunkJournalsProcessingOutput::EarlyBreak;
                }
            }
        }

        ChunkJournalsProcessingOutput::Processed
    }
}

pub async fn run_new(
    db: Database,
    instance_creator: InstanceCreator,
    in_block_transitions: &mut InBlockTransitions,
    config: RunnerConfig,
) {
    let mut join_set = JoinSet::new();
    let chunk_size = config.chunk_processing_threads;
    let mut allowance_counter = GasAllowanceCounter::new(config.block_gas_limit);
    let mut is_out_of_gas_for_block = false;

    loop {
        let states = in_block_transitions
            .states_iter()
            .filter_map(|(&actor_id, &state)| {
                if state.cached_queue_size == 0 {
                    return None;
                }
                let actor_state =
                    chunks_splitting::ActorStateHashWithQueueSize::new(actor_id, state);

                Some(actor_state)
            })
            .collect();

        let chunks = chunks_splitting::prepare_execution_chunks(
            chunk_size,
            states,
            |execution_chunks, actor_state| {
                let (actor_id, hash, queue_size) = actor_state.into_inner();
                let chunk_idx = execution_chunks.chunk_idx(queue_size);
                execution_chunks.insert_into(chunk_idx, actor_id, hash);
            },
        );

        if chunks.is_empty() {
            // No more chunks to process. Stopping.
            break;
        }

        for chunk in chunks {
            chunk_execution_spawn::spawn_chunk_execution(
                chunk,
                db.clone(),
                &instance_creator,
                &mut allowance_counter,
                &mut join_set,
                None::<fn(ActorId) -> bool>,
            );

            let (chunk_journals, max_gas_spent_in_chunk) =
                chunk_execution_processing::collect_chunk_journals(
                    &mut join_set,
                    in_block_transitions,
                )
                .await;

            let output = chunk_execution_processing::process_chunk_execution_journals(
                chunk_journals,
                &db,
                &allowance_counter,
                in_block_transitions,
                &mut is_out_of_gas_for_block,
                None::<fn(&Vec<JournalNote>, &mut InBlockTransitions) -> bool>,
            );
            match output {
                ChunkJournalsProcessingOutput::Processed => {}
                ChunkJournalsProcessingOutput::EarlyBreak => break,
            }

            allowance_counter.charge(max_gas_spent_in_chunk);

            if is_out_of_gas_for_block {
                // Ran out of gas for the block, stopping processing.
                break;
            }
        }
    }
}

pub async fn run_overlaid(
    db: Database,
    instance_creator: InstanceCreator,
    in_block_transitions: &mut InBlockTransitions,
    config: RunnerConfig,
    base_program: ActorId,
) {
    let mut join_set = JoinSet::new();
    let chunk_size = config.chunk_processing_threads;
    let mut allowance_counter = GasAllowanceCounter::new(
        config
            .block_gas_limit
            .saturating_mul(config.gas_limit_multiplier),
    );
    let mut is_out_of_gas_for_block = false;
    let mut overlaid_ctx = OverlaidContext::new(base_program, db.clone(), in_block_transitions);

    loop {
        let states = in_block_transitions
            .states_iter()
            .filter_map(|(&actor_id, &state)| {
                if state.cached_queue_size == 0 {
                    return None;
                }
                let actor_state =
                    chunks_splitting::ActorStateHashWithQueueSize::new(actor_id, state);

                Some(actor_state)
            })
            .collect();

        let chunks =
            chunks_splitting::prepare_execution_chunks(chunk_size, states, |chunk, actor_state| {
                let (actor_id, hash, queue_size) = actor_state.into_inner();
                if base_program == actor_id {
                    // Insert base program into heaviest chunk, which is going to be executed first.
                    // This is done to get faster reply from the target dispatch for which overlaid
                    // executor was created.
                    chunk.insert_into_heaviest(actor_id, hash);
                } else {
                    let chunk_idx = chunk.chunk_idx(queue_size);
                    chunk.insert_into(chunk_idx, actor_id, hash);
                }
            });

        if chunks.is_empty() {
            // No more chunks to process. Stopping.
            break;
        }

        for chunk in chunks {
            chunk_execution_spawn::spawn_chunk_execution(
                chunk,
                db.clone(),
                &instance_creator,
                &mut allowance_counter,
                &mut join_set,
                Some(|program_id| {
                    // If the queue wasn't nullified, the following call will nullify it and skip job spawning.
                    overlaid_ctx.nullify_queue(program_id, in_block_transitions)
                }),
            );

            let (chunk_journals, max_gas_spent_in_chunk) =
                chunk_execution_processing::collect_chunk_journals(
                    &mut join_set,
                    in_block_transitions,
                )
                .await;

            let output = chunk_execution_processing::process_chunk_execution_journals(
                chunk_journals,
                &db,
                &allowance_counter,
                in_block_transitions,
                &mut is_out_of_gas_for_block,
                Some(
                    |journal: &Vec<JournalNote>, in_block_transitions: &mut InBlockTransitions| {
                        overlaid_ctx.nullify_or_break_early(journal, in_block_transitions)
                    },
                ),
            );

            match output {
                ChunkJournalsProcessingOutput::Processed => {}
                ChunkJournalsProcessingOutput::EarlyBreak => break,
            }

            allowance_counter.charge(max_gas_spent_in_chunk);

            if is_out_of_gas_for_block {
                // Ran out of gas for the block, stopping processing.
                break;
            }
        }
    }
}

// pub async fn run(
//     db: Database,
//     instance_creator: InstanceCreator,
//     in_block_transitions: &mut InBlockTransitions,
//     config: RunnerConfig,
//     execution_mode: ExecutionMode,
// ) {
//     let mut join_set = JoinSet::new();
//     let chunk_size = config.chunk_processing_threads;
//     let mut allowance_counter = GasAllowanceCounter::new(
//         config
//             .block_gas_limit
//             .saturating_mul(config.gas_limit_multiplier),
//     );
//     let mut is_out_of_gas_for_block = false;

//     let mut overlaid_ctx = execution_mode
//         .overlay_base_program()
//         .map(|base_program| OverlaidContext::new(base_program, db.clone(), in_block_transitions));

//     loop {
//         let states = in_block_transitions
//             .states_iter()
//             .filter_map(|(&actor_id, &state)| {
//                 if state.cached_queue_size == 0 {
//                     return None;
//                 }

//                 Some((actor_id, state))
//             })
//             .collect();

//         let chunks = split_to_chunks(chunk_size, states, execution_mode);

//         if chunks.is_empty() {
//             // No more chunks to process. Stopping.
//             break;
//         }

//         for chunk in chunks {
//             let chunk_len = chunk.len();

//             spawn_chunk_execution(
//                 chunk,
//                 db.clone(),
//                 &instance_creator,
//                 &mut allowance_counter,
//                 &mut join_set,
//                 &mut overlaid_ctx,
//                 in_block_transitions,
//             );

//             let (chunk_journals, max_gas_spent_in_chunk) =
//                 collect_chunk_journals(&mut join_set, chunk_len, in_block_transitions).await;

//             let output = process_chunk_execution_journals(
//                 chunk_journals,
//                 execution_mode.is_overlaid(),
//                 &db,
//                 &allowance_counter,
//                 &mut overlaid_ctx,
//                 in_block_transitions,
//                 &mut is_out_of_gas_for_block,
//             );
//             match output {
//                 ChunkJournalsProcessingOutput::Processed => {}
//                 ChunkJournalsProcessingOutput::EarlyBreak => break,
//             }

//             allowance_counter.charge(max_gas_spent_in_chunk);

//             if is_out_of_gas_for_block {
//                 // Ran out of gas for the block, stopping processing.
//                 break;
//             }
//         }
//     }
// }

// `split_to_chunks` is not exactly sorting (sorting usually `n*log(n)` this one is `O(n)``),
// but rather partitioning into subsets (chunks) of programs with approximately similar queue sizes.

// fn split_to_chunks(
//     chunk_size: usize,
//     states: Vec<(ActorId, StateHashWithQueueSize)>,
//     execution_mode: ExecutionMode,
// ) -> Vec<Vec<(ActorId, H256)>> {
//     fn chunk_idx(queue_size: usize, number_of_chunks: usize) -> usize {
//         // Simplest implementation of chunk partitioning '| 1 | 2 | 3 | 4 | ..'
//         debug_assert_ne!(queue_size, 0);
//         queue_size.min(number_of_chunks) - 1
//     }

//     let number_of_chunks = states.len().div_ceil(chunk_size);
//     let mut chunks = vec![vec![]; number_of_chunks];

//     for (
//         actor_id,
//         StateHashWithQueueSize {
//             hash,
//             cached_queue_size,
//         },
//     ) in states
//     {
//         let queue_size = cached_queue_size as usize;
//         let chunk_idx = chunk_idx(queue_size, number_of_chunks);

//         if let Some(base_program) = execution_mode.overlay_base_program()
//             && base_program == actor_id
//         {
//             // Insert base program into heaviest chunk, which is going to be executed first.
//             // This is done to get faster reply from the target dispatch for which overlaid
//             // executor was created.
//             chunks
//                 .last_mut()
//                 .expect("chunks instantiated with `number_of_chunks` len")
//                 .push((actor_id, hash));
//         } else {
//             chunks[chunk_idx].push((actor_id, hash));
//         }
//     }

//     chunks
//         .into_iter()
//         // Merge uneven chunks
//         .flatten()
//         // Repartition chunks in reverse order to ensure all chunks have an equal number of elements
//         .rev()
//         .chunks(chunk_size)
//         // Convert into vector of vectors
//         .into_iter()
//         .map(|c| c.into_iter().collect())
//         .collect()
// }

/// Spawns in the `join_set` tasks for each program in the chunk remembering position of the program in the chunk.
///
/// Each program receives one (same copy) value of gas allowance, because all programs in the chunk are executed in parallel.
/// It means that in the same time unit (!) all programs simultaneously charge gas allowance. If programs were to be
/// executed concurrently, then each of the program should have received a reference to the global gas allowance counter
/// and charge gas from it concurrently.
///
/// If it's a case of an overlaid execution (i.e. `overlaid_ctx` is `Some`), then the queues of all programs are nullified.
/// The nullification is done only once. For more info, see impl of the [`OverlaidContext`].

// fn spawn_chunk_execution(
//     chunk: Vec<(ActorId, H256)>,
//     db: Database,
//     instance_creator: &InstanceCreator,
//     allowance_counter: &mut GasAllowanceCounter,
//     join_set: &mut ChunksJoinSet,
//     overlaid_ctx: &mut Option<OverlaidContext>,
//     in_block_transitions: &mut InBlockTransitions,
// ) {
//     for (chunk_pos, (program_id, state_hash)) in chunk.into_iter().enumerate() {
//         let db = db.clone();
//         let mut executor = instance_creator
//             .instantiate()
//             .expect("Failed to instantiate executor");

//         let gas_allowance_for_chunk = allowance_counter.left().min(CHUNK_PROCESSING_GAS_LIMIT);

//         if let Some(overlaid_ctx) = overlaid_ctx.as_mut() {
//             // If we are in overlaid execution mode, nullify queues for all programs except for the base one.
//             if overlaid_ctx.nullify_queue(program_id, in_block_transitions) {
//                 // If the queue was already nullified, skip job spawning.
//                 continue;
//             }
//         }

//         join_set.spawn_blocking(move || {
//             let (jn, new_state_hash, gas_spent) = run_runtime(
//                 db,
//                 &mut executor,
//                 program_id,
//                 state_hash,
//                 gas_allowance_for_chunk,
//             );
//             (chunk_pos, program_id, new_state_hash, jn, gas_spent)
//         });
//     }
// }

// fn run_runtime(
//     db: Database,
//     executor: &mut InstanceWrapper,
//     program_id: ActorId,
//     state_hash: H256,
//     gas_allowance: u64,
// ) -> (ProgramJournals, H256, u64) {
//     let code_id = db.program_code_id(program_id).expect("Code ID must be set");

//     let instrumented_code = db.instrumented_code(ethexe_runtime_common::VERSION, code_id);
//     let code_metadata = db.code_metadata(code_id);

//     executor
//         .run(
//             db,
//             program_id,
//             state_hash,
//             instrumented_code,
//             code_metadata,
//             gas_allowance,
//         )
//         .expect("Some error occurs while running program in instance")
// }

/// Collects journals from all executed programs in the chunk.
///
/// The [`spawn_chunk_execution`] step adds to the `join_set` tasks for each program in the chunk.
/// The loop in the functions handles the output of each task:
/// - modifies the state by setting a new state hash calculated by the [`ethexe_runtime_common::RuntimeJournalHandler`]
/// - collects journals for later processing
/// - tracks the maximum gas spent among all programs in the chunk
///
/// Due to the nature of the parallel program queues execution (see [`spawn_chunk_execution`] gas allowance clarifications),
/// the actual gas allowance spent is actually the maximum among all programs in the chunk, not the sum.

// async fn collect_chunk_journals(
//     join_set: &mut ChunksJoinSet,
//     chunk_len: usize,
//     in_block_transitions: &mut InBlockTransitions,
// ) -> (ChunkJournals, u64) {
//     let mut max_gas_spent_in_chunk = 0u64;
//     let mut chunk_journals = vec![None; chunk_len];

//     while let Some(result) = join_set
//         .join_next()
//         .await
//         .transpose()
//         .expect("Failed to join task")
//     {
//         let (chunk_pos, program_id, new_state_hash, program_journals, gas_spent) = result;

//         // Handle state updates that occurred during journal processing within the runtime (allocations, pages).
//         // This should happen before processing the journal notes because `send_dispatch` from another program can modify the state.
//         in_block_transitions.modify(program_id, |state, _| {
//             state.hash = new_state_hash;
//         });

//         chunk_journals[chunk_pos] = Some((program_id, program_journals));
//         max_gas_spent_in_chunk = max_gas_spent_in_chunk.max(gas_spent);
//     }

//     (chunk_journals, max_gas_spent_in_chunk)
// }

/// Processes journal of each program in the chunk.
///
/// The processing is done with [`ethexe_runtime_common::JournalHandler`], which actually sends messages
/// generated after executing program queues.
///
/// The journals are processed sequentially in the order of programs in the chunk.
/// If it's an overlaid execution, then the queues of the programs are nullified (if not already nullified)
/// until the expected reply is found (see [`try_set_early_break`] for more details)

// fn process_chunk_execution_journals(
//     chunk_journals: ChunkJournals,
//     is_overlaid_execution: bool,
//     db: &Database,
//     allowance_counter: &GasAllowanceCounter,
//     overlaid_ctx: &mut Option<OverlaidContext>,
//     in_block_transitions: &mut InBlockTransitions,
//     is_out_of_gas_for_block: &mut bool,
// ) -> ChunkJournalsProcessingOutput {
//     for program_journals in chunk_journals {
//         let Some((program_id, program_journals)) = program_journals else {
//             if is_overlaid_execution {
//                 continue;
//             }

//             unreachable!(
//                 "Program journal is `None`, this should never happen in a common execution"
//             );
//         };

//         // Flag signals that journals processing can be stopped early, as an expected reply was found.
//         let mut overlay_early_break = is_overlaid_execution.then_some(false);
//         for (journal, dispatch_origin, call_reply) in program_journals {
//             if let Some(_flag) = overlay_early_break.as_mut() {
//                 // try_set_early_break(flag, &journal);
//                 todo!()
//             }

//             // When it's `Some(true)`, no need to nullify queues anymore.
//             if matches!(overlay_early_break, Some(false))
//                 && let Some(overlaid_ctx) = overlaid_ctx.as_mut()
//             {
//                 overlaid_ctx.nullify_receivers_queues(&journal, in_block_transitions);
//             }

//             let mut journal_handler = JournalHandler {
//                 program_id,
//                 dispatch_origin,
//                 call_reply,
//                 controller: TransitionController {
//                     transitions: in_block_transitions,
//                     storage: db,
//                 },
//                 gas_allowance_counter: allowance_counter,
//                 chunk_gas_limit: CHUNK_PROCESSING_GAS_LIMIT,
//                 out_of_gas_for_block: is_out_of_gas_for_block,
//             };
//             core_processor::handle_journal(journal, &mut journal_handler);

//             if overlay_early_break == Some(true) {
//                 return ChunkJournalsProcessingOutput::EarlyBreak;
//             }
//         }
//     }

//     ChunkJournalsProcessingOutput::Processed
// }

#[cfg(test)]
mod tests {
    use super::*;
    use ethexe_common::{BlockHeader, StateHashWithQueueSize, gear::Origin};
    use ethexe_runtime_common::state::{
        ActiveProgram, Dispatch, MaybeHashOf, MessageQueueHashWithSize, Program, ProgramState,
        Storage,
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
                let cached_queue_size = rand::random::<u8>() % MAX_QUEUE_SIZE + 1;
                states_to_queue_size.insert(hash, cached_queue_size as usize);

                chunks_splitting::ActorStateHashWithQueueSize::new(
                    ActorId::from(i),
                    StateHashWithQueueSize {
                        hash,
                        cached_queue_size,
                    },
                )
            })
            .take(STATE_SIZE),
        );

        let chunks = chunks_splitting::prepare_execution_chunks(
            CHUNK_PROCESSING_THREADS,
            states,
            |chunks, actor_state| {
                let (actor_id, hash, queue_size) = actor_state.into_inner();
                let chunk_idx = chunks.chunk_idx(queue_size);
                chunks.insert_into(chunk_idx, actor_id, hash);
            },
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
                queue: MessageQueueHashWithSize {
                    hash: MaybeHashOf::empty(),
                    cached_queue_size: 0,
                },
                waitlist_hash: MaybeHashOf::empty(),
                stash_hash: MaybeHashOf::empty(),
                mailbox_hash: MaybeHashOf::empty(),
                balance: 1_000_000_000_000,
                executable_balance: 100_000_000_000_000,
            };

            pid_state.queue.modify_queue(&db, |queue| {
                for id in messages {
                    let dispatch =
                        Dispatch::new(&db, id, source, vec![], 0, false, Origin::Ethereum, false)
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
            cached_queue_size: 0,
        };

        // Create a proper state for pid2
        let pid2_overlay_mid2 = MessageId::from(H256::random());
        let pid2_state = create_pid_state(vec![MessageId::from(H256::random()), pid2_overlay_mid2]);
        let pid2_state_hash = db.write_program_state(pid2_state);
        let pid2_state_hash_with_queue_size = StateHashWithQueueSize {
            hash: pid2_state_hash,
            cached_queue_size: 0,
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
            InBlockTransitions::new(block_header, states, Default::default());

        let base_program = pid2;

        access_state(pid2, &mut in_block_transitions, &db, |state, storage, _| {
            let mut queue = state
                .queue
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
                .queue
                .query(storage)
                .expect("Failed to read queue for pid1");
            assert_eq!(queue.len(), 3);
        });

        let mut overlaid_ctx =
            OverlaidContext::new(base_program, db.clone(), &mut in_block_transitions);
        access_state(pid2, &mut in_block_transitions, &db, |state, storage, _| {
            let mut queue = state
                .queue
                .query(storage)
                .expect("Failed to read queue for pid2");
            assert_eq!(queue.len(), 1);

            let dispatch = queue.dequeue().expect("pid2 queue has 1 dispatch");
            assert_eq!(dispatch.id, pid2_overlay_mid2);
        });

        assert!(overlaid_ctx.nullify_queue(pid1, &mut in_block_transitions));
        access_state(pid1, &mut in_block_transitions, &db, |state, storage, _| {
            let queue = state
                .queue
                .query(storage)
                .expect("Failed to read queue for pid1");
            assert_eq!(queue.len(), 0);
        });
    }
}
