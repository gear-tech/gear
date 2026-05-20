// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use ethexe_db::{CASDatabase, Database};
use ethexe_runtime_common::{InBlockTransitions, TransitionController, state::ProgramState};
use gprimitives::ActorId;

pub(crate) mod events;
pub(crate) mod overlaid;
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

    fn controller(&mut self) -> TransitionController<'_, dyn CASDatabase + '_> {
        TransitionController {
            storage: self.db.cas(),
            transitions: &mut self.transitions,
        }
    }

    /// A wrapper for the lower level [`TransitionController::update_state`].
    fn update_state<T>(
        &mut self,
        program_id: ActorId,
        f: impl FnOnce(&mut ProgramState, &(dyn CASDatabase + '_), &mut InBlockTransitions) -> T,
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
