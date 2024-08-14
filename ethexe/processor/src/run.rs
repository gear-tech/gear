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

use crate::{
    host::{InstanceCreator, InstanceWrapper},
    LocalOutcome,
};
use core_processor::common::JournalNote;
use ethexe_common::router::{OutgoingMessage, StateTransition};
use ethexe_db::CodesStorage;
use ethexe_runtime_common::Handler;
use gear_core::{
    ids::{ActorId, ProgramId},
    message::Message,
};
use gprimitives::H256;
use std::collections::BTreeMap;
use tokio::sync::{mpsc, oneshot};

enum Task {
    Run {
        program_id: ProgramId,
        state_hash: H256,
        result_sender: oneshot::Sender<Vec<JournalNote>>,
    },
    WakeMessages {
        program_id: ProgramId,
        state_hash: H256,
        result_sender: oneshot::Sender<H256>,
    },
}

pub fn run(
    threads_amount: usize,
    instance_creator: InstanceCreator,
    programs: &mut BTreeMap<ProgramId, H256>,
) -> (Vec<Message>, Vec<LocalOutcome>) {
    tokio::task::block_in_place(|| {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(threads_amount)
            .enable_all()
            .build()
            .unwrap();

        rt.block_on(async { run_in_async(instance_creator, programs).await })
    })
}

// TODO: Returning Vec<LocalOutcome> is a temporary solution.
// In future need to send all messages to users and all state hashes changes to sequencer.
async fn run_in_async(
    instance_creator: InstanceCreator,
    programs: &mut BTreeMap<ProgramId, H256>,
) -> (Vec<Message>, Vec<LocalOutcome>) {
    let mut to_users_messages = vec![];
    let mut results = BTreeMap::new();

    let num_workers = 4;

    let mut task_senders = vec![];
    let mut handles = vec![];

    // create workers
    for id in 0..num_workers {
        let (task_sender, task_receiver) = mpsc::channel(100);
        task_senders.push(task_sender);
        let handle = tokio::spawn(worker(id, instance_creator.clone(), task_receiver));
        handles.push(handle);
    }

    wake_messages(&task_senders, programs).await;

    loop {
        // Send tasks to process programs in workers, until all queues are empty.

        let mut no_more_to_do = true;
        for index in (0..programs.len()).step_by(num_workers) {
            let result_receivers = one_batch(index, &task_senders, programs).await;

            let mut super_journal = vec![];
            for (program_id, receiver) in result_receivers.into_iter() {
                let journal = receiver.await.unwrap();
                if !journal.is_empty() {
                    no_more_to_do = false;
                }
                super_journal.push((program_id, journal));
            }

            for (program_id, journal) in super_journal {
                let mut handler = Handler {
                    program_id,
                    program_states: programs,
                    storage: instance_creator.db(),
                    block_info: Default::default(),
                    results: Default::default(),
                    to_users_messages: Default::default(),
                };
                core_processor::handle_journal(journal, &mut handler);

                for (id, new_hash) in handler.results {
                    results.insert(id, (new_hash, vec![]));
                }

                for message in &handler.to_users_messages {
                    let entry = results.get_mut(&message.source()).expect("should be");
                    entry.1.push(message.clone());
                }

                to_users_messages.append(&mut handler.to_users_messages);
            }
        }

        if no_more_to_do {
            break;
        }
    }

    for handle in handles {
        handle.abort();
    }

    let outcomes = results
        .into_iter()
        .map(|(id, (new_state_hash, outgoing_messages))| {
            LocalOutcome::Transition(StateTransition {
                actor_id: id,
                new_state_hash,
                value_to_receive: 0,  // TODO (breathx): propose this
                value_claims: vec![], // TODO (breathx): propose this
                messages:
                    outgoing_messages
                        .into_iter()
                        .map(|message| {
                            let (
                                id,
                                _source,
                                destination,
                                payload,
                                _gas_limit,
                                value,
                                message_details,
                            ) = message.into_parts();

                            let reply_details =
                                message_details.and_then(|details| details.to_reply_details());

                            OutgoingMessage {
                                id,
                                destination,
                                payload: payload.into_vec(),
                                value,
                                reply_details,
                            }
                        })
                        .collect(),
            })
        })
        .collect();

    (to_users_messages, outcomes)
}

async fn run_task(executor: &mut InstanceWrapper, task: Task) {
    match task {
        Task::Run {
            program_id,
            state_hash,
            result_sender,
        } => {
            let code_id = executor
                .db()
                .program_code_id(program_id)
                .expect("Code ID must be set");

            let instrumented_code = executor
                .db()
                .instrumented_code(ethexe_runtime::VERSION, code_id);

            let journal = executor
                .run(program_id, code_id, state_hash, instrumented_code)
                .expect("Some error occurs while running program in instance");

            result_sender.send(journal).unwrap();
        }
        Task::WakeMessages {
            program_id,
            state_hash,
            result_sender,
        } => {
            let new_state_hash = executor
                .wake_messages(program_id, state_hash)
                .expect("Some error occurs while waking messages");
            result_sender.send(new_state_hash).unwrap();
        }
    }
}

async fn worker(
    id: usize,
    instance_creator: InstanceCreator,
    mut task_receiver: mpsc::Receiver<Task>,
) {
    log::trace!("Worker {} started", id);

    let mut executor = instance_creator
        .instantiate()
        .expect("Failed to instantiate executor");

    while let Some(task) = task_receiver.recv().await {
        run_task(&mut executor, task).await;
    }
}

async fn one_batch(
    from_index: usize,
    task_senders: &[mpsc::Sender<Task>],
    programs: &mut BTreeMap<ActorId, H256>,
) -> BTreeMap<ProgramId, oneshot::Receiver<Vec<JournalNote>>> {
    let mut result_receivers = BTreeMap::new();

    for (sender, (program_id, state_hash)) in
        task_senders.iter().zip(programs.iter().skip(from_index))
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

async fn wake_messages(
    task_senders: &[mpsc::Sender<Task>],
    programs: &mut BTreeMap<ProgramId, H256>,
) {
    let mut result_receivers = vec![];
    for (task_sender, (&program_id, &state_hash)) in
        task_senders.iter().cycle().zip(programs.iter())
    {
        let (result_sender, result_receiver) = oneshot::channel();
        task_sender
            .send(Task::WakeMessages {
                program_id,
                state_hash,
                result_sender,
            })
            .await
            .unwrap();
        result_receivers.push((program_id, result_receiver));
    }

    for (program_id, result_receiver) in result_receivers {
        let new_state_hash = result_receiver.await;
        programs.insert(program_id, new_state_hash.unwrap());
    }
}
