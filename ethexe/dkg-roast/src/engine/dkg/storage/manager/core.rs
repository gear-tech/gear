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
use crate::engine::dkg::{
    DkgAction, DkgCompleted, DkgEvent, DkgResult, DkgState, DkgStateMachine, SessionConfig,
};
use anyhow::Result;
use ethexe_common::{
    Address,
    crypto::{DkgRound1, DkgRound2, DkgRound2Culprits, DkgSessionId},
    db::DkgSessionState,
};
use std::collections::HashMap;

/// Aggregated updates produced by the manager.
#[derive(Debug, Default)]
pub struct DkgManagerUpdates {
    pub session_states: Vec<(DkgSessionId, DkgSessionState)>,
    pub completions: Vec<DkgCompleted>,
}

/// Manager output: outbound actions plus persistence updates.
#[derive(Debug)]
pub struct DkgManagerOutput {
    pub actions: Vec<DkgAction>,
    pub updates: DkgManagerUpdates,
}

/// DKG Manager handles DKG sessions for different eras.
#[derive(Debug)]
pub struct DkgManager {
    /// Active DKG state machines per era
    state: ManagerState,
}

impl DkgManager {
    /// Create a new DKG manager for the local validator address.
    pub fn new(self_address: Address) -> Self {
        Self {
            state: ManagerState {
                sessions: HashMap::new(),
                self_address,
            },
        }
    }

    /// Start DKG for a new era.
    pub fn start_dkg(
        &mut self,
        era_index: u64,
        mut validators: Vec<Address>,
        threshold: u16,
    ) -> Result<DkgManagerOutput> {
        // Skip duplicate starts for the same era.
        if self.state.sessions.contains_key(&era_index) {
            return Ok(self.empty_output());
        }

        // Sort for deterministic identifiers and share indices.
        validators.sort();
        let config = SessionConfig {
            era_index,
            validators,
            threshold,
            self_address: self.state.self_address,
        };

        // Initialize and drive the state machine; apply local loopback.
        let mut state_machine = DkgStateMachine::new();
        let actions = state_machine.process_event(DkgEvent::Start(config))?;
        let actions = processing::apply_local_rounds(&self.state, &mut state_machine, actions)?;

        self.state.sessions.insert(era_index, state_machine);

        Ok(self.build_output(vec![era_index], actions))
    }

    /// Force restart DKG for an era, clearing any in-memory session and re-running.
    pub fn restart_dkg(
        &mut self,
        era_index: u64,
        validators: Vec<Address>,
        threshold: u16,
    ) -> Result<DkgManagerOutput> {
        self.state.sessions.remove(&era_index);
        self.start_dkg(era_index, validators, threshold)
    }

    /// Process Round1 package.
    pub fn process_round1(
        &mut self,
        from: Address,
        message: DkgRound1,
    ) -> Result<DkgManagerOutput> {
        let era = message.session.era;
        self.handle_event_for_era(
            era,
            DkgEvent::Round1 {
                from,
                message: Box::new(message),
            },
        )
    }

    /// Process Round2 packages.
    pub fn process_round2(
        &mut self,
        from: Address,
        message: DkgRound2,
    ) -> Result<DkgManagerOutput> {
        let era = message.session.era;
        self.handle_event_for_era(
            era,
            DkgEvent::Round2 {
                from,
                message: Box::new(message),
            },
        )
    }

    /// Process round2 culprits.
    pub fn process_round2_culprits(
        &mut self,
        from: Address,
        message: DkgRound2Culprits,
    ) -> Result<DkgManagerOutput> {
        let era = message.session.era;
        self.handle_event_for_era(era, DkgEvent::Round2Culprits { from, message })
    }

    /// Process complaint.
    pub fn process_complaint(
        &mut self,
        from: Address,
        message: ethexe_common::crypto::DkgComplaint,
    ) -> Result<DkgManagerOutput> {
        let era = message.session.era;
        self.handle_event_for_era(era, DkgEvent::Complaint { from, message })
    }

    /// Process justification.
    pub fn process_justification(
        &mut self,
        from: Address,
        message: ethexe_common::crypto::DkgJustification,
    ) -> Result<DkgManagerOutput> {
        let era = message.session.era;
        self.handle_event_for_era(era, DkgEvent::Justification { from, message })
    }

    /// Apply timeout ticks across all sessions.
    pub fn process_timeouts(&mut self) -> Result<DkgManagerOutput> {
        let actions = processing::collect_timeout_actions(&mut self.state)?;
        Ok(self.build_output(self.active_eras(), actions))
    }

    /// Get DKG state for an era.
    pub fn get_state(&self, era: u64) -> Option<&DkgState> {
        self.state.sessions.get(&era).map(|sm| sm.state())
    }

    /// Build an empty output when no actions are produced.
    fn empty_output(&self) -> DkgManagerOutput {
        DkgManagerOutput {
            actions: Vec::new(),
            updates: DkgManagerUpdates::default(),
        }
    }

    /// Builds output and snapshots state for touched eras.
    fn build_output(&self, eras: Vec<u64>, actions: Vec<DkgAction>) -> DkgManagerOutput {
        let updates = self.collect_updates(eras, actions.as_slice());
        DkgManagerOutput { actions, updates }
    }

    /// Applies an event to a single-era state machine.
    fn handle_event_for_era(&mut self, era: u64, event: DkgEvent) -> Result<DkgManagerOutput> {
        let actions = processing::apply_event(&mut self.state, era, event)?;
        Ok(self.build_output(vec![era], actions))
    }

    /// Returns the list of eras with active sessions.
    fn active_eras(&self) -> Vec<u64> {
        self.state.sessions.keys().copied().collect()
    }

    /// Snapshots session state and completed outputs for persistence.
    fn collect_updates(&self, eras: Vec<u64>, actions: &[DkgAction]) -> DkgManagerUpdates {
        let mut updates = DkgManagerUpdates::default();
        for era in eras {
            if let Some(state_machine) = self.state.sessions.get(&era) {
                let session_id = DkgSessionId { era };
                let state = state_machine.snapshot_state();
                updates.session_states.push((session_id, state));
            }
        }
        for action in actions {
            if let DkgAction::Complete(result) = action
                && let DkgResult::Success(completed) = result.as_ref()
            {
                updates.completions.push(completed.as_ref().clone());
            }
        }
        updates
    }
}
