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

use crate::{
    host::{InstanceCreator, InstanceWrapper},
    ProcessorConfig,
};
use ethexe_db::{CodesStorage, Database};
use ethexe_runtime_common::{
    state::Storage, InBlockTransitions, JournalHandler, ProgramJournals, TransitionController,
};
use gprimitives::{ActorId, H256};
use std::iter;

pub async fn run(
    config: &ProcessorConfig,
    db: Database,
    instance_creator: InstanceCreator,
    in_block_transitions: &mut InBlockTransitions,
) {
    run_in_async(
        config.virtual_threads,
        db,
        instance_creator,
        in_block_transitions,
    )
    .await
}

// Splits to buckets by queue size
fn split_to_buckets(
    virtual_threads: usize,
    states: &Vec<(ActorId, H256, u8)>,
) -> Vec<(ActorId, H256, u8)> {
    fn bucket_idx(queue_size: usize, number_of_buckets: usize) -> usize {
        // Simplest implementation of bucket partitioning '..1| 2 | 3 | 4 ..'
        queue_size.clamp(1, number_of_buckets) - 1
    }

    let max_size_of_bucket = virtual_threads;
    let number_of_buckets = states.len().div_ceil(max_size_of_bucket);

    let mut buckets = Vec::from_iter(iter::repeat_n(Vec::new(), number_of_buckets));

    for (actor_id, state_hash, queue_size) in states {
        let bucket_idx = bucket_idx(*queue_size as usize, number_of_buckets);
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

async fn run_in_async(
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
                .filter_map(|(actor_id, state_hash)| {
                    let program_state = db.read_state(*state_hash).unwrap();

                    if program_state.queue_hash.is_empty() {
                        return None;
                    }

                    let queue_size = program_state.queue_hash.query(&db).unwrap().len();

                    Some((*actor_id, *state_hash, queue_size as u8))
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
                // State was updated during journal handling inside the runtime
                in_block_transitions.modify_state(program_id, new_state_hash);

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
                    (rand::random::<u8>() % MAX_QUEUE_SIZE + 1),
                )
            })
            .take(STATE_SIZE),
        );

        let buckets = split_to_buckets(VIRT_THREADS_NUM, &states);

        //println!();
        //for (_, _, queue_size) in &buckets {
        //    print!("{queue_size}, ");
        //}
        //println!();

        // Checking buckets partitioning
        let accum_buckets = buckets
            .iter()
            .chunks(VIRT_THREADS_NUM)
            .into_iter()
            .map(|bucket| bucket.fold(0, |acc, (_, _, queue_size)| acc + *queue_size as usize))
            .collect::<Vec<_>>();

        for i in 0..accum_buckets.len() - 1 {
            assert!(
                accum_buckets[i] >= accum_buckets[i + 1],
                "Backets are not sorted"
            );
        }
    }
}
