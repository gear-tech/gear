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
    OverlaidProcessor, Processor, ProcessorConfig,
};
use core_processor::common::JournalNote;
use ethexe_common::gear::Origin;
use ethexe_db::{CodesStorage, Database};
use ethexe_runtime_common::{InBlockTransitions, JournalHandler, TransitionController};
use gear_core::{ids::ProgramId, message::DispatchKind};
use gprimitives::H256;
use std::collections::{BTreeMap, BTreeSet};
use tokio::sync::{mpsc, oneshot};

/// A ethexe-processir task unit.
enum Task {
    /// A task for processing message queue of the program with id `program_id`.
    /// The message queue is expected to be obtained form the storage using the `state_hash`.
    Run {
        program_id: ProgramId,
        state_hash: H256,
        result_sender: oneshot::Sender<(Vec<JournalNote>, Option<Origin>)>,
    },
}

pub fn run(processor: &Processor, in_block_transitions: &mut InBlockTransitions) {
    let config = processor.config();
    let db = processor.db.clone();
    let instance_creator = processor.creator.clone();

    run_impl(&config, db, instance_creator, in_block_transitions, None);
}

struct OverlayRunContext {
    nulled_queues: BTreeSet<ProgramId>,
    db: Database,
}

impl OverlayRunContext {
    fn new(
        base_program: ProgramId,
        db: Database,
        in_block_transitions: &mut InBlockTransitions,
    ) -> Self {
        let mut this = Self {
            nulled_queues: BTreeSet::new(),
            db,
        };

        this.nullify_queue_impl(in_block_transitions, base_program, true);

        this
    }

    /// Nullifies the queue of the program with id `program_id`.
    ///
    /// If the queue was nullified, it won't be nullified again.
    /// The nullification is done to increase speed of the overlaid execution as concerned programs
    /// can have large queues, which will slow down the processing for the reply from the main program
    /// dispatch.
    fn nullify_queue(
        &mut self,
        in_block_transitions: &mut InBlockTransitions,
        program_id: ProgramId,
    ) {
        self.nullify_queue_impl(in_block_transitions, program_id, false);
    }

    fn nullify_queue_impl(
        &mut self,
        in_block_transitions: &mut InBlockTransitions,
        program_id: ProgramId,
        is_base_program: bool,
    ) {
        log::warn!("Called nullifying queue for program {program_id}");
        if self.nulled_queues.contains(&program_id) {
            return;
        }
        log::warn!("Nullifying queue for program {program_id} will be executed");

        let mut transition_controller = TransitionController {
            transitions: in_block_transitions,
            storage: &self.db,
        };

        transition_controller.update_state(program_id, |state, _, _| {
            state.queue_hash.modify_queue(&self.db, |queue| {
                if is_base_program {
                    log::warn!("Base program queue will be nullified");
                    log::warn!("Queue state - {:#?}", queue);
                    // Last dispatch is the one for which overlaid executor was created.
                    let dispatch = queue
                        .pop_back()
                        .expect("last dispatch must be added before");
                    queue.clear();
                    queue.queue(dispatch);
                    log::warn!("Queue state after - {:#?}", queue);
                } else {
                    log::warn!("Non-base program {program_id} queue will be nullified");
                    log::warn!("Queue state - {:#?}", queue);
                    // Otherwise, we need to clear the queue.
                    queue.clear();
                    log::warn!("Queue state after - {:#?}", queue);
                }
            });
        });

        self.nulled_queues.insert(program_id);
    }
}

pub fn run_overlaid(
    processor: &OverlaidProcessor,
    in_block_transitions: &mut InBlockTransitions,
    base_pid: ProgramId,
) {
    let config = processor.0.config();
    let db = processor.0.db.clone();
    let instance_creator = processor.0.creator.clone();
    let overlay_ctx = OverlayRunContext::new(base_pid, db.clone(), in_block_transitions);

    run_impl(
        &config,
        db,
        instance_creator,
        in_block_transitions,
        Some(overlay_ctx),
    );
}

fn run_impl(
    config: &ProcessorConfig,
    db: Database,
    instance_creator: InstanceCreator,
    in_block_transitions: &mut InBlockTransitions,
    maybe_overlaid_context: Option<OverlayRunContext>,
) {
    tokio::task::block_in_place(|| {
        let mut rt_builder = tokio::runtime::Builder::new_multi_thread();

        if let Some(worker_threads) = config.worker_threads_override {
            rt_builder.worker_threads(worker_threads);
        };

        rt_builder.enable_all();

        let rt = rt_builder.build().unwrap();

        rt.block_on(run_in_async(
            config.virtual_threads,
            db,
            instance_creator,
            in_block_transitions,
            maybe_overlaid_context,
        ))
    })
}

async fn run_in_async(
    virtual_threads: usize,
    db: Database,
    instance_creator: InstanceCreator,
    in_block_transitions: &mut InBlockTransitions,
    mut maybe_overlaid_context: Option<OverlayRunContext>,
) {
    let mut task_senders = Vec::with_capacity(virtual_threads);
    let mut handles = Vec::with_capacity(virtual_threads);

    // create workers
    for id in 0..virtual_threads {
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
        for index in (0..in_block_transitions.states_amount()).step_by(virtual_threads) {
            let result_receivers = one_batch(
                index,
                &task_senders,
                in_block_transitions,
                maybe_overlaid_context.as_mut(),
            )
            .await;

            let mut super_journal = vec![];
            for (program_id, receiver) in result_receivers.into_iter() {
                let (journal, dispatch_origin) = receiver.await.unwrap();
                if !journal.is_empty() {
                    super_journal.push((
                        program_id,
                        dispatch_origin.expect("origin should be set for non-empty journal"),
                        journal,
                    ));
                    no_more_to_do = false;
                }
            }

            for (program_id, dispatch_origin, journal) in super_journal {
                // Overlaid execution case.
                //
                // The handling is required for the situations when current batch program sent messages
                // to the program, which will be executed later (it's in a further batch). If nullification
                // is not done here, then any newly sent dispatches to programs from further batches will
                // be nullified in the [`one_batch`] function.
                for note in &journal {
                    match note {
                        JournalNote::SendDispatch { dispatch, .. }
                            if dispatch.kind() == DispatchKind::Handle =>
                        {
                            if let Some(overlaid_ctx) = maybe_overlaid_context.as_mut() {
                                overlaid_ctx
                                    .nullify_queue(in_block_transitions, dispatch.destination());
                            }
                        }
                        _ => {}
                    }
                }

                let mut handler = JournalHandler {
                    program_id,
                    controller: TransitionController {
                        transitions: in_block_transitions,
                        storage: &db,
                    },
                    dispatch_origin,
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

/// A worker that processes [`Task`].
///
/// Basically, waits for a task to be sent from the sending end of the channel
/// and then runs it with a newly instantiated executor.
///
/// The worker is expected to be run in a separate thread, so the sending end
/// of the channel is used from the other (main) one.
/// Actually, the sending end is used when tasks are sent in batches (see [`one_batch`] function).
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

/// Inner implementation of the task processing done by the worker.
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

/// Sends a number of tasks to different workers ([`worker`]).
///
/// The result of the task is waited through the receiving end of the channel.
/// The sending end is given to a task processor, which is the [`worker`] itself.
///
/// The batch is, basically, a set of tasks created from the `in_block_transitions`, which
/// are sent to different workers. The size of the batch is maximum of a size of `task_senders`,
/// which itself has a size of `virtual_threads`.
///
/// Each time the function is called it is expected to have `from_index` value
/// to be updated, as it is used to skip already processed in block transitions.
/// The `from_index` argument is itself expected to be updated by the virtual
/// threads amount. So, if there are 3 virtual threads, then `from_index`
/// will be set to 0, 3, 6, 9, etc.
///
/// Returns a set of receivers, which are used to wait for the results of the tasks to be sent from [`worker`].
async fn one_batch(
    from_index: usize,
    task_senders: &[mpsc::Sender<Task>],
    in_block_transitions: &mut InBlockTransitions,
    mut maybe_overlaid_context: Option<&mut OverlayRunContext>,
) -> BTreeMap<ProgramId, oneshot::Receiver<(Vec<JournalNote>, Option<Origin>)>> {
    let mut result_receivers = BTreeMap::new();

    // This will modify `in_block_transitions` in case overlaid context was provided.
    let actors_to_process = in_block_transitions
        .states_iter()
        .skip(from_index)
        .map(|(program_id, _)| *program_id)
        .collect::<Vec<_>>();
    for program_id in actors_to_process {
        if let Some(overlay_ctx) = &mut maybe_overlaid_context {
            overlay_ctx.nullify_queue(in_block_transitions, program_id);
        }
    }

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

// todo [sab] test how many messages were executed in overlay
