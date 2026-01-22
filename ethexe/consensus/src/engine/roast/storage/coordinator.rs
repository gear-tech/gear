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

//! ROAST Coordinator (Leader) Implementation

use crate::{
    engine::roast::core::{RoastResult, SessionConfig, tweak_public_key_package},
    policy::{dkg_session_id, select_roast_leader},
};
use anyhow::{Result, anyhow};
use ethexe_common::{
    Address,
    crypto::{
        DkgIdentifier, SignAggregate, SignCulprits, SignKind, SignNonceCommit, SignNoncePackage,
        SignSessionRequest, SignShare, tweak::hash_to_scalar,
    },
    db::{DkgStorageRO, SignSessionState, SignStorageRW},
    ecdsa::PublicKey,
};
use roast_secp256k1_evm::{
    Coordinator,
    error::RoastError,
    frost::{Signature, round1::SigningCommitments, round2::SignatureShare},
};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

/// Coordinator configuration
#[derive(Debug, Clone)]
pub struct CoordinatorConfig {
    /// Timeout for collecting nonce commitments
    pub nonce_timeout: Duration,
    /// Timeout for collecting partial signatures
    pub partial_timeout: Duration,
}

impl Default for CoordinatorConfig {
    fn default() -> Self {
        Self {
            nonce_timeout: Duration::from_secs(30),
            partial_timeout: Duration::from_secs(30),
        }
    }
}

/// Coordinator state
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoordinatorState {
    /// Idle - no active signing session
    Idle,
    /// Waiting for nonce commitments
    WaitingForNonces { started_at: Instant },
    /// Waiting for partial signatures
    WaitingForPartials { started_at: Instant },
    /// Signing completed
    Completed(SignAggregate),
    /// Signing failed
    Failed(String),
}

/// Events that coordinator can process
#[derive(Debug, Clone)]
pub enum CoordinatorEvent {
    /// Start new signing session
    Start(SessionConfig),
    /// Received nonce commitment
    NonceCommit(SignNonceCommit),
    /// Received partial signature
    PartialSignature(SignShare),
    /// Timeout occurred
    Timeout,
}

/// Actions to perform after processing event
#[derive(Debug, Clone)]
pub enum CoordinatorAction {
    /// Broadcast signing request to participants
    BroadcastRequest(SignSessionRequest),
    /// Broadcast signing package to participants
    BroadcastSigningPackage(SignNoncePackage),
    /// Broadcast final aggregated signature
    BroadcastAggregate(SignAggregate),
    /// Broadcast malicious signer report
    BroadcastCulprits(SignCulprits),
    /// Complete with result
    Complete(RoastResult),
}

/// ROAST Coordinator (Leader)
#[derive(Debug)]
pub struct RoastCoordinator<DB> {
    state: CoordinatorState,
    config: CoordinatorConfig,
    session_config: Option<SessionConfig>,
    db: DB,
    coordinator: Option<Coordinator>,
    identifier_by_address: BTreeMap<Address, DkgIdentifier>,
    address_by_identifier: BTreeMap<DkgIdentifier, Address>,
    tweaked_public_key: Option<[u8; 33]>,
}

impl<DB> RoastCoordinator<DB>
where
    DB: DkgStorageRO + SignStorageRW + Clone,
{
    /// Create new coordinator
    pub fn new(db: DB, config: CoordinatorConfig) -> Self {
        Self {
            state: CoordinatorState::Idle,
            config,
            session_config: None,
            db,
            coordinator: None,
            identifier_by_address: BTreeMap::new(),
            address_by_identifier: BTreeMap::new(),
            tweaked_public_key: None,
        }
    }

    /// Get current state
    pub fn state(&self) -> &CoordinatorState {
        &self.state
    }

    /// Process event and return actions
    pub fn process_event(&mut self, event: CoordinatorEvent) -> Result<Vec<CoordinatorAction>> {
        match event {
            CoordinatorEvent::Start(config) => self.handle_start(config),
            CoordinatorEvent::NonceCommit(commit) => self.handle_nonce_commit(commit),
            CoordinatorEvent::PartialSignature(partial) => self.handle_partial_signature(partial),
            CoordinatorEvent::Timeout => self.handle_timeout(),
        }
    }

    fn handle_start(&mut self, mut config: SessionConfig) -> Result<Vec<CoordinatorAction>> {
        if !matches!(self.state, CoordinatorState::Idle) {
            return Err(anyhow!("Coordinator already active"));
        }

        config.participants.sort();
        let expected_leader = select_roast_leader(
            &config.participants,
            config.msg_hash,
            config.session.era,
            config.attempt,
        );
        if expected_leader != config.self_address {
            return Err(anyhow!("Self is not elected leader for this attempt"));
        }
        self.identifier_by_address = if let Some(state) = self
            .db
            .dkg_session_state(dkg_session_id(config.session.era))
        {
            if state.identifier_map.is_empty() {
                BTreeMap::new()
            } else {
                let map: BTreeMap<Address, DkgIdentifier> =
                    state.identifier_map.into_iter().collect();
                if !config
                    .participants
                    .iter()
                    .all(|addr| map.contains_key(addr))
                {
                    return Err(anyhow!("Missing identifiers for some participants"));
                }
                map
            }
        } else {
            BTreeMap::new()
        };
        if self.identifier_by_address.is_empty() {
            self.identifier_by_address = config
                .participants
                .iter()
                .map(|addr| {
                    let identifier = DkgIdentifier::derive(addr.as_ref())
                        .map_err(|_| anyhow!("Failed to derive identifier"))?;
                    Ok((*addr, identifier))
                })
                .collect::<Result<BTreeMap<_, _>>>()?;
        }
        self.address_by_identifier = self
            .identifier_by_address
            .iter()
            .map(|(addr, id)| (*id, *addr))
            .collect();

        let Some(base_public_key_package) = self.db.public_key_package(config.session.era) else {
            return Err(anyhow!("Missing public key package for era"));
        };
        let tweak = hash_to_scalar(config.tweak_target);
        let public_key_package = tweak_public_key_package(&base_public_key_package, tweak)?;

        let verifying_key = public_key_package
            .verifying_key()
            .serialize()
            .map_err(|err| anyhow!("Failed to serialize verifying key: {err}"))?;
        let tweaked_pk: [u8; 33] = verifying_key
            .as_slice()
            .try_into()
            .map_err(|_| anyhow!("Invalid verifying key length"))?;
        self.tweaked_public_key = Some(tweaked_pk);

        let coordinator = Coordinator::new(
            config.participants.len() as u16,
            config.threshold,
            public_key_package,
            config.msg_hash.as_bytes().to_vec(),
        )
        .map_err(|err| anyhow!("Failed to create coordinator: {err}"))?;

        self.coordinator = Some(coordinator);
        self.session_config = Some(config.clone());
        self.state = CoordinatorState::WaitingForNonces {
            started_at: Instant::now(),
        };

        let request = SignSessionRequest {
            session: config.session,
            leader: config.self_address,
            attempt: config.attempt,
            msg_hash: config.msg_hash,
            tweak_target: config.tweak_target,
            threshold: config.threshold,
            participants: config.participants.clone(),
            kind: SignKind::ArbitraryHash,
        };

        self.persist_state()?;

        Ok(vec![CoordinatorAction::BroadcastRequest(request)])
    }

    fn handle_nonce_commit(&mut self, commit: SignNonceCommit) -> Result<Vec<CoordinatorAction>> {
        let coordinator = self
            .coordinator
            .as_mut()
            .ok_or_else(|| anyhow!("No active coordinator"))?;

        let identifier = self
            .identifier_by_address
            .get(&commit.from)
            .copied()
            .ok_or_else(|| anyhow!("Unknown participant"))?;

        let signing_commitments = SigningCommitments::deserialize(&commit.nonce_commit)
            .map_err(|err| anyhow!("Failed to deserialize commitments: {err}"))?;

        let status = match coordinator.receive(identifier, None, signing_commitments) {
            Ok(status) => status,
            Err(RoastError::MaliciousSigner(_)) => {
                return Ok(vec![CoordinatorAction::BroadcastCulprits(SignCulprits {
                    session: commit.session,
                    msg_hash: commit.msg_hash,
                    culprits: vec![commit.from],
                })]);
            }
            Err(err) => return Err(anyhow!("Coordinator receive failed: {err}")),
        };

        self.db
            .mutate_sign_session_state(commit.msg_hash, commit.session.era, |state| {
                if !state
                    .nonce_commits
                    .iter()
                    .any(|existing| existing.from == commit.from)
                {
                    state.nonce_commits.push(commit.clone());
                }
            });

        if let roast_secp256k1_evm::SessionStatus::Started {
            signers,
            signing_package,
        } = status
        {
            self.state = CoordinatorState::WaitingForPartials {
                started_at: Instant::now(),
            };
            self.persist_state()?;

            let mut commitments = Vec::new();
            for (identifier, commitment) in signing_package.signing_commitments() {
                if !signers.contains(identifier) {
                    continue;
                }
                let addr = self
                    .address_by_identifier
                    .get(identifier)
                    .copied()
                    .ok_or_else(|| anyhow!("Unknown signer identifier"))?;
                let bytes = commitment
                    .serialize()
                    .map_err(|err| anyhow!("Failed to serialize commitments: {err}"))?;
                commitments.push((addr, bytes));
            }
            commitments.sort_by_key(|(addr, _)| *addr);

            let package = SignNoncePackage {
                session: commit.session,
                msg_hash: commit.msg_hash,
                commitments,
            };

            return Ok(vec![CoordinatorAction::BroadcastSigningPackage(package)]);
        }

        self.persist_state()?;
        Ok(vec![])
    }

    fn handle_partial_signature(&mut self, partial: SignShare) -> Result<Vec<CoordinatorAction>> {
        let coordinator = self
            .coordinator
            .as_mut()
            .ok_or_else(|| anyhow!("No active coordinator"))?;

        let identifier = self
            .identifier_by_address
            .get(&partial.from)
            .copied()
            .ok_or_else(|| anyhow!("Unknown participant"))?;

        let signature_share = SignatureShare::deserialize(&partial.partial_sig)
            .map_err(|err| anyhow!("Failed to deserialize signature share: {err}"))?;
        let signing_commitments = SigningCommitments::deserialize(&partial.next_commitments)
            .map_err(|err| anyhow!("Failed to deserialize commitments: {err}"))?;

        let status =
            match coordinator.receive(identifier, Some(signature_share), signing_commitments) {
                Ok(status) => status,
                Err(RoastError::MaliciousSigner(_)) => {
                    return Ok(vec![CoordinatorAction::BroadcastCulprits(SignCulprits {
                        session: partial.session,
                        msg_hash: partial.msg_hash,
                        culprits: vec![partial.from],
                    })]);
                }
                Err(err) => return Err(anyhow!("Coordinator receive failed: {err}")),
            };

        self.db
            .mutate_sign_session_state(partial.msg_hash, partial.session.era, |state| {
                if !state
                    .sign_shares
                    .iter()
                    .any(|existing| existing.from == partial.from)
                {
                    state.sign_shares.push(partial.clone());
                }
            });

        if let roast_secp256k1_evm::SessionStatus::Finished { signature } = status {
            let aggregate = self.build_aggregate(partial.session, partial.msg_hash, signature)?;
            self.state = CoordinatorState::Completed(aggregate.clone());

            let config = self.session_config.as_ref().unwrap();
            self.db
                .mutate_sign_session_state(config.msg_hash, config.session.era, |state| {
                    state.aggregate = Some(aggregate.clone());
                    state.completed = true;
                });

            return Ok(vec![
                CoordinatorAction::BroadcastAggregate(aggregate.clone()),
                CoordinatorAction::Complete(RoastResult::Success(aggregate)),
            ]);
        }

        self.persist_state()?;
        Ok(vec![])
    }

    fn handle_timeout(&mut self) -> Result<Vec<CoordinatorAction>> {
        match &self.state {
            CoordinatorState::WaitingForNonces { started_at } => {
                if started_at.elapsed() > self.config.nonce_timeout {
                    self.state = CoordinatorState::Failed("Nonce timeout".to_string());
                    return Ok(vec![CoordinatorAction::Complete(RoastResult::Failed(
                        "Nonce timeout".to_string(),
                    ))]);
                }
            }
            CoordinatorState::WaitingForPartials { started_at } => {
                if started_at.elapsed() > self.config.partial_timeout {
                    self.state = CoordinatorState::Failed("Partial timeout".to_string());
                    return Ok(vec![CoordinatorAction::Complete(RoastResult::Failed(
                        "Partial timeout".to_string(),
                    ))]);
                }
            }
            _ => {}
        }

        Ok(vec![])
    }

    fn build_aggregate(
        &self,
        session: ethexe_common::crypto::DkgSessionId,
        msg_hash: gprimitives::H256,
        signature: Signature,
    ) -> Result<SignAggregate> {
        let tweaked_pk = self
            .tweaked_public_key
            .ok_or_else(|| anyhow!("Missing tweaked public key"))?;

        let signature_bytes = signature
            .serialize()
            .map_err(|err| anyhow!("Failed to serialize signature: {err}"))?;

        let mut r_bytes = [0u8; 33];
        let r_slice = signature_bytes
            .get(..33)
            .ok_or_else(|| anyhow!("Signature missing R"))?;
        r_bytes.copy_from_slice(r_slice);

        let r_uncompressed = PublicKey::from_bytes(r_bytes)
            .map_err(|err| anyhow!("Invalid signature R: {err}"))?
            .to_uncompressed();
        let (r_x, r_y) = r_uncompressed.split_at(32);

        let z_bytes = signature_bytes
            .get(33..65)
            .ok_or_else(|| anyhow!("Signature missing z"))?;

        let mut sig96 = [0u8; 96];
        sig96[..32].copy_from_slice(r_x);
        sig96[32..64].copy_from_slice(r_y);
        sig96[64..96].copy_from_slice(z_bytes);

        Ok(SignAggregate {
            session,
            msg_hash,
            tweaked_pk,
            signature96: sig96,
        })
    }

    fn persist_state(&self) -> Result<()> {
        let Some(config) = self.session_config.as_ref() else {
            return Ok(());
        };

        let request = self
            .session_config
            .as_ref()
            .map(|config| SignSessionRequest {
                session: config.session,
                leader: config.self_address,
                attempt: config.attempt,
                msg_hash: config.msg_hash,
                tweak_target: config.tweak_target,
                threshold: config.threshold,
                participants: config.participants.clone(),
                kind: SignKind::ArbitraryHash,
            });

        let state = SignSessionState {
            request,
            nonce_commits: vec![],
            sign_shares: vec![],
            aggregate: None,
            completed: matches!(self.state, CoordinatorState::Completed(_)),
        };

        self.db
            .set_sign_session_state(config.msg_hash, config.session.era, state);

        Ok(())
    }
}
