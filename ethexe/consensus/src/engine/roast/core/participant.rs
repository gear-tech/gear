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

//! ROAST Participant Implementation

use super::tweak_key_package;
use anyhow::{Result, anyhow};
use ethexe_common::{
    Address,
    crypto::{
        DkgIdentifier, DkgKeyPackage, PreNonceCommitment, SignNonceCommit, SignNoncePackage,
        SignSessionRequest, SignShare, tweak::hash_to_scalar,
    },
};
use rand::rngs::OsRng;
use roast_secp256k1_evm::frost::{
    SigningPackage,
    round1::{self, SigningCommitments, SigningNonces},
    round2::{self, SignatureShare},
};
use std::collections::BTreeMap;

/// Participant configuration
#[derive(Debug, Clone)]
pub struct ParticipantConfig {
    /// This participant's address
    pub self_address: Address,
}

/// Participant state
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParticipantState {
    /// Idle
    Idle,
    /// Nonce sent, waiting for signing package
    NonceSent,
    /// Partial signature sent
    PartialSent,
}

/// Events that participant can process
#[derive(Debug, Clone)]
pub enum ParticipantEvent {
    /// Received signing request from coordinator
    SignRequest {
        request: SignSessionRequest,
        key_package: Box<DkgKeyPackage>,
        identifiers: BTreeMap<Address, DkgIdentifier>,
        pre_nonce: Option<PreNonceCommitment>,
    },
    /// Received signing package from coordinator
    SigningPackage(SignNoncePackage),
}

/// Actions participant should perform
#[derive(Debug, Clone)]
pub enum ParticipantAction {
    /// Send nonce commitment to coordinator
    SendNonceCommit(SignNonceCommit),
    /// Send partial signature to coordinator
    SendPartialSignature(SignShare),
}

/// ROAST Participant
#[derive(Debug)]
pub struct RoastParticipant {
    state: ParticipantState,
    config: ParticipantConfig,
    current_session: Option<SignSessionRequest>,
    key_package: Option<DkgKeyPackage>,
    signing_nonces: Option<SigningNonces>,
    identifiers: BTreeMap<Address, DkgIdentifier>,
}

impl RoastParticipant {
    /// Create new participant
    pub fn new(config: ParticipantConfig) -> Self {
        Self {
            state: ParticipantState::Idle,
            config,
            current_session: None,
            key_package: None,
            signing_nonces: None,
            identifiers: BTreeMap::new(),
        }
    }

    /// Get current state
    pub fn state(&self) -> &ParticipantState {
        &self.state
    }

    /// Process event and return actions
    pub fn process_event(&mut self, event: ParticipantEvent) -> Result<Vec<ParticipantAction>> {
        match event {
            ParticipantEvent::SignRequest {
                request,
                key_package,
                identifiers,
                pre_nonce,
            } => self.handle_sign_request(request, *key_package, identifiers, pre_nonce),
            ParticipantEvent::SigningPackage(package) => self.handle_signing_package(package),
        }
    }

    fn handle_sign_request(
        &mut self,
        request: SignSessionRequest,
        key_package: DkgKeyPackage,
        identifiers: BTreeMap<Address, DkgIdentifier>,
        pre_nonce: Option<PreNonceCommitment>,
    ) -> Result<Vec<ParticipantAction>> {
        if !matches!(self.state, ParticipantState::Idle) {
            return Err(anyhow!("Participant already in session"));
        }

        self.identifiers = identifiers;

        let expected_identifier = self
            .identifiers
            .get(&self.config.self_address)
            .copied()
            .ok_or_else(|| anyhow!("Self not in participants list"))?;
        if expected_identifier != *key_package.identifier() {
            return Err(anyhow!("Key package identifier does not match DKG map"));
        }

        let tweak = hash_to_scalar(request.tweak_target);
        let tweaked_key_package = tweak_key_package(&key_package, tweak)?;

        let (signing_nonces, signing_commitments) = match pre_nonce {
            Some(pre_nonce) => {
                let signing_nonces = SigningNonces::deserialize(&pre_nonce.nonces)
                    .map_err(|err| anyhow!("Failed to deserialize signing nonces: {err}"))?;
                let signing_commitments =
                    SigningCommitments::deserialize(&pre_nonce.commitments)
                        .map_err(|err| anyhow!("Failed to deserialize commitments: {err}"))?;
                if *signing_nonces.commitments() != signing_commitments {
                    return Err(anyhow!("Pre-nonce commitments mismatch"));
                }
                (signing_nonces, signing_commitments)
            }
            None => {
                let mut rng = OsRng;
                round1::commit(tweaked_key_package.signing_share(), &mut rng)
            }
        };

        let signing_commitments = signing_commitments
            .serialize()
            .map_err(|err| anyhow!("Failed to serialize commitments: {err}"))?;

        self.key_package = Some(tweaked_key_package);
        self.signing_nonces = Some(signing_nonces);
        self.current_session = Some(request.clone());
        self.state = ParticipantState::NonceSent;

        let commit_msg = SignNonceCommit {
            session: request.session,
            from: self.config.self_address,
            msg_hash: request.msg_hash,
            nonce_commit: signing_commitments,
        };

        Ok(vec![ParticipantAction::SendNonceCommit(commit_msg)])
    }

    fn handle_signing_package(
        &mut self,
        package: SignNoncePackage,
    ) -> Result<Vec<ParticipantAction>> {
        if !matches!(self.state, ParticipantState::NonceSent) {
            tracing::debug!(
                state = ?self.state,
                "Ignoring signing package in unexpected participant state"
            );
            return Ok(vec![]);
        }

        if let Some(current) = &self.current_session
            && (current.session != package.session || current.msg_hash != package.msg_hash)
        {
            tracing::debug!(
                expected_session = ?current.session,
                expected_hash = %current.msg_hash,
                got_session = ?package.session,
                got_hash = %package.msg_hash,
                "Ignoring signing package for different session"
            );
            return Ok(vec![]);
        }

        let current = self
            .current_session
            .as_ref()
            .ok_or_else(|| anyhow!("No current session"))?;

        if current.session != package.session {
            return Err(anyhow!("Session ID mismatch"));
        }
        if current.msg_hash != package.msg_hash {
            return Err(anyhow!("Message hash mismatch"));
        }

        let self_identifier = self
            .identifiers
            .get(&self.config.self_address)
            .copied()
            .ok_or_else(|| anyhow!("Self identifier not found"))?;

        let mut commitments = BTreeMap::new();
        for (addr, bytes) in &package.commitments {
            let identifier = self
                .identifiers
                .get(addr)
                .ok_or_else(|| anyhow!("Unknown participant commitment"))?;
            let signing_commitments = SigningCommitments::deserialize(bytes)
                .map_err(|err| anyhow!("Failed to deserialize commitments: {err}"))?;
            commitments.insert(*identifier, signing_commitments);
        }

        if !commitments.contains_key(&self_identifier) {
            return Ok(vec![]);
        }

        let signing_package = SigningPackage::new(commitments, package.msg_hash.as_bytes());

        let signing_nonces = self
            .signing_nonces
            .take()
            .ok_or_else(|| anyhow!("Missing signing nonces"))?;
        let key_package = self.key_package.as_ref().ok_or_else(|| {
            anyhow::Error::new(crate::engine::roast::RoastErrorKind::MissingKeyPackage)
        })?;

        let signature_share: SignatureShare =
            round2::sign(&signing_package, &signing_nonces, key_package)
                .map_err(|err| anyhow!("Failed to sign: {err}"))?;

        let mut rng = OsRng;
        let (next_signing_nonces, next_commitments) =
            round1::commit(key_package.signing_share(), &mut rng);
        self.signing_nonces = Some(next_signing_nonces);

        let next_commitments = next_commitments
            .serialize()
            .map_err(|err| anyhow!("Failed to serialize next commitments: {err}"))?;

        let partial_sig = SignShare {
            session: package.session,
            from: self.config.self_address,
            msg_hash: package.msg_hash,
            partial_sig: signature_share.serialize(),
            next_commitments,
        };

        self.state = ParticipantState::PartialSent;

        Ok(vec![ParticipantAction::SendPartialSignature(partial_sig)])
    }

    /// Reset to idle state
    pub fn reset(&mut self) {
        self.state = ParticipantState::Idle;
        self.current_session = None;
        self.key_package = None;
        self.signing_nonces = None;
        self.identifiers.clear();
    }
}
