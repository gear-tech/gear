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

//! DKG Protocol implementation using ROAST/FROST primitives.

use crate::engine::dkg::DkgErrorKind;
use anyhow::Result;
use ethexe_common::{
    Address,
    crypto::{
        DkgComplaint, DkgIdentifier, DkgJustification, DkgKeyPackage, DkgPublicKeyPackage,
        DkgRound1, DkgRound2, DkgRound2Culprits, DkgSessionId, DkgVssCommitment,
    },
};
use rand::rngs::OsRng;
use roast_secp256k1_evm::dkg::{Dealer, Participant};
use std::collections::BTreeMap;

/// DKG protocol configuration.
#[derive(Debug, Clone)]
pub struct DkgConfig {
    /// Session ID
    pub session: DkgSessionId,
    /// Threshold (t)
    pub threshold: u16,
    /// All participants (sorted)
    pub participants: Vec<Address>,
    /// Address of this participant
    pub self_address: Address,
}

/// Outcome of DKG finalization.
#[derive(Debug)]
pub enum FinalizeResult {
    Completed {
        key_package: Box<DkgKeyPackage>,
        public_key_package: DkgPublicKeyPackage,
        vss_commitment: DkgVssCommitment,
    },
    Culprits(DkgRound2Culprits),
}

/// DKG Protocol handler.
#[derive(Debug)]
pub struct DkgProtocol {
    pub(super) config: DkgConfig,
    pub(super) identifiers: BTreeMap<Address, DkgIdentifier>,
    pub(super) addresses: BTreeMap<DkgIdentifier, Address>,
    pub(super) participant: Participant,
    pub(super) dealer: Dealer,
    pub(super) round1_packages: BTreeMap<DkgIdentifier, DkgRound1>,
    pub(super) round2_packages: BTreeMap<DkgIdentifier, DkgRound2>,
    pub(super) complaints: Vec<DkgComplaint>,
    pub(super) justifications: Vec<DkgJustification>,
    pub(super) round2_culprits: Vec<DkgRound2Culprits>,
}

impl DkgProtocol {
    /// Create a new DKG protocol instance.
    pub fn new(mut config: DkgConfig) -> Result<Self> {
        // Enforce deterministic ordering and local membership.
        config.participants.sort();
        if !config.participants.contains(&config.self_address) {
            return Err(anyhow::Error::new(DkgErrorKind::SelfNotInParticipants));
        }

        // Derive identifiers; duplicates are rejected.
        let mut identifiers = BTreeMap::new();
        for address in config.participants.iter() {
            let identifier = DkgIdentifier::derive(address.as_ref())
                .map_err(|_| anyhow::Error::new(DkgErrorKind::InvalidParticipantIdentifier))?;
            identifiers.insert(*address, identifier);
        }
        if identifiers.len() != config.participants.len() {
            return Err(anyhow::Error::new(DkgErrorKind::DuplicateParticipants));
        }

        let addresses = identifiers
            .iter()
            .map(|(address, identifier)| (*identifier, *address))
            .collect::<BTreeMap<_, _>>();
        let participants = identifiers.values().copied().collect::<Vec<_>>();
        let mut rng = OsRng;

        // Create participant + dealer roles for this node.
        let participant = Participant::new(
            identifiers[&config.self_address],
            participants.len() as u16,
            config.threshold,
            &mut rng,
        )?;
        let dealer = Dealer::new(participants.len() as u16, config.threshold, participants)?;

        Ok(Self {
            config,
            identifiers,
            addresses,
            participant,
            dealer,
            round1_packages: BTreeMap::new(),
            round2_packages: BTreeMap::new(),
            complaints: Vec::new(),
            justifications: Vec::new(),
            round2_culprits: Vec::new(),
        })
    }
}
