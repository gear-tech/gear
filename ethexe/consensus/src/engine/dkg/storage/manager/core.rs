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

//! DKG Manager - Integration layer between consensus and DKG state machine

use super::{processing, types::ManagerState};
use crate::engine::{
    dkg::{DkgAction, DkgEvent, DkgResult, DkgState, DkgStateMachine, SessionConfig},
    storage::DkgStore,
};
use anyhow::Result;
#[cfg(test)]
use ethexe_common::crypto::{DkgPublicKeyPackage, DkgShare, DkgVssCommitment};
use ethexe_common::{
    Address,
    crypto::{DkgRound1, DkgRound2, DkgRound2Culprits, DkgSessionId},
};
use std::collections::HashMap;

/// DKG Manager handles DKG sessions for different eras
#[derive(Debug)]
pub struct DkgManager<DB> {
    /// Active DKG state machines per era
    state: ManagerState<DB>,
}

impl<DB> DkgManager<DB>
where
    DB: DkgStore,
{
    /// Create new DKG manager
    pub fn new(db: DB, self_address: Address) -> Self {
        Self {
            state: ManagerState {
                sessions: HashMap::new(),
                self_address,
                db,
            },
        }
    }

    /// Start DKG for a new era
    pub fn start_dkg(
        &mut self,
        era_index: u64,
        mut validators: Vec<Address>,
        threshold: u16,
    ) -> Result<Vec<DkgAction>> {
        // Check if already running
        if self.state.sessions.contains_key(&era_index) {
            return Ok(vec![]);
        }

        validators.sort();
        let config = SessionConfig {
            era_index,
            validators,
            threshold,
            self_address: self.state.self_address,
        };

        let mut state_machine = DkgStateMachine::new();
        let actions = state_machine.process_event(DkgEvent::Start(config))?;
        let actions = processing::apply_local_rounds(&self.state, &mut state_machine, actions)?;

        self.state.sessions.insert(era_index, state_machine);

        self.persist_session_state(era_index)?;
        self.persist_completion(actions.as_slice())?;

        Ok(actions)
    }

    /// Force restart DKG for an era, clearing any in-memory session and re-running.
    pub fn restart_dkg(
        &mut self,
        era_index: u64,
        validators: Vec<Address>,
        threshold: u16,
    ) -> Result<Vec<DkgAction>> {
        self.state.sessions.remove(&era_index);
        self.state
            .db
            .mutate_dkg_session_state(DkgSessionId { era: era_index }, |state| {
                state.completed = false;
            });
        self.start_dkg(era_index, validators, threshold)
    }

    /// Process Round1 package
    pub fn process_round1(&mut self, from: Address, message: DkgRound1) -> Result<Vec<DkgAction>> {
        let era = message.session.era;
        let actions = processing::process_round_event(
            &mut self.state,
            era,
            DkgEvent::Round1 {
                from,
                message: Box::new(message),
            },
        )?;
        self.persist_session_state(era)?;
        self.persist_completion(actions.as_slice())?;
        Ok(actions)
    }

    /// Process Round2 packages
    pub fn process_round2(&mut self, from: Address, message: DkgRound2) -> Result<Vec<DkgAction>> {
        let era = message.session.era;
        let actions = processing::process_round_event(
            &mut self.state,
            era,
            DkgEvent::Round2 {
                from,
                message: Box::new(message),
            },
        )?;
        self.persist_session_state(era)?;
        self.persist_completion(actions.as_slice())?;
        Ok(actions)
    }

    /// Process round2 culprits
    pub fn process_round2_culprits(
        &mut self,
        from: Address,
        message: DkgRound2Culprits,
    ) -> Result<Vec<DkgAction>> {
        let era = message.session.era;
        let actions = processing::process_round_event(
            &mut self.state,
            era,
            DkgEvent::Round2Culprits { from, message },
        )?;
        self.persist_session_state(era)?;
        self.persist_completion(actions.as_slice())?;
        Ok(actions)
    }

    /// Process complaint
    pub fn process_complaint(
        &mut self,
        from: Address,
        message: ethexe_common::crypto::DkgComplaint,
    ) -> Result<Vec<DkgAction>> {
        let era = message.session.era;
        let actions = processing::process_round_event(
            &mut self.state,
            era,
            DkgEvent::Complaint { from, message },
        )?;
        self.persist_session_state(era)?;
        self.persist_completion(actions.as_slice())?;
        Ok(actions)
    }

    /// Process justification
    pub fn process_justification(
        &mut self,
        from: Address,
        message: ethexe_common::crypto::DkgJustification,
    ) -> Result<Vec<DkgAction>> {
        let era = message.session.era;
        let actions = processing::process_round_event(
            &mut self.state,
            era,
            DkgEvent::Justification { from, message },
        )?;
        self.persist_session_state(era)?;
        self.persist_completion(actions.as_slice())?;
        Ok(actions)
    }

    pub fn process_timeouts(&mut self) -> Result<Vec<DkgAction>> {
        let actions = processing::process_timeouts(&mut self.state)?;
        for era in self.state.sessions.keys().copied().collect::<Vec<_>>() {
            self.persist_session_state(era)?;
        }
        self.persist_completion(actions.as_slice())?;
        Ok(actions)
    }

    /// Get DKG state for an era
    pub fn get_state(&self, era: u64) -> Option<&DkgState> {
        self.state.sessions.get(&era).map(|sm| sm.state())
    }

    /// Check if DKG is completed for an era
    pub fn is_completed(&self, era: u64) -> bool {
        self.state.db.dkg_completed(era)
    }

    /// Get public key package if DKG completed
    #[cfg(test)]
    #[allow(dead_code)]
    pub fn get_public_key_package(&self, era: u64) -> Option<DkgPublicKeyPackage> {
        self.state.db.public_key_package(era)
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub fn get_vss_commitment(&self, era: u64) -> Option<DkgVssCommitment> {
        self.state.db.dkg_vss_commitment(era)
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub fn get_dkg_share(&self, era: u64) -> Option<DkgShare> {
        self.state.db.dkg_share(era)
    }

    fn persist_session_state(&self, era: u64) -> Result<()> {
        let Some(state_machine) = self.state.sessions.get(&era) else {
            return Ok(());
        };
        let session_id = DkgSessionId { era };
        let state = state_machine.snapshot_state();
        self.state.db.set_dkg_session_state(session_id, state);
        Ok(())
    }

    fn persist_completion(&self, actions: &[DkgAction]) -> Result<()> {
        for action in actions {
            if let DkgAction::Complete(result) = action
                && let DkgResult::Success(completed) = result.as_ref()
            {
                let era = completed.share.era;
                self.state
                    .db
                    .set_public_key_package(era, completed.public_key_package.clone());
                self.state
                    .db
                    .set_dkg_key_package(era, completed.key_package.clone());
                self.state
                    .db
                    .set_dkg_vss_commitment(era, completed.vss_commitment.clone());
                self.state.db.set_dkg_share(completed.share.clone());
                self.state
                    .db
                    .mutate_dkg_session_state(DkgSessionId { era }, |state| {
                        state.completed = true;
                    });
            }
        }
        Ok(())
    }
}
