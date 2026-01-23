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

use super::RoastMessage;
use crate::engine::roast::RoastManager;
use anyhow::Result;
use ethexe_common::{
    Address,
    crypto::{
        SignAggregate, SignCulprits, SignNonceCommit, SignNoncePackage, SignSessionRequest,
        SignShare,
    },
};
use gprimitives::{ActorId, H256};

/// External inputs for the ROAST engine (network + local triggers).
#[derive(Debug, Clone)]
pub enum RoastEngineEvent {
    StartSigning {
        msg_hash: H256,
        era: u64,
        tweak_target: ActorId,
        threshold: u16,
        participants: Vec<Address>,
    },
    SignSessionRequest {
        from: Address,
        request: SignSessionRequest,
    },
    NonceCommit {
        commit: SignNonceCommit,
    },
    NoncePackage {
        package: SignNoncePackage,
    },
    SignShare {
        partial: SignShare,
    },
    SignCulprits {
        culprits: SignCulprits,
    },
    SignAggregate {
        aggregate: SignAggregate,
    },
}

/// ROAST engine wraps the manager for signing sessions.
#[derive(Debug)]
pub struct RoastEngine<DB> {
    manager: RoastManager<DB>,
}

impl<DB> RoastEngine<DB>
where
    DB: crate::engine::storage::RoastStore,
{
    /// Creates a new ROAST engine bound to a DB and local validator address.
    pub fn new(db: DB, self_address: Address) -> Self {
        Self {
            manager: RoastManager::new(db, self_address),
        }
    }

    /// Routes a ROAST event through the manager.
    pub fn handle_event(&mut self, event: RoastEngineEvent) -> Result<Vec<RoastMessage>> {
        match event {
            RoastEngineEvent::StartSigning {
                msg_hash,
                era,
                tweak_target,
                threshold,
                participants,
            } => self
                .manager
                .start_signing(msg_hash, era, tweak_target, threshold, participants),
            RoastEngineEvent::SignSessionRequest { from, request } => {
                self.manager.process_sign_request(from, request)
            }
            RoastEngineEvent::NonceCommit { commit } => self.manager.process_nonce_commit(commit),
            RoastEngineEvent::NoncePackage { package } => {
                self.manager.process_nonce_package(package)
            }
            RoastEngineEvent::SignShare { partial } => {
                self.manager.process_partial_signature(partial)
            }
            RoastEngineEvent::SignCulprits { culprits } => {
                self.manager.process_culprits(culprits)?;
                Ok(vec![])
            }
            RoastEngineEvent::SignAggregate { aggregate } => {
                self.manager.process_aggregate(aggregate)?;
                Ok(vec![])
            }
        }
    }

    /// Advances timeout-driven retries for active sessions.
    pub fn tick_timeouts(&mut self) -> Result<Vec<RoastMessage>> {
        self.manager.process_timeouts()
    }

    /// Returns the stored aggregate signature, if available.
    pub fn get_signature(&self, msg_hash: H256, era: u64) -> Option<SignAggregate> {
        self.manager.get_signature(msg_hash, era)
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub fn get_cached_signature(
        &self,
        msg_hash: H256,
        era: u64,
        tweak_target: ActorId,
    ) -> Option<SignAggregate> {
        self.manager
            .get_cached_signature(msg_hash, era, tweak_target)
    }

    #[allow(dead_code)]
    pub fn get_pre_nonce_cache(
        &self,
        era: u64,
        tweak_target: ActorId,
    ) -> Option<Vec<ethexe_common::crypto::PreNonceCommitment>> {
        self.manager.get_pre_nonce_cache(era, tweak_target)
    }

    #[allow(dead_code)]
    pub fn set_pre_nonce_cache(
        &self,
        era: u64,
        tweak_target: ActorId,
        cache: Vec<ethexe_common::crypto::PreNonceCommitment>,
    ) {
        self.manager.set_pre_nonce_cache(era, tweak_target, cache);
    }
}
