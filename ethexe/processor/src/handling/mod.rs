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

use ethexe_db::{CASDatabase, Database};
use ethexe_runtime_common::{InBlockTransitions, TransitionController, state::ProgramState};
use gprimitives::ActorId;

pub(crate) mod events;
mod overlaid;
pub(crate) mod run;

/// A high-level interface for executing ops,
/// which mutate states based on the current block request events.
///
/// This is based a wrapper which holds data needed to instantiate [`TransitionController`],
/// which itself performs recording actual state transitions.
pub struct ProcessingHandler {
    db: Database,

    #[cfg(not(test))]
    transitions: InBlockTransitions,

    #[cfg(test)]
    pub transitions: InBlockTransitions,
}

impl ProcessingHandler {
    pub fn new(db: Database, transitions: InBlockTransitions) -> Self {
        ProcessingHandler { db, transitions }
    }

    pub fn into_transitions(self) -> InBlockTransitions {
        self.transitions
    }

    fn controller(&mut self) -> TransitionController<'_, Box<dyn CASDatabase>> {
        TransitionController {
            storage: self.db.cas(),
            transitions: &mut self.transitions,
        }
    }

    /// A wrapper for the lower level [`TransitionController::update_state`].
    fn update_state<T>(
        &mut self,
        program_id: ActorId,
        f: impl FnOnce(&mut ProgramState, &Box<dyn CASDatabase>, &mut InBlockTransitions) -> T,
    ) -> T {
        self.controller().update_state(program_id, f)
    }

    #[cfg(test)]
    #[track_caller]
    pub fn program_state(&mut self, program_id: ActorId) -> ProgramState {
        use ethexe_runtime_common::state::Storage;

        let state_hash = self
            .transitions
            .state_of(&program_id)
            .expect("Program not found")
            .hash;
        self.db
            .program_state(state_hash)
            .expect("Program state not found in DB")
    }
}
