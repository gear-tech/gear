// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
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
    ProcessorError, Result,
    handling::run::{
        self, CommonRunContext, RunContext,
        chunks_splitting::{ActorStateHashWithQueueSize, ExecutionChunks},
    },
    host::InstanceCreator,
};
use core_processor::common::JournalNote;
use ethexe_common::{BlockHeader, db::CodesStorageRO, gear::MessageType, injected::Promise};
use ethexe_db::{CASDatabase, Database};
use ethexe_runtime_common::{InBlockTransitions, TransitionController};
use gear_core::{
    code::{CodeMetadata, InstrumentedCode},
    gas::GasAllowanceCounter,
    message::ReplyDetails,
};
use gprimitives::{ActorId, MessageId};
use std::collections::HashSet;
use tokio::sync::mpsc;

/// Overlay execution context.
///
/// The context nullifies queues and stores which programs have their queues nullified.
///
/// The nullification is an optimization for RPC overlay mode execution of the target dispatch.
/// It allows not to empty unnecessary queues processing of not concerned programs.
pub(crate) struct OverlaidRunContext {
    inner: CommonRunContext,
    base_program: ActorId,
    nullified_queue_programs: HashSet<ActorId>,
}

impl OverlaidRunContext {
    pub(crate) fn new(
        db: Database,
        base_program: ActorId,
        mut transitions: InBlockTransitions,
        gas_allowance: u64,
        chunk_size: usize,
        instance_creator: InstanceCreator,
        block_header: BlockHeader,
    ) -> Self {
        let mut transition_controller = TransitionController {
            transitions: &mut transitions,
            storage: &db,
        };
        transition_controller.update_state(base_program, |state, _, _| {
            state.canonical_queue.modify_queue(&db, |queue| {
                log::debug!("Base program {base_program} queue will be nullified");
                log::debug!("Queue state - {:#?}", queue);
                // Last dispatch is the one for which overlaid executor was created.
                // Implicit invariant!
                let dispatch = queue
                    .pop_back()
                    .expect("last dispatch must be added before");
                queue.clear();
                queue.queue(dispatch);
                log::debug!("Queue state after - {:#?}", queue);
            });
        });

        Self {
            inner: CommonRunContext::new(
                db,
                instance_creator,
                transitions,
                gas_allowance,
                chunk_size,
                block_header,
            ),
            base_program,
            nullified_queue_programs: [base_program].into_iter().collect(),
        }
    }

    pub(crate) async fn run(
        mut self,
        promise_sender: Option<mpsc::UnboundedSender<Promise>>,
    ) -> Result<InBlockTransitions> {
        let _ = run::run_for_queue_type(&mut self, MessageType::Canonical, promise_sender).await?;
        Ok(self.inner.transitions)
    }

    /// Nullifies queues of dispatches receivers in case there is no reply to the base message.
    /// Otherwise, flags for early break.
    ///
    /// Returns `true` if early break is needed, `false` otherwise.
    ///
    /// Reply to the base message is checked by looking for a reply message to the message with `MessageId::zero()`.
    /// By contract, a message with `MessageId::zero()` is the one sent in overlay execution
    /// to calculate the reply for the program's `handle` function.
    pub(crate) fn nullify_or_break_early(&mut self, journal: &[JournalNote]) -> bool {
        let mut ret = false;

        // Possibly flag for early break.
        for note in journal {
            if let JournalNote::SendDispatch { dispatch, .. } = note
                && let Some((mid, _)) = dispatch.reply_details().map(ReplyDetails::into_parts)
                && mid == MessageId::zero()
            {
                ret = true;
            }
        }

        if !ret {
            self.nullify_receivers_queues(journal);
        }

        ret
    }

    /// Nullifies queues of all programs that are going to receive messages from the sender.
    ///
    /// The receiver program is obtained from `JournalNote::SendDispatch` of the journal,
    /// which belongs to the sender. More precisely, it's a journal created after executing
    /// one if dispatches of the sender's queue.
    fn nullify_receivers_queues(&mut self, journal: &[JournalNote]) {
        for note in journal {
            let JournalNote::SendDispatch { dispatch, .. } = note else {
                continue;
            };

            let _ = self.nullify_queue(dispatch.destination());
        }
    }

    /// Possibly nullifies the queue for the given program.
    ///
    /// The nullification is done only once per program.
    ///
    /// Returns `true` if the procedure successfully nullified the queue.
    /// If program's queue was already nullified or `program_id` is user, returns `false`.
    pub(crate) fn nullify_queue(&mut self, program_id: ActorId) -> bool {
        if self.nullified_queue_programs.contains(&program_id) {
            return false;
        }

        log::debug!("Nullifying queue for program {program_id}");
        let mut transition_controller = TransitionController {
            transitions: &mut self.inner.transitions,
            storage: &self.inner.db,
        };
        transition_controller.update_state(program_id, |state, _, _| {
            state.canonical_queue.modify_queue(&self.inner.db, |queue| {
                log::debug!("Queue state before nullification - {:#?}", queue);
                queue.clear();
                log::debug!("Queue state after nullification - {:#?}", queue);
            });
        });

        self.nullified_queue_programs.insert(program_id);

        true
    }
}

impl RunContext for OverlaidRunContext {
    fn instance_creator(&self) -> &InstanceCreator {
        &self.inner.instance_creator
    }

    fn block_header(&self) -> BlockHeader {
        self.inner.block_header
    }

    fn chunk_size(&self) -> usize {
        self.inner.chunk_size
    }

    fn program_code(&self, program_id: ActorId) -> Result<(InstrumentedCode, CodeMetadata)> {
        let code_id = self
            .inner
            .db
            .program_code_id(program_id)
            .ok_or_else(|| ProcessorError::MissingCodeIdForProgram(program_id))?;

        run::instrumented_code_and_metadata(&self.inner.db, code_id)
    }

    fn borrow_inner(
        &mut self,
    ) -> (
        &dyn CASDatabase,
        &mut InBlockTransitions,
        &mut GasAllowanceCounter,
    ) {
        (
            self.inner.db.cas(),
            &mut self.inner.transitions,
            &mut self.inner.gas_allowance_counter,
        )
    }

    fn states(&self, processing_queue_type: MessageType) -> Vec<ActorStateHashWithQueueSize> {
        run::states(&self.inner.transitions, processing_queue_type)
    }

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

        if self.base_program == actor_id {
            // Insert base program into heaviest chunk, which is going to be executed first.
            // This is done to get faster reply from the target dispatch for which overlaid
            // executor was created.
            execution_chunks.insert_into_heaviest(actor_id, hash);
        } else {
            let chunk_idx = execution_chunks.chunk_idx(queue_size);
            execution_chunks.insert_into(chunk_idx, actor_id, hash);
        }
    }

    fn check_task_no_run(&mut self, program_id: ActorId) -> bool {
        // If the queue wasn't nullified, the following call will nullify it and skip job spawning.
        self.nullify_queue(program_id)
    }

    fn break_early(&mut self, journal: &[JournalNote]) -> bool {
        self.nullify_or_break_early(journal)
    }
}
