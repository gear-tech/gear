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

use ethexe_common::{db::CodesStorageRead, StateHashWithQueueSize};
use ethexe_db::Database;
use ethexe_runtime_common::{
    InBlockTransitions, JournalHandler, ProgramJournals, TransitionController,
};
use gear_core::ids::ActorId;
use gprimitives::H256;
use itertools::Itertools;
use tokio::task::JoinSet;

use crate::host::{InstanceCreator, InstanceWrapper};

pub async fn run(
    chunk_processing_threads: usize,
    db: Database,
    instance_creator: InstanceCreator,
    in_block_transitions: &mut InBlockTransitions,
) {
    let mut join_set = JoinSet::new();
    let chunk_size = chunk_processing_threads;

    loop {
        let states: Vec<_> = in_block_transitions
            .states_iter()
            .filter_map(|(&actor_id, &state)| {
                if state.cached_queue_size == 0 {
                    return None;
                }

                Some((actor_id, state))
            })
            .collect();

        let chunks = split_to_chunks(chunk_size, states);

        if chunks.is_empty() {
            break;
        }

        for chunk in chunks {
            for (program_id, state_hash, _) in chunk {
                let db = db.clone();
                let mut executor = instance_creator
                    .instantiate()
                    .expect("Failed to instantiate executor");

                let _ = join_set.spawn_blocking(move || {
                    let (jn, new_state_hash) =
                        run_runtime(db, &mut executor, program_id, state_hash);
                    (program_id, new_state_hash, jn)
                });
            }

            let mut chunk_journal = Vec::new();
            while let Some(result) = join_set
                .join_next()
                .await
                .transpose()
                .expect("Failed to join task")
            {
                chunk_journal.push(result);
            }

            for (program_id, new_state_hash, program_journals) in chunk_journal {
                // State was updated during journal handling inside the runtime (allocations, pages)
                in_block_transitions.modify(program_id, |state, _| {
                    state.hash = new_state_hash;
                });

                for (journal, dispatch_origin, call_reply) in program_journals {
                    let mut journal_handler = JournalHandler {
                        program_id,
                        dispatch_origin,
                        call_reply,
                        controller: TransitionController {
                            transitions: in_block_transitions,
                            storage: &db,
                        },
                    };
                    core_processor::handle_journal(journal, &mut journal_handler);
                }
            }
        }
    }
}

// `split_to_chunks` is not exactly sorting (sorting usually `n*log(n)` this one is `O(n)``),
// but rather partitioning into subsets (chunks) of programs with approximately similar queue sizes.
fn split_to_chunks(
    chunk_size: usize,
    states: Vec<(ActorId, StateHashWithQueueSize)>,
) -> Vec<Vec<(ActorId, H256, usize)>> {
    fn chunk_idx(queue_size: usize, number_of_chunks: usize) -> usize {
        // Simplest implementation of chunk partitioning '| 1 | 2 | 3 | 4 | ..'
        debug_assert_ne!(queue_size, 0);
        queue_size.min(number_of_chunks) - 1
    }

    let number_of_chunks = states.len().div_ceil(chunk_size);
    let mut chunks = vec![vec![]; number_of_chunks];

    for (
        actor_id,
        StateHashWithQueueSize {
            hash,
            cached_queue_size,
        },
    ) in states
    {
        let queue_size = cached_queue_size as usize;
        let chunk_idx = chunk_idx(queue_size, number_of_chunks);
        chunks[chunk_idx].push((actor_id, hash, queue_size));
    }

    chunks
        .into_iter()
        // Merge uneven chunks
        .flatten()
        // Repartition chunks in reverse order to ensure all chunks have an equal number of elements
        .rev()
        .chunks(chunk_size)
        // Convert into vector of vectors
        .into_iter()
        .map(|c| c.into_iter().collect())
        .collect()
}

fn run_runtime(
    db: Database,
    executor: &mut InstanceWrapper,
    program_id: ActorId,
    state_hash: H256,
) -> (ProgramJournals, H256) {
    let code_id = db.program_code_id(program_id).expect("Code ID must be set");

    let instrumented_code = db.instrumented_code(ethexe_runtime::VERSION, code_id);

    executor
        .run(db, program_id, code_id, state_hash, instrumented_code)
        .expect("Some error occurs while running program in instance")
}

#[cfg(test)]
mod tests {
    use gprimitives::ActorId;

    use super::*;

    #[test]
    fn chunk_partitioning() {
        const STATE_SIZE: usize = 1_000;
        const CHUNK_PROCESSING_THREADS: usize = 16;
        const MAX_QUEUE_SIZE: u8 = 20;

        let states = Vec::from_iter(
            std::iter::repeat_with(|| {
                (
                    ActorId::from(0),
                    StateHashWithQueueSize {
                        hash: H256::zero(),
                        cached_queue_size: (rand::random::<u8>() % MAX_QUEUE_SIZE + 1),
                    },
                )
            })
            .take(STATE_SIZE),
        );

        let chunks = split_to_chunks(CHUNK_PROCESSING_THREADS, states);

        // Checking chunks partitioning
        let accum_chunks = chunks
            .into_iter()
            .map(|chunk| {
                chunk
                    .into_iter()
                    .map(|(_, _, queue_size)| queue_size)
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
}
