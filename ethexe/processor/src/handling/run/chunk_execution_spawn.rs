// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Chunk execution spawning logic.
//!
//! This module handles spawning program execution tasks in a thread pool.

use super::*;
use crate::thread_pool;
use ethexe_runtime_common::ProcessQueueContext;
use futures::stream::FuturesOrdered;

/// An alias introduced for better readability of the chunks execution steps.
pub type ChunkItemOutput = (ActorId, H256, ProgramJournals, u64);

/// Prepared input for executing one program queue in a chunk.
pub struct ChunkItemInput {
    pub program_id: ActorId,
    pub state_hash: H256,
    pub instrumented_code: InstrumentedCode,
    pub code_metadata: CodeMetadata,
}

/// Spawns in the thread pool tasks for each program in the chunk remembering position of the program in the chunk.
///
/// Each program receives one (same copy) value of gas allowance, because all programs in the chunk are executed in parallel.
/// It means that in the same time unit (!) all programs simultaneously charge gas allowance. If programs were to be
/// executed concurrently, then each of the program should have received a reference to the global gas allowance counter
/// and charge gas from it concurrently.
pub async fn spawn_chunk_execution(
    ctx: &mut impl RunContext,
    chunk: Vec<ChunkItemInput>,
    queue_type: MessageType,
) -> Result<Vec<ChunkItemOutput>> {
    let gas_allowance_for_chunk = ctx
        .inner()
        .gas_allowance_counter
        .left()
        .min(CHUNK_PROCESSING_GAS_LIMIT);

    let promise_policy = ctx.promise_policy();

    let block_info = BlockInfo {
        height: ctx.inner().height,
        timestamp: ctx.inner().timestamp,
    };

    chunk
        .into_iter()
        .map(|chunk_item| {
            let ChunkItemInput {
                program_id,
                state_hash,
                instrumented_code,
                code_metadata,
            } = chunk_item;

            let mut executor = ctx.inner().instance_creator.instantiate()?;
            let promise_sink = ctx.inner().promise_sink.clone();
            Ok(thread_pool::spawn(move || {
                let (jn, new_state_hash, gas_spent) = executor.run(
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
                    promise_sink,
                )?;
                Ok((program_id, new_state_hash, jn, gas_spent))
            }))
        })
        .collect::<Result<FuturesOrdered<_>>>()?
        .try_collect()
        .await
}
