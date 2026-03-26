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

// TODO: #5120 split to several files and move to separate module

use crate::{
    ProcessorError, Result, handling::run::chunks_splitting::ExecutionChunks, host::InstanceCreator,
};
use chunk_execution_processing::ChunkJournalsProcessingOutput;
use chunks_splitting::ActorStateHashWithQueueSize;
use core_processor::common::JournalNote;
use ethexe_common::{
    BlockHeader, CALL_REPLY_SOFT_LIMIT, OUTGOING_MESSAGES_BYTES_SOFT_LIMIT,
    OUTGOING_MESSAGES_SOFT_LIMIT, PROGRAM_MODIFICATIONS_SOFT_LIMIT, PromisePolicy,
    StateHashWithQueueSize,
    db::CodesStorageRO,
    gear::{CHUNK_PROCESSING_GAS_LIMIT, MessageType},
    injected::Promise,
};
use ethexe_db::{CASDatabase, Database};
use ethexe_runtime_common::{
    BlockInfo, InBlockTransitions, JournalHandler, ProgramJournals, TransitionController,
};
use futures::prelude::*;
use gear_core::{
    code::{CodeMetadata, InstrumentedCode},
    gas::GasAllowanceCounter,
};
use gprimitives::{ActorId, CodeId, H256};
use itertools::Itertools;
use tokio::sync::mpsc;

// Process chosen queue type in chunks
pub(super) async fn run_for_queue_type(
    ctx: &mut impl RunContext,
    queue_type: MessageType,
) -> Result<()> {
    'main_loop: loop {
        // Prepare chunks for execution, by splitting states into chunks of the specified size.
        let chunks = chunks_splitting::prepare_execution_chunks(ctx, queue_type);

        if chunks.is_empty() {
            // No more chunks to process. Stopping.
            break;
        }

        for chunk in chunks {
            // IMPORTANT: check limits in the beginning of the loop,
            // because events and txs handling can already set the status to out of limits.
            // TODO: #5226 even if we run out of modifications limit, we still can process the programs,
            // which are already touched.
            let LimitsStatus::WithinLimits = ctx.limits_status() else {
                // If we are out of limits (gas, outgoing messages, call replies and etc.), stopping execution.
                break 'main_loop;
            };

            // Spawn on a separate thread an execution of each program (it's queue) in the chunk.
            let chunk_outputs =
                chunk_execution_spawn::spawn_chunk_execution(ctx, chunk, queue_type).await?;

            // Collect journals from all executed programs in the chunk.
            let (chunk_journals, max_gas_spent_in_chunk) =
                chunk_execution_processing::collect_chunk_journals(ctx, chunk_outputs);

            // Process journals of all executed programs in the chunk.
            match chunk_execution_processing::process_chunk_execution_journals(ctx, chunk_journals)
            {
                ChunkJournalsProcessingOutput::Processed => {}
                ChunkJournalsProcessingOutput::EarlyBreak => break 'main_loop,
            }

            // Charge global gas allowance counter with the maximum gas spent in the chunk.
            let charge_result = ctx
                .inner_mut()
                .gas_allowance_counter
                .charge(max_gas_spent_in_chunk);

            assert!(
                charge_result.is_enough(),
                "Gas allowance counter MUST be enough after charging with max gas spent in chunk"
            );
        }
    }

    log::trace!(
        "Finished processing queue type {queue_type:?} in chunks, limits status: {:?}",
        ctx.limits_status()
    );

    Ok(())
}

#[derive(Debug)]
pub(super) enum LimitsStatus {
    WithinLimits,
    OutOfGas,
    OutOfOutgoingMessages,
    OutOfOutgoingMessagesBytes,
    OutOfCallReplies,
    OutOfProgramModifications,
}

/// Context for running program queues in chunks.
///
/// Main responsibility of the trait is to maintain DRY principle
/// between common and overlaid execution contexts. It's not meant
/// to emphasize any particular trait/feature/abstraction.
pub(super) trait RunContext {
    fn program_code(&self, program_id: ActorId) -> Result<(InstrumentedCode, CodeMetadata)>;

    /// Get reference to inner.
    fn inner(&self) -> &CommonRunContext;

    /// Get mutable reference to inner.
    fn inner_mut(&mut self) -> &mut CommonRunContext;

    /// Get program states for the specified queue type.
    fn states(&self, queue_type: MessageType) -> Vec<ActorStateHashWithQueueSize>;

    /// Handle chunk data for a specific actor state.
    ///
    /// In common execution, the actor state is inserted into the chunks collection based
    /// on its queue size.
    /// In overlaid execution, the base program is always inserted into the heaviest chunk.
    ///
    /// The trait method provides a default implementation for a common execution.
    fn handle_chunk_data(
        &self,
        execution_chunks: &mut ExecutionChunks,
        actor_state: ActorStateHashWithQueueSize,
        queue_type: MessageType,
    ) {
        let ActorStateHashWithQueueSize {
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

    /// [`PromisePolicy`] tells processor should it emit promises or not.
    /// By default if [`RunContext::promise_out_tx`] returns [`Some`] this function will return [`PromisePolicy::Enabled`].
    fn promise_policy(&self) -> PromisePolicy {
        match self.inner().promise_out_tx.is_some() {
            true => PromisePolicy::Enabled,
            false => PromisePolicy::Disabled,
        }
    }

    fn journal_handler<'a>(
        &'a mut self,
        program_id: ActorId,
        message_type: MessageType,
        call_reply: bool,
    ) -> JournalHandler<'a, dyn CASDatabase + 'a> {
        let CommonRunContext {
            db,
            transitions,
            gas_allowance_counter,
            outgoing_messages_limiter,
            outgoing_messages_bytes_limiter,
            call_reply_limiter,
            out_of_gas,
            ..
        } = self.inner_mut();

        JournalHandler {
            program_id,
            message_type,
            call_reply,
            controller: TransitionController {
                storage: db.cas(),
                transitions,
            },
            gas_allowance_counter,
            chunk_gas_limit: CHUNK_PROCESSING_GAS_LIMIT,
            out_of_gas,
            outgoing_messages_limiter,
            outgoing_messages_bytes_limiter,
            call_reply_limiter,
        }
    }

    fn limits_status(&self) -> LimitsStatus {
        let CommonRunContext {
            transitions,
            outgoing_messages_limiter,
            outgoing_messages_bytes_limiter,
            call_reply_limiter,
            out_of_gas,
            ..
        } = self.inner();

        if *out_of_gas {
            LimitsStatus::OutOfGas
        } else if *outgoing_messages_limiter == 0 {
            LimitsStatus::OutOfOutgoingMessages
        } else if *outgoing_messages_bytes_limiter == 0 {
            LimitsStatus::OutOfOutgoingMessagesBytes
        } else if *call_reply_limiter == 0 {
            LimitsStatus::OutOfCallReplies
        } else if transitions.modifications_len() >= PROGRAM_MODIFICATIONS_SOFT_LIMIT as usize {
            LimitsStatus::OutOfProgramModifications
        } else {
            LimitsStatus::WithinLimits
        }
    }
}

/// Common run context.
pub(crate) struct CommonRunContext {
    pub(super) db: Database,
    pub(super) transitions: InBlockTransitions,
    instance_creator: InstanceCreator,
    gas_allowance_counter: GasAllowanceCounter,
    outgoing_messages_limiter: u32,
    outgoing_messages_bytes_limiter: u32,
    call_reply_limiter: u32,
    out_of_gas: bool,
    chunk_size: usize,
    block_header: BlockHeader,
    promise_out_tx: Option<mpsc::UnboundedSender<Promise>>,
}

impl CommonRunContext {
    pub(crate) fn new(
        db: Database,
        instance_creator: InstanceCreator,
        in_block_transitions: InBlockTransitions,
        gas_allowance: u64,
        chunk_size: usize,
        block_header: BlockHeader,
        promise_out_tx: Option<mpsc::UnboundedSender<Promise>>,
    ) -> Self {
        CommonRunContext {
            db,
            instance_creator,
            transitions: in_block_transitions,
            gas_allowance_counter: GasAllowanceCounter::new(gas_allowance),
            outgoing_messages_limiter: OUTGOING_MESSAGES_SOFT_LIMIT,
            outgoing_messages_bytes_limiter: OUTGOING_MESSAGES_BYTES_SOFT_LIMIT,
            call_reply_limiter: CALL_REPLY_SOFT_LIMIT,
            out_of_gas: false,
            chunk_size,
            block_header,
            promise_out_tx,
        }
    }

    fn disable_promises(&mut self) {
        if self.promise_out_tx.take().is_some() {
            log::trace!("dropping the promise sender");
        }
    }

    pub(crate) async fn run(mut self) -> Result<InBlockTransitions> {
        // Start with injected queues processing.
        run_for_queue_type(&mut self, MessageType::Injected).await?;

        if let LimitsStatus::WithinLimits = self.limits_status() {
            self.disable_promises();
            run_for_queue_type(&mut self, MessageType::Canonical).await?;
        }

        Ok(self.transitions)
    }
}

impl RunContext for CommonRunContext {
    fn program_code(&self, program_id: ActorId) -> Result<(InstrumentedCode, CodeMetadata)> {
        let code_id = self
            .transitions
            .registered_programs()
            .get(&program_id)
            .map(|code_id| Ok(*code_id))
            .unwrap_or_else(|| {
                self.db
                    .program_code_id(program_id)
                    .ok_or_else(|| ProcessorError::MissingCodeIdForProgram(program_id))
            })?;

        instrumented_code_and_metadata(&self.db, code_id)
    }

    fn states(&self, processing_queue_type: MessageType) -> Vec<ActorStateHashWithQueueSize> {
        states(&self.transitions, processing_queue_type)
    }

    fn inner(&self) -> &CommonRunContext {
        self
    }

    fn inner_mut(&mut self) -> &mut CommonRunContext {
        self
    }
}

pub(super) fn instrumented_code_and_metadata(
    db: &Database,
    code_id: CodeId,
) -> Result<(InstrumentedCode, CodeMetadata)> {
    db.instrumented_code(ethexe_runtime_common::VERSION, code_id)
        .and_then(|instrumented_code| {
            db.code_metadata(code_id)
                .map(|metadata| (instrumented_code, metadata))
        })
        .ok_or_else(|| ProcessorError::MissingInstrumentedCodeForProgram(code_id))
}

pub(super) fn states(
    in_block_transitions: &InBlockTransitions,
    processing_queue_type: MessageType,
) -> Vec<ActorStateHashWithQueueSize> {
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
            let actor_state = ActorStateHashWithQueueSize::new(actor_id, state);

            Some(actor_state)
        })
        .collect()
}

pub(super) mod chunks_splitting {
    use super::*;

    // An alias introduced for better readability of the chunks splitting steps.
    type Chunks = Vec<Vec<(ActorId, H256)>>;

    // `prepare_execution_chunks` is not exactly sorting (sorting usually `n*log(n)` this one is `O(n)`),
    // but rather partitioning into subsets (chunks) of programs with approximately similar queue sizes.
    pub(super) fn prepare_execution_chunks(
        ctx: &mut impl RunContext,
        queue_type: MessageType,
    ) -> Chunks {
        let states = ctx.states(queue_type);
        let mut execution_chunks = ExecutionChunks::new(ctx.inner().chunk_size, states.len());

        for state in states {
            ctx.handle_chunk_data(&mut execution_chunks, state, queue_type);
        }

        execution_chunks.arrange_execution_chunks(ctx)
    }

    /// A helper  struct to bundle actor id, state hash and queue size together
    /// for easier handling in chunk preparation.
    pub struct ActorStateHashWithQueueSize {
        pub actor_id: ActorId,
        pub hash: H256,
        pub canonical_queue_size: usize,
        pub injected_queue_size: usize,
    }

    impl ActorStateHashWithQueueSize {
        pub fn new(actor_id: ActorId, state: StateHashWithQueueSize) -> Self {
            Self {
                actor_id,
                hash: state.hash,
                canonical_queue_size: state.canonical_queue_size as usize,
                injected_queue_size: state.injected_queue_size as usize,
            }
        }
    }

    /// A helper struct to manage execution chunks during their preparation.
    pub struct ExecutionChunks {
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
        pub fn chunk_idx(&self, mq_size: usize) -> usize {
            // Simplest implementation of chunk partitioning '| 1 | 2 | 3 | 4 | ..'
            debug_assert_ne!(mq_size, 0);
            mq_size.min(self.chunks.len()) - 1
        }

        /// Inserts chunk execution data into the specified chunk index.
        pub fn insert_into(&mut self, idx: usize, actor_id: ActorId, hash: H256) {
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
        pub fn insert_into_heaviest(&mut self, actor_id: ActorId, hash: H256) {
            if let Some(chunk) = self.chunks.last_mut() {
                chunk.push((actor_id, hash));
            } else {
                panic!("Chunks are empty, cannot insert into heaviest chunk");
            }
        }

        /// Arranges execution chunks by merging uneven chunks and reversing their order,
        /// so the heaviest chunks are processed first.
        fn arrange_execution_chunks<R: RunContext>(self, ctx: &mut R) -> Chunks {
            self.chunks
                .into_iter()
                // Merge uneven chunks
                .flatten()
                // Repartition chunks in reverse order to ensure all chunks have an equal number of elements
                .rev()
                .chunks(self.chunk_size)
                // Convert into vector of vectors
                .into_iter()
                .map(|c| {
                    c.into_iter()
                        // `check_task_no_run` function isn't used in a common execution, but it's used only for an overlaid one.
                        // The function is intended to nullify program queues only once before execution. If the queue wasn't nullified
                        // earlier the function will nullify it and skip spawning the job for the program queue as it's empty. If the queue
                        // was already nullified, the function will return `false` and the job will be spawned as usual.
                        // For more info, see impl of the [`OverlaidContext`].
                        .filter(|&(program_id, _)| !ctx.check_task_no_run(program_id))
                        .collect()
                })
                .collect()
        }
    }
}

pub(crate) mod chunk_execution_spawn {
    use super::*;
    use crate::{THREAD_POOL, host::InstanceWrapper};
    use ethexe_runtime_common::ProcessQueueContext;
    use itertools::Either;

    pub struct Executable {
        queue_type: MessageType,
        block_info: BlockInfo,
        promise_policy: PromisePolicy,
        program_id: ActorId,
        state_hash: H256,
        instrumented_code: InstrumentedCode,
        code_metadata: CodeMetadata,
        executor: InstanceWrapper,
        db: Box<dyn CASDatabase>,
        gas_allowance_for_chunk: u64,
        promise_out_tx: Option<mpsc::UnboundedSender<Promise>>,
    }

    pub fn execute_chunk_item(executable: Executable) -> Result<ChunkItemOutput> {
        let Executable {
            queue_type,
            block_info,
            promise_policy,
            program_id,
            state_hash,
            instrumented_code,
            code_metadata,
            mut executor,
            db,
            gas_allowance_for_chunk,
            promise_out_tx,
        } = executable;

        let (jn, new_state_hash, gas_spent) = executor.run(
            db,
            ProcessQueueContext {
                program_id,
                state_root: state_hash,
                queue_type,
                instrumented_code,
                code_metadata,
                gas_allowance: GasAllowanceCounter::new(gas_allowance_for_chunk),
                block_info,
                promise_policy,
            },
            promise_out_tx,
        )?;
        Ok((program_id, new_state_hash, jn, gas_spent))
    }

    /// An alias introduced for better readability of the chunks execution steps.
    pub type ChunkItemOutput = (ActorId, H256, ProgramJournals, u64);

    /// Spawns in the thread pool tasks for each program in the chunk remembering position of the program in the chunk.
    ///
    /// Each program receives one (same copy) value of gas allowance, because all programs in the chunk are executed in parallel.
    /// It means that in the same time unit (!) all programs simultaneously charge gas allowance. If programs were to be
    /// executed concurrently, then each of the program should have received a reference to the global gas allowance counter
    /// and charge gas from it concurrently.
    pub(super) async fn spawn_chunk_execution(
        ctx: &mut impl RunContext,
        chunk: Vec<(ActorId, H256)>,
        queue_type: MessageType,
    ) -> Result<Vec<ChunkItemOutput>> {
        let gas_allowance_for_chunk = ctx
            .inner()
            .gas_allowance_counter
            .left()
            .min(CHUNK_PROCESSING_GAS_LIMIT);

        let promise_policy = ctx.promise_policy();

        let block_header = ctx.inner().block_header;
        let block_info = BlockInfo {
            height: block_header.height,
            timestamp: block_header.timestamp,
        };

        let executables = chunk
            .into_iter()
            .map(|(program_id, state_hash)| {
                let (instrumented_code, code_metadata) = ctx.program_code(program_id)?;

                let executor = ctx.inner().instance_creator.instantiate()?;

                Ok(Either::Left(Executable {
                    queue_type,
                    block_info,
                    promise_policy,
                    program_id,
                    state_hash,
                    instrumented_code,
                    code_metadata,
                    executor,
                    db: ctx.inner().db.cas().clone_boxed(),
                    gas_allowance_for_chunk,
                    promise_out_tx: ctx.inner().promise_out_tx.clone(),
                }))
            })
            .collect::<Result<Vec<_>>>()?;

        THREAD_POOL
            .spawn_many(executables)
            .map(|output| output.unwrap_left())
            .try_collect()
            .await
    }
}

mod chunk_execution_processing {
    use super::*;
    use crate::handling::run::chunk_execution_spawn::ChunkItemOutput;

    // Aliases introduced for better readability of the chunk journals processing steps.
    type ProgramChunkJournals = (ActorId, ChunkJournals);
    type ChunkJournals = Vec<ExtendedJournal>;
    type ExtendedJournal = (Vec<JournalNote>, MessageType, bool);

    /// Output of the chunk journals processing step.
    ///
    /// Chunk journals processing is actually a loop, which can break early.
    /// The early break must also stop other steps of the caller chunk processing
    /// function. So to expose the logic in a clear way, the enum is introduced.
    pub enum ChunkJournalsProcessingOutput {
        Processed,
        EarlyBreak,
    }

    /// Collects journals from all executed programs in the chunk.
    ///
    /// The [`chunk_execution_spawn::spawn_chunk_execution`] step spawns tasks for each program in the chunk.
    /// The loop in the functions handles the output of each task:
    /// - modifies the state by setting a new state hash calculated by the [`ethexe_runtime_common::RuntimeJournalHandler`]
    /// - collects journals for later processing
    /// - tracks the maximum gas spent among all programs in the chunk
    ///
    /// Due to the nature of the parallel program queues execution (see [`chunk_execution_spawn::spawn_chunk_execution`] gas allowance clarifications),
    /// the actual gas allowance spent is actually the maximum among all programs in the chunk, not the sum.
    pub fn collect_chunk_journals(
        ctx: &mut impl RunContext,
        chunk_outputs: Vec<ChunkItemOutput>,
    ) -> (Vec<ProgramChunkJournals>, u64) {
        let mut max_gas_spent_in_chunk = 0u64;

        let chunk_journals = chunk_outputs
            .into_iter()
            .map(
                |(program_id, new_state_hash, program_journals, gas_spent)| {
                    // Handle state updates that occurred during journal processing within the runtime (allocations, pages).
                    // This should happen before processing the journal notes because `send_dispatch` from another program can modify the state.
                    ctx.inner_mut().transitions.modify(program_id, |state, _| {
                        state.hash = new_state_hash;
                    });

                    max_gas_spent_in_chunk = max_gas_spent_in_chunk.max(gas_spent);

                    (program_id, program_journals)
                },
            )
            .collect();

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
    pub fn process_chunk_execution_journals(
        ctx: &mut impl RunContext,
        chunk_journals: Vec<ProgramChunkJournals>,
    ) -> ChunkJournalsProcessingOutput {
        for (program_id, program_journals) in chunk_journals {
            for (journal, message_type, call_reply) in program_journals {
                let break_flag = ctx.break_early(&journal);

                let mut journal_handler = ctx.journal_handler(program_id, message_type, call_reply);
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
    use crate::{handling::overlaid::OverlaidRunContext, host};
    use ethexe_common::{MaybeHashOf, StateHashWithQueueSize, gear::MessageType};
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

        let states = std::iter::repeat_with(|| {
            i += 1;
            let hash = H256::from_low_u64_le(i);
            let canonical_queue_size = rand::random::<u8>() % MAX_QUEUE_SIZE + 1;
            states_to_queue_size.insert(hash, canonical_queue_size as usize);

            (
                ActorId::from(i),
                StateHashWithQueueSize {
                    hash,
                    canonical_queue_size,
                    injected_queue_size: 0,
                },
            )
        })
        .take(STATE_SIZE)
        .collect();

        let transitions = InBlockTransitions::new(0, states, Default::default());

        let mut ctx = CommonRunContext::new(
            Database::memory(),
            InstanceCreator::new(host::runtime()).unwrap(),
            transitions,
            1_000_000,
            CHUNK_PROCESSING_THREADS,
            BlockHeader::dummy(3),
            None,
        );

        let chunks = chunks_splitting::prepare_execution_chunks(&mut ctx, MessageType::Canonical);

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
        let db = Database::memory();

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

        let mut in_block_transitions = InBlockTransitions::new(3, states, Default::default());

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

        let mut overlaid_ctx = OverlaidRunContext::new(
            db.clone(),
            base_program,
            in_block_transitions,
            100,
            16,
            InstanceCreator::new(host::runtime()).unwrap(),
            BlockHeader::dummy(3),
        );
        access_state(
            pid2,
            &mut overlaid_ctx.inner_mut().transitions,
            &db,
            |state, storage, _| {
                let mut queue = state
                    .canonical_queue
                    .query(storage)
                    .expect("Failed to read queue for pid2");
                assert_eq!(queue.len(), 1);

                let dispatch = queue.dequeue().expect("pid2 queue has 1 dispatch");
                assert_eq!(dispatch.id, pid2_overlay_mid2);
            },
        );

        assert!(overlaid_ctx.nullify_queue(pid1));
        access_state(
            pid1,
            &mut overlaid_ctx.inner_mut().transitions,
            &db,
            |state, storage, _| {
                let queue = state
                    .canonical_queue
                    .query(storage)
                    .expect("Failed to read queue for pid1");
                assert_eq!(queue.len(), 0);
            },
        );
    }
}
