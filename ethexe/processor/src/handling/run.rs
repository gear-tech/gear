// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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

use crate::host::{InstanceCreator, InstanceWrapper};
use core_processor::common::JournalNote;
use ethexe_db::{CodesStorage, Database};
use ethexe_runtime_common::{InBlockTransitions, JournalHandler, TransitionController};
use gear_core::ids::ProgramId;
use gprimitives::H256;
use std::collections::BTreeMap;
use tokio::sync::{mpsc, oneshot};

enum Task {
    Run {
        program_id: ProgramId,
        state_hash: H256,
        result_sender: oneshot::Sender<Vec<JournalNote>>,
    },
}

pub fn run(
    num_workers: usize,
    db: Database,
    instance_creator: InstanceCreator,
    in_block_transitions: &mut InBlockTransitions,
) {
    tokio::task::block_in_place(|| {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(num_workers)
            .enable_all()
            .build()
            .unwrap();

        rt.block_on(async {
            run_in_async(num_workers, db, instance_creator, in_block_transitions).await
        })
    })
}

// TODO: Returning Vec<LocalOutcome> is a temporary solution.
// In future need to send all messages to users and all state hashes changes to sequencer.
async fn run_in_async(
    num_workers: usize,
    db: Database,
    instance_creator: InstanceCreator,
    in_block_transitions: &mut InBlockTransitions,
) {
    let mut task_senders = vec![];
    let mut handles = vec![];

    // create workers
    for id in 0..num_workers {
        let (task_sender, task_receiver) = mpsc::channel(100);
        task_senders.push(task_sender);
        let handle = tokio::spawn(worker(
            id,
            db.clone(),
            instance_creator.clone(),
            task_receiver,
        ));
        handles.push(handle);
    }

    loop {
        // Send tasks to process programs in workers, until all queues are empty.

        let mut no_more_to_do = true;
        for index in (0..in_block_transitions.states_amount()).step_by(num_workers) {
            let result_receivers = one_batch(index, &task_senders, in_block_transitions).await;

            let mut super_journal = vec![];
            for (program_id, receiver) in result_receivers.into_iter() {
                let journal = receiver.await.unwrap();
                if !journal.is_empty() {
                    no_more_to_do = false;
                }
                super_journal.push((program_id, journal));
            }

            for (program_id, journal) in super_journal {
                let mut handler = JournalHandler {
                    program_id,
                    controller: TransitionController {
                        transitions: in_block_transitions,
                        storage: &db,
                    },
                };
                core_processor::handle_journal(journal, &mut handler);
            }
        }

        if no_more_to_do {
            break;
        }
    }

    for handle in handles {
        handle.abort();
    }
}

async fn run_task(db: Database, executor: &mut InstanceWrapper, task: Task) {
    match task {
        Task::Run {
            program_id,
            state_hash,
            result_sender,
        } => {
            let code_id = db.program_code_id(program_id).expect("Code ID must be set");

            let instrumented_code = db.instrumented_code(ethexe_runtime::VERSION, code_id);

            let journal = executor
                .run(db, program_id, code_id, state_hash, instrumented_code)
                .expect("Some error occurs while running program in instance");

            result_sender.send(journal).unwrap();
        }
    }
}

async fn worker(
    id: usize,
    db: Database,
    instance_creator: InstanceCreator,
    mut task_receiver: mpsc::Receiver<Task>,
) {
    log::trace!("Worker {} started", id);

    let mut executor = instance_creator
        .instantiate()
        .expect("Failed to instantiate executor");

    while let Some(task) = task_receiver.recv().await {
        run_task(db.clone(), &mut executor, task).await;
    }
}

async fn one_batch(
    from_index: usize,
    task_senders: &[mpsc::Sender<Task>],
    in_block_transitions: &mut InBlockTransitions,
) -> BTreeMap<ProgramId, oneshot::Receiver<Vec<JournalNote>>> {
    let mut result_receivers = BTreeMap::new();

    for (sender, (program_id, state_hash)) in task_senders
        .iter()
        .zip(in_block_transitions.states_iter().skip(from_index))
    {
        let (result_sender, result_receiver) = oneshot::channel();

        let task = Task::Run {
            program_id: *program_id,
            state_hash: *state_hash,
            result_sender,
        };

        sender.send(task).await.unwrap();

        result_receivers.insert(*program_id, result_receiver);
    }

    result_receivers
}
