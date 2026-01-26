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

use super::{DkgAction, DkgManager, DkgState};
use crate::engine::dkg::storage::DkgManagerOutput;
use anyhow::{Result, anyhow};
use ethexe_common::{
    Address,
    crypto::{DkgRound1, DkgRound2, DkgRound2Culprits},
    db::OnChainStorageRO,
};

/// External inputs for the DKG engine (network + local triggers).
#[derive(Debug, Clone)]
pub enum DkgEngineEvent {
    Start {
        era: u64,
        validators: Vec<Address>,
        threshold: u16,
    },
    Round1 {
        from: Address,
        message: Box<DkgRound1>,
    },
    Round2 {
        from: Address,
        message: DkgRound2,
    },
    Round2Culprits {
        from: Address,
        message: DkgRound2Culprits,
    },
    Complaint {
        from: Address,
        message: ethexe_common::crypto::DkgComplaint,
    },
    Justification {
        from: Address,
        message: ethexe_common::crypto::DkgJustification,
    },
}

/// DKG engine wraps the manager and persistence layer.
#[derive(Debug)]
pub struct DkgEngine<DB> {
    manager: DkgManager,
    db: DB,
}

impl<DB> DkgEngine<DB>
where
    DB: super::super::storage::DkgStore + OnChainStorageRO,
{
    /// Creates a new DKG engine bound to a DB and local validator address.
    pub fn new(db: DB, self_address: Address) -> Self {
        Self {
            manager: DkgManager::new(self_address),
            db,
        }
    }

    /// Routes a DKG event through the manager and persists any outputs.
    pub fn handle_event(&mut self, event: DkgEngineEvent) -> Result<Vec<DkgAction>> {
        // Resolve the era to load state and apply recovery on errors.
        let era = match &event {
            DkgEngineEvent::Start { era, .. } => *era,
            DkgEngineEvent::Round1 { message, .. } => message.session.era,
            DkgEngineEvent::Round2 { message, .. } => message.session.era,
            DkgEngineEvent::Round2Culprits { message, .. } => message.session.era,
            DkgEngineEvent::Complaint { message, .. } => message.session.era,
            DkgEngineEvent::Justification { message, .. } => message.session.era,
        };

        // Dispatch into the manager, which drives the state machine.
        let result = match event {
            DkgEngineEvent::Start {
                era,
                validators,
                threshold,
            } => self.manager.start_dkg(era, validators, threshold),
            DkgEngineEvent::Round1 { from, message } => self.manager.process_round1(from, *message),
            DkgEngineEvent::Round2 { from, message } => self.manager.process_round2(from, message),
            DkgEngineEvent::Round2Culprits { from, message } => {
                self.manager.process_round2_culprits(from, message)
            }
            DkgEngineEvent::Complaint { from, message } => {
                self.manager.process_complaint(from, message)
            }
            DkgEngineEvent::Justification { from, message } => {
                self.manager.process_justification(from, message)
            }
        };

        // Persist outputs or attempt a restart when errors are recoverable.
        match result {
            Ok(output) => self.finish_output(output),
            Err(err) => {
                if let Ok(actions) = self.restart_from_storage(era) {
                    return Ok(actions);
                }
                Err(err)
            }
        }
    }

    /// Advances timeouts across active DKG sessions.
    pub fn tick_timeouts(&mut self) -> Result<Vec<DkgAction>> {
        let output = self.manager.process_timeouts()?;
        self.finish_output(output)
    }

    /// Restarts DKG for an era using validators from storage.
    pub fn restart_from_storage(&mut self, era: u64) -> Result<Vec<DkgAction>> {
        let Some(validators) = self.db.validators(era) else {
            return Err(anyhow!(
                "Unable to restart DKG for era {era}: validators missing"
            ));
        };
        let validators: Vec<_> = validators.into_iter().collect();
        let threshold = ((validators.len() as u64 * 2) / 3).max(1) as u16;
        self.reset_dkg_completion(era);
        let output = self.manager.restart_dkg(era, validators, threshold)?;
        self.finish_output(output)
    }

    /// Restarts DKG for an era with an explicit validator set.
    pub fn restart_with(
        &mut self,
        era: u64,
        validators: Vec<Address>,
        threshold: u16,
    ) -> Result<Vec<DkgAction>> {
        self.reset_dkg_completion(era);
        let output = self.manager.restart_dkg(era, validators, threshold)?;
        self.finish_output(output)
    }

    /// Returns the in-memory state for the given era, if any.
    pub fn get_state(&self, era: u64) -> Option<&DkgState> {
        self.manager.get_state(era)
    }

    /// Returns whether DKG completed for the given era.
    pub fn is_completed(&self, era: u64) -> bool {
        self.db.dkg_completed(era)
    }

    #[cfg(any(test, feature = "test-utils"))]
    #[allow(dead_code)]
    pub fn get_public_key_package(
        &self,
        era: u64,
    ) -> Option<ethexe_common::crypto::DkgPublicKeyPackage> {
        self.db.public_key_package(era)
    }

    #[cfg(any(test, feature = "test-utils"))]
    #[allow(dead_code)]
    pub fn get_vss_commitment(&self, era: u64) -> Option<ethexe_common::crypto::DkgVssCommitment> {
        self.db.dkg_vss_commitment(era)
    }

    #[cfg(any(test, feature = "test-utils"))]
    #[allow(dead_code)]
    pub fn get_dkg_share(&self, era: u64) -> Option<ethexe_common::crypto::DkgShare> {
        self.db.dkg_share(era)
    }

    /// Persists session snapshots and completed outputs, then returns actions.
    fn finish_output(&mut self, output: DkgManagerOutput) -> Result<Vec<DkgAction>> {
        let DkgManagerOutput { actions, updates } = output;
        for (session_id, state) in updates.session_states {
            self.db.set_dkg_session_state(session_id, state);
        }
        for completed in updates.completions {
            let era = completed.share.era;
            self.db
                .set_public_key_package(era, completed.public_key_package);
            self.db.set_dkg_key_package(era, completed.key_package);
            self.db
                .set_dkg_vss_commitment(era, completed.vss_commitment);
            self.db.set_dkg_share(completed.share);
            self.db.mutate_dkg_session_state(
                ethexe_common::crypto::DkgSessionId { era },
                |state| {
                    state.completed = true;
                },
            );
        }
        Ok(actions)
    }

    /// Clears the completion flag for a given era in storage.
    fn reset_dkg_completion(&mut self, era: u64) {
        self.db
            .mutate_dkg_session_state(ethexe_common::crypto::DkgSessionId { era }, |state| {
                state.completed = false;
            });
    }
}
