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

//! Chunk execution spawning logic.
//!
//! This module handles spawning program execution tasks in a thread pool.

use super::*;
use crate::{handling::thread_pool::ThreadPool, host::InstanceWrapper};
use ethexe_runtime_common::ProcessQueueContext;
use std::sync::LazyLock;

/// An alias introduced for better readability of the chunks execution steps.
pub type ChunkItemOutput = (ActorId, H256, ProgramJournals, u64);

/// Spawns in the thread pool tasks for each program in the chunk remembering position of the program in the chunk.
///
/// Each program receives one (same copy) value of gas allowance, because all programs in the chunk are executed in parallel.
/// It means that in the same time unit (!) all programs simultaneously charge gas allowance. If programs were to be
/// executed concurrently, then each of the program should have received a reference to the global gas allowance counter
/// and charge gas from it concurrently.
pub async fn spawn_chunk_execution(
    ctx: &mut impl RunContext,
    chunk: Vec<(ActorId, H256)>,
    queue_type: MessageType,
) -> Result<Vec<ChunkItemOutput>> {
    struct Executable {
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

    fn execute_chunk_item(executable: Executable) -> Result<ChunkItemOutput> {
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

    static THREAD_POOL: LazyLock<ThreadPool<Executable, Result<ChunkItemOutput>>> =
        LazyLock::new(|| ThreadPool::new(execute_chunk_item));

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

            Ok(Executable {
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
            })
        })
        .collect::<Result<Vec<_>>>()?;

    THREAD_POOL.spawn_many(executables).try_collect().await
}
