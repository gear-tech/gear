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
use anyhow::{Result, anyhow};
use ethexe_common::{
    Address,
    crypto::{DkgRound1, DkgRound2, DkgRound2Culprits},
    db::OnChainStorageRO,
};

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

#[derive(Debug)]
pub struct DkgEngine<DB> {
    manager: DkgManager<DB>,
    db: DB,
}

impl<DB> DkgEngine<DB>
where
    DB: super::super::storage::DkgStore + OnChainStorageRO,
{
    pub fn new(db: DB, self_address: Address) -> Self {
        Self {
            manager: DkgManager::new(db.clone(), self_address),
            db,
        }
    }

    pub fn handle_event(&mut self, event: DkgEngineEvent) -> Result<Vec<DkgAction>> {
        let era = match &event {
            DkgEngineEvent::Start { era, .. } => *era,
            DkgEngineEvent::Round1 { message, .. } => message.session.era,
            DkgEngineEvent::Round2 { message, .. } => message.session.era,
            DkgEngineEvent::Round2Culprits { message, .. } => message.session.era,
            DkgEngineEvent::Complaint { message, .. } => message.session.era,
            DkgEngineEvent::Justification { message, .. } => message.session.era,
        };

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

        match result {
            Ok(actions) => Ok(actions),
            Err(err) => {
                if let Ok(actions) = self.restart_from_storage(era) {
                    return Ok(actions);
                }
                Err(err)
            }
        }
    }

    pub fn tick_timeouts(&mut self) -> Result<Vec<DkgAction>> {
        self.manager.process_timeouts()
    }

    pub fn restart_from_storage(&mut self, era: u64) -> Result<Vec<DkgAction>> {
        let Some(validators) = self.db.validators(era) else {
            return Err(anyhow!(
                "Unable to restart DKG for era {era}: validators missing"
            ));
        };
        let validators: Vec<_> = validators.into_iter().collect();
        let threshold = ((validators.len() as u64 * 2) / 3).max(1) as u16;
        self.manager.restart_dkg(era, validators, threshold)
    }

    pub fn restart_with(
        &mut self,
        era: u64,
        validators: Vec<Address>,
        threshold: u16,
    ) -> Result<Vec<DkgAction>> {
        self.manager.restart_dkg(era, validators, threshold)
    }

    pub fn get_state(&self, era: u64) -> Option<&DkgState> {
        self.manager.get_state(era)
    }

    pub fn is_completed(&self, era: u64) -> bool {
        self.manager.is_completed(era)
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub fn get_public_key_package(
        &self,
        era: u64,
    ) -> Option<ethexe_common::crypto::DkgPublicKeyPackage> {
        self.manager.get_public_key_package(era)
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub fn get_vss_commitment(&self, era: u64) -> Option<ethexe_common::crypto::DkgVssCommitment> {
        self.manager.get_vss_commitment(era)
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub fn get_dkg_share(&self, era: u64) -> Option<ethexe_common::crypto::DkgShare> {
        self.manager.get_dkg_share(era)
    }
}
