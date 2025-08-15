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

use core_processor::common::JournalNote;
use ethexe_db::Database;
use ethexe_runtime_common::{InBlockTransitions, TransitionController};
use gprimitives::ActorId;
use std::collections::HashSet;

/// Overlay execution context.
///
/// The context nullifies queues and stores which programs have their queues nullified.
///
/// The nullification is an optimization for RPC overlay mode execution of the target dispatch.
/// It allows not to empty unnecessary queues processing of not concerned programs.
pub(crate) struct OverlaidContext {
    db: Database,
    nullified_queue_programs: HashSet<ActorId>,
}

impl OverlaidContext {
    /// Creates a new `OverlaidContext`.
    ///
    /// Overlaid context is created with the base program's queue nullified retaining only the last
    /// message which by implicit contract is the target message for which the overlaid executor was created.
    pub(crate) fn new(
        base_program: ActorId,
        db: Database,
        transitions: &mut InBlockTransitions,
    ) -> Self {
        let mut transition_controller = TransitionController {
            transitions,
            storage: &db,
        };
        transition_controller.update_state(base_program, |state, _, _| {
            state.queue.modify_queue(&db, |queue| {
                log::warn!("Base program {base_program} queue will be nullified");
                log::warn!("Queue state - {:#?}", queue);
                // Last dispatch is the one for which overlaid executor was created.
                // Implicit invariant!
                let dispatch = queue
                    .pop_back()
                    .expect("last dispatch must be added before");
                queue.clear();
                queue.queue(dispatch);
                log::warn!("Queue state after - {:#?}", queue);
            });
        });

        let mut nullified_queue_programs = HashSet::new();
        nullified_queue_programs.insert(base_program);

        Self {
            db,
            nullified_queue_programs,
        }
    }

    /// Possibly nullifies the queue for the given program.
    ///
    /// The nullification is done only once per program.
    ///
    /// Returns `true` if the procedure successfully nullified the queue.
    /// If program's queue was already nullified, returns `false`.
    pub(crate) fn nullify_queue(
        &mut self,
        program_id: ActorId,
        transitions: &mut InBlockTransitions,
    ) -> bool {
        if self.nullified_queue_programs.contains(&program_id) {
            return false;
        }

        log::warn!("Nullifying queue for program {program_id}");
        let mut transition_controller = TransitionController {
            transitions,
            storage: &self.db,
        };
        transition_controller.update_state(program_id, |state, _, _| {
            state.queue.modify_queue(&self.db, |queue| {
                log::warn!("Queue state before nullification - {:#?}", queue);
                queue.clear();
                log::warn!("Queue state after nullification - {:#?}", queue);
            });
        });

        self.nullified_queue_programs.insert(program_id);

        true
    }

    /// Nullifies queues of all programs that are going to receive messages from the sender.
    ///
    /// The receiver program is obtained from `JournalNote::SendDispatch` of the journal,
    /// which belongs to the sender. More precisely, it's a journal created after executing
    /// one if dispatches of the sender's queue.
    pub(crate) fn nullify_receivers_queues(
        &mut self,
        journal: &[JournalNote],
        in_block_transitions: &mut InBlockTransitions,
    ) {
        for note in journal {
            let JournalNote::SendDispatch { dispatch, .. } = note else {
                continue;
            };

            let _ = self.nullify_queue(dispatch.destination(), in_block_transitions);
        }
    }
}
