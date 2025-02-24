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

//! # Bucketed parallel program execution
//!
//! ## Overview
//!
//! This approach speeds up the processing of multiple programs in parallel.
//! The main idea is to split programs into buckets based on their queue sizes or, in the future, another computation weight metric.
//!
//! Bucket processing helps reduce waiting time, as it minimizes the delay caused by the slowest message among all concurrently executed messages.
//! This works because, in sorted buckets, the computation time for each bucket element (queue messages) should be approximately equal.
//!
//! The second part of the approach is executing an entire program queue in one go within a single runtime instance.
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
//! where the "iter" column represents a computation step, and the "program" columns represent the queue sizes of each program:
//!
//! | iter | program 1 | program 2 | program 3 | ... | program N |
//! |-----:|----------:|----------:|----------:|----:|----------:|
//! |    0 |        10 |         1 |         5 | ... |         7 |
//! |    1 |         3 |         0 |         0 | ... |         0 |
//! |    2 |         3 |         1 |         1 | ... |         0 |
//! |    3 |         0 |         0 |         0 | ... |         0 |
//!
//! Before executing the programs, we need to split them into buckets.
//! The maximum bucket size is equal to the number of virtual threads.
//! The number of buckets is calculated as the total number of programs divided by the maximum bucket size.
//!
//! For example, given M buckets (virtual threads) and N program states, a sorted bucket structure will look like this:
//!
//! | bucket 0 | bucket 1 | bucket 2 | bucket 3 | ... | bucket M |
//! |---------:|---------:|---------:|---------:|----:|---------:|
//! |        9 |        7 |        4 |        3 | ... |        1 |
//! |        7 |        7 |        3 |        3 | ... |        1 |
//! |        8 |        6 |        4 |        3 | ... |        1 |
//! |       10 |        5 |        4 |        3 | ... |        1 |
//!
//! As you can see, the bucket contents are not strictly sorted, but this is not an issue.
//! We only need buckets with approximately equal queue sizes to ensure efficient parallel execution.
//! The entire queue is processed in a single runtime instance in one go, so prioritizing larger queues improves efficiency.
//!
//! Once all program queues have been processed, we deterministically merge journals and handle them.
//! After that, we repeat the process until no more messages remain
//! or we run out of processing time/gas allowance (to be implemented).
//!
//! **High-level overview of the algorithm:**
//!
//!   1. Split programs into buckets based on their queue sizes.
//!   2. Execute program queues in parallel using runtime instances per program.
//!   3. Merge journals and handle them deterministically.
//!   4. Repeat steps 1-3 until no messages are processed, or we run out of processing time/gas allowance.
//!
//! ---
//!
//! ## Simplest Bucket Splitting Algorithm
//!
//! A basic bucket-splitting algorithm is implemented as follows:
//!
//!   1. First, calculate a temporary bucket index based on the program queue size.
//!   2. Next, store the required data in a temporary bucket list.
//!   3. Repeat this process for all programs.
//!   4. Since the number of elements in each temporary bucket is random, they need to be redistributed
//!      to ensure all final buckets contain an equal number of elements.
//!      To achieve this, we first merge all temporary bucket lists sequentially
//!      and then redistribute the data according to the expected bucket size.
//!
//! ---
//!
//! ## Future Improvements
//!
//! Currently, the bucket partitioning algorithm is simple and does not consider a program’s execution time.
//! In the future, we could introduce a weight multiplier to the queue size to improve partitioning efficiency.
//! This weight multiplier could be calculated based on program execution time statistics.

use crate::host::{InstanceCreator, InstanceWrapper};
use ethexe_db::{CodesStorage, Database};
use ethexe_runtime_common::{
    InBlockTransitions, JournalHandler, ProgramJournals, TransitionController,
};
use gprimitives::{ActorId, H256};
use std::iter;

pub async fn run(
    virtual_threads: usize,
    db: Database,
    instance_creator: InstanceCreator,
    in_block_transitions: &mut InBlockTransitions,
) {
    let mut join_set = tokio::task::JoinSet::new();
    let max_bucket_size = virtual_threads;

    loop {
        let mut no_message_processed = true;

        let buckets = split_to_buckets(
            virtual_threads,
            &in_block_transitions
                .states_iter()
                .filter_map(|(actor_id, state)| {
                    if state.cached_queue_size == 0 {
                        return None;
                    }

                    Some((*actor_id, state.hash, state.cached_queue_size as usize))
                })
                .collect(),
        );

        for bucket in buckets.chunks(max_bucket_size) {
            for (task_num, (program_id, state_hash, _)) in bucket.iter().enumerate() {
                let db = db.clone();
                let mut executor = instance_creator
                    .instantiate()
                    .expect("Failed to instantiate executor");
                let program_id = *program_id;
                let state_hash = *state_hash;

                let _ = join_set.spawn_blocking(move || {
                    let (jn, new_state_hash) =
                        run_runtime(db, &mut executor, program_id, state_hash);
                    (task_num, program_id, new_state_hash, jn)
                });
            }

            let bucket_size = bucket.len();
            let mut handler = DeterministicJournalHandler::new(bucket_size);

            while let Some((task_num, program_id, new_state_hash, program_journals)) = join_set
                .join_next()
                .await
                .transpose()
                .expect("Failed to join task")
            {
                // State was updated during journal handling inside the runtime (allocations, pages)
                in_block_transitions.modify(program_id, |state, _| {
                    state.hash = new_state_hash;
                });

                handler.set_journal_part(task_num, program_id, program_journals);
                handler.try_handle_journal_part(
                    &db,
                    in_block_transitions,
                    &mut no_message_processed,
                );
            }
        }

        if no_message_processed {
            break;
        }
    }
}

fn split_to_buckets(
    virtual_threads: usize,
    states: &Vec<(ActorId, H256, usize)>,
) -> Vec<(ActorId, H256, usize)> {
    fn bucket_idx(queue_size: usize, number_of_buckets: usize) -> usize {
        // Simplest implementation of bucket partitioning '..1| 2 | 3 | 4 ..'
        queue_size.clamp(1, number_of_buckets) - 1
    }

    let max_size_of_bucket = virtual_threads;
    let number_of_buckets = states.len().div_ceil(max_size_of_bucket);

    let mut buckets = Vec::from_iter(iter::repeat_n(Vec::new(), number_of_buckets));

    for (actor_id, state_hash, queue_size) in states {
        let bucket_idx = bucket_idx(*queue_size, number_of_buckets);
        buckets[bucket_idx].push((*actor_id, *state_hash, *queue_size));
    }

    buckets.into_iter().flatten().rev().collect()
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

struct DeterministicJournalHandler {
    mega_journal: Vec<Option<(ActorId, ProgramJournals)>>,
    current_idx: usize,
}

impl DeterministicJournalHandler {
    fn new(bucket_size: usize) -> Self {
        Self {
            mega_journal: Vec::from_iter(std::iter::repeat_n(None, bucket_size)),
            current_idx: bucket_size - 1,
        }
    }

    fn set_journal_part(
        &mut self,
        idx: usize,
        program_id: ActorId,
        program_journals: ProgramJournals,
    ) {
        self.mega_journal[idx] = Some((program_id, program_journals));
    }

    fn try_handle_journal_part(
        &mut self,
        db: &Database,
        in_block_transitions: &mut InBlockTransitions,
        no_message_processed: &mut bool,
    ) {
        let start_idx = self.current_idx;

        // Traverse mega_journal in reverse order, because smaller buckets likely to finish first
        for idx in (0..=start_idx).rev() {
            let Some((program_id, program_journals)) = self.mega_journal[idx].take() else {
                // Can't proceed journal processing, need to wait till `idx` part of journal is ready
                self.current_idx = idx;
                return;
            };

            if !program_journals.is_empty() {
                *no_message_processed = false;

                for (journal, dispatch_origin) in program_journals {
                    let mut journal_handler = JournalHandler {
                        program_id,
                        dispatch_origin,
                        controller: TransitionController {
                            transitions: in_block_transitions,
                            storage: db,
                        },
                    };
                    core_processor::handle_journal(journal, &mut journal_handler);
                }
            }
        }
    }
}

impl Drop for DeterministicJournalHandler {
    fn drop(&mut self) {
        #[cfg(debug_assertions)]
        {
            let cnt = self.mega_journal.iter().fold(0usize, |accum, j| {
                if let Some(j) = j {
                    return accum + j.1.len();
                }
                accum
            });

            assert_eq!(cnt, 0, "Not all journal notes were processed");
        }
    }
}

#[cfg(test)]
mod tests {
    use gprimitives::ActorId;
    use itertools::Itertools;

    use super::*;

    #[test]
    fn it_test_bucket_partitioning() {
        const STATE_SIZE: usize = 1_000;
        const VIRT_THREADS_NUM: usize = 16;
        const MAX_QUEUE_SIZE: u8 = 20;

        let states = Vec::from_iter(
            std::iter::repeat_with(|| {
                (
                    ActorId::from(0),
                    H256::zero(),
                    (rand::random::<u8>() % MAX_QUEUE_SIZE + 1) as usize,
                )
            })
            .take(STATE_SIZE),
        );

        let buckets = split_to_buckets(VIRT_THREADS_NUM, &states);

        // Checking buckets partitioning
        let accum_buckets = buckets
            .iter()
            .chunks(VIRT_THREADS_NUM)
            .into_iter()
            .map(|bucket| bucket.fold(0, |acc, (_, _, queue_size)| acc + *queue_size))
            .collect::<Vec<_>>();

        for i in 0..accum_buckets.len() - 1 {
            assert!(
                accum_buckets[i] >= accum_buckets[i + 1],
                "Backets are not sorted"
            );
        }
    }
}
