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
use core_processor::common::JournalNote;
use ethexe_db::{CodesStorage, Database};
use ethexe_runtime_common::{
    state::Storage, InBlockTransitions, JournalHandler, TransitionController,
};
use gprimitives::{ActorId, H256};
use std::iter;

pub fn run(
    config: &ProcessorConfig,
    db: Database,
    instance_creator: InstanceCreator,
    in_block_transitions: &mut InBlockTransitions,
) {
    tokio::task::block_in_place(|| {
        let mut rt_builder = tokio::runtime::Builder::new_multi_thread();

        if let Some(worker_threads) = config.worker_threads_override {
            rt_builder.worker_threads(worker_threads);
        };

        rt_builder.enable_all();

        let rt = rt_builder.build().unwrap();

        rt.block_on(async {
            run_in_async(
                config.virtual_threads,
                db,
                instance_creator,
                in_block_transitions,
            )
            .await
        })
    })
}

// Splits to backets by queue size
fn split_to_buckets(
    virtual_threads: usize,
    states: &Vec<(ActorId, H256, u8)>,
) -> Vec<(ActorId, H256, u8)> {
    fn backet_idx(queue_size: usize, number_of_backets: usize) -> usize {
        // Simplest implementation of backet partitioning '..1| 2 | 3 | 4 ..'
        queue_size.clamp(1, number_of_backets) - 1
    }

    let max_size_of_backet = virtual_threads;
    // FIXME:
    let number_of_backets = (states.len() / max_size_of_backet) + 1;

    let mut backets = Vec::from_iter(iter::repeat_n(Vec::new(), number_of_backets));

    for (actor_id, state_hash, queue_size) in states {
        let backet_idx = backet_idx(*queue_size as usize, number_of_backets);
        backets[backet_idx].push((*actor_id, *state_hash, *queue_size));
    }

    backets.into_iter().flatten().rev().collect()
}

fn run_runtime(
    db: Database,
    executor: &mut InstanceWrapper,
    program_id: ActorId,
    state_hash: H256,
) -> (Vec<JournalNote>, H256) {
    let code_id = db.program_code_id(program_id).expect("Code ID must be set");

    let instrumented_code = db.instrumented_code(ethexe_runtime::VERSION, code_id);

    executor
        .run(db, program_id, code_id, state_hash, instrumented_code)
        .expect("Some error occurs while running program in instance")
}

struct DeterministicJournalHandler {
    mega_journal: Vec<Option<(ActorId, Vec<JournalNote>)>>,
    current_idx: usize,
    backet_size: usize,
}

impl DeterministicJournalHandler {
    fn new(backet_size: usize) -> Self {
        Self {
            mega_journal: Vec::from_iter(std::iter::repeat_n(None, backet_size)),
            current_idx: 0,
            backet_size,
        }
    }

    fn set_journal_part(
        &mut self,
        idx: usize,
        program_id: ActorId,
        journal_notes: Vec<JournalNote>,
    ) {
        self.mega_journal[idx] = Some((program_id, journal_notes));
    }

    // TODO: traverse mega_journal in reverse order
    fn try_handle_journal_part(
        &mut self,
        db: &Database,
        in_block_transitions: &mut InBlockTransitions,
        no_message_processed: &mut bool,
    ) {
        let start_idx = self.current_idx;

        for idx in start_idx..self.backet_size {
            let Some((program_id, journal_notes)) = self.mega_journal[idx].take() else {
                // Can't proceed journal processing, need to wait till `idx` part of journal is ready
                self.current_idx = idx;
                return;
            };

            if !journal_notes.is_empty() {
                *no_message_processed = false;

                let mut journal_handler = JournalHandler {
                    program_id,
                    controller: TransitionController {
                        transitions: in_block_transitions,
                        storage: db,
                    },
                };
                core_processor::handle_journal(journal_notes, &mut journal_handler);
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

            while let Some((task_num, program_id, new_state_hash, journal_notes)) = join_set
                .join_next()
                .await
                .transpose()
                .expect("Failed to join task")
            {
                // State was updated during journal handling inside the runtime
                in_block_transitions.modify_state(program_id, new_state_hash);

                handler.set_journal_part(task_num, program_id, journal_notes);
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
    fn it_test_backet_partitioning() {
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

        let backets = split_to_buckets(VIRT_THREADS_NUM, &states);

        //println!();
        //for (_, _, queue_size) in &backets {
        //    print!("{queue_size}, ");
        //}
        //println!();

        // Checking backets partitioning
        let accum_backets = backets
            .iter()
            .chunks(VIRT_THREADS_NUM)
            .into_iter()
            .map(|backet| backet.fold(0, |acc, (_, _, queue_size)| acc + *queue_size as usize))
            .collect::<Vec<_>>();

        for i in 0..accum_backets.len() - 1 {
            assert!(
                accum_backets[i] >= accum_backets[i + 1],
                "Backets are not sorted"
            );
        }
    }
}
