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

use super::{
    actions, cache, session,
    types::{RoastMessage, SessionProgress},
};
use crate::{
    engine::{
        roast::{
            core::{
                ParticipantConfig, ParticipantEvent, RoastParticipant, RoastResult, SessionConfig,
            },
            storage::coordinator::{
                CoordinatorAction, CoordinatorConfig, CoordinatorEvent, RoastCoordinator,
            },
        },
        storage::RoastStore,
    },
    policy::{RoastSessionId, dkg_session_id, roast_session_id, select_roast_leader},
};
use anyhow::Result;
use ethexe_common::{
    Address,
    crypto::{
        SignAggregate, SignCulprits, SignNonceCommit, SignNoncePackage, SignSessionRequest,
        SignShare,
    },
    db::SignSessionState,
};
use gprimitives::{ActorId, H256};
use std::{
    collections::{BTreeSet, HashMap},
    time::Instant,
};

/// ROAST Manager handles threshold signing sessions
#[derive(Debug)]
pub struct RoastManager<DB> {
    /// Active coordinator sessions (when we are leader)
    coordinators: HashMap<RoastSessionId, RoastCoordinator<DB>>,
    /// Active participant sessions (when we are participant)
    participants: HashMap<RoastSessionId, RoastParticipant>,
    /// Observed session progress for timeouts/failover
    session_progress: HashMap<RoastSessionId, SessionProgress>,
    /// Excluded signers per session
    excluded: HashMap<RoastSessionId, BTreeSet<Address>>,
    /// Database
    db: DB,
    /// This validator's address
    self_address: Address,
    /// Configuration
    coordinator_config: CoordinatorConfig,
    participant_config: ParticipantConfig,
}

impl<DB> RoastManager<DB>
where
    DB: RoastStore,
{
    /// Create new ROAST manager
    pub fn new(db: DB, self_address: Address) -> Self {
        Self {
            coordinators: HashMap::new(),
            participants: HashMap::new(),
            session_progress: HashMap::new(),
            excluded: HashMap::new(),
            db: db.clone(),
            self_address,
            coordinator_config: CoordinatorConfig::default(),
            participant_config: ParticipantConfig { self_address },
        }
    }

    /// Start signing session for a batch commitment
    /// Start signing session as coordinator
    /// This should be called by the batch producer who initiates signing
    pub fn start_signing(
        &mut self,
        msg_hash: H256,
        era: u64,
        tweak_target: ActorId,
        threshold: u16,
        mut participants: Vec<Address>,
    ) -> Result<Vec<RoastMessage>> {
        cache::prune_caches_if_needed(&self.db, era);

        if let Some(message) =
            cache::cached_signature_message(&self.db, era, tweak_target, msg_hash)
        {
            tracing::debug!(
                era,
                msg_hash = %msg_hash,
                tweak_target = %tweak_target,
                cache_hit = true,
                "ROAST signature cache hit"
            );
            return Ok(vec![message]);
        }
        tracing::debug!(
            era,
            msg_hash = %msg_hash,
            tweak_target = %tweak_target,
            cache_hit = false,
            "ROAST signature cache miss"
        );

        let session_id = roast_session_id(msg_hash, era);
        let attempt = 0;
        participants.sort();
        let leader = select_roast_leader(&participants, msg_hash, era, attempt);

        if leader == self.self_address {
            self.start_as_coordinator(session_id, attempt, tweak_target, threshold, participants)
        } else {
            let request = SignSessionRequest {
                session: dkg_session_id(era),
                leader,
                attempt,
                msg_hash,
                tweak_target,
                threshold,
                participants: participants.clone(),
                kind: ethexe_common::crypto::SignKind::ArbitraryHash,
            };
            self.session_progress.insert(
                session_id,
                SessionProgress {
                    last_activity: Instant::now(),
                    attempt,
                    participants,
                    threshold,
                    tweak_target,
                    leader,
                    leader_request_seen: false,
                },
            );
            Ok(vec![RoastMessage::SignSessionRequest(request)])
        }
    }

    fn start_as_coordinator(
        &mut self,
        session_id: RoastSessionId,
        attempt: u32,
        tweak_target: ActorId,
        threshold: u16,
        participants: Vec<Address>,
    ) -> Result<Vec<RoastMessage>> {
        let leader =
            select_roast_leader(&participants, session_id.msg_hash, session_id.era, attempt);
        if leader != self.self_address {
            return Ok(vec![]);
        }
        let config = SessionConfig {
            session: dkg_session_id(session_id.era),
            msg_hash: session_id.msg_hash,
            tweak_target,
            attempt,
            threshold,
            participants,
            self_address: self.self_address,
        };

        let mut coordinator =
            RoastCoordinator::new(self.db.clone(), self.coordinator_config.clone());

        let actions = coordinator.process_event(CoordinatorEvent::Start(config.clone()))?;
        self.coordinators.insert(session_id, coordinator);
        self.session_progress.insert(
            session_id,
            SessionProgress {
                last_activity: Instant::now(),
                attempt,
                participants: config.participants.clone(),
                threshold: config.threshold,
                tweak_target: config.tweak_target,
                leader: self.self_address,
                leader_request_seen: true,
            },
        );

        actions::coordinator_actions_to_outbound(actions)
    }

    /// Process signing request from coordinator
    pub fn process_sign_request(
        &mut self,
        from: Address,
        request: SignSessionRequest,
    ) -> Result<Vec<RoastMessage>> {
        cache::prune_caches_if_needed(&self.db, request.session.era);

        if let Some(message) = cache::cached_signature_for_request(&self.db, &request) {
            tracing::debug!(
                era = request.session.era,
                msg_hash = %request.msg_hash,
                tweak_target = %request.tweak_target,
                cache_hit = true,
                "ROAST signature cache hit"
            );
            return Ok(vec![message]);
        }
        tracing::debug!(
            era = request.session.era,
            msg_hash = %request.msg_hash,
            tweak_target = %request.tweak_target,
            cache_hit = false,
            "ROAST signature cache miss"
        );

        let session_id = roast_session_id(request.msg_hash, request.session.era);

        if self.participants.contains_key(&session_id) {
            return Ok(vec![]);
        }

        if let Some(progress) = self.session_progress.get(&session_id) {
            if request.attempt < progress.attempt {
                return Ok(vec![]);
            }
            if request.attempt == progress.attempt && request.leader == progress.leader {
                if progress.leader_request_seen && request.leader != self.self_address {
                    return Ok(vec![]);
                }
                if from != request.leader {
                    return Ok(vec![]);
                }
            }
        }

        session::ensure_sorted_participants(&request)?;
        let expected_leader = select_roast_leader(
            &request.participants,
            request.msg_hash,
            request.session.era,
            request.attempt,
        );
        if expected_leader != request.leader {
            return Err(anyhow::anyhow!("Unexpected leader for attempt"));
        }

        if request.leader == self.self_address {
            if from != request.leader {
                return self.start_as_coordinator(
                    roast_session_id(request.msg_hash, request.session.era),
                    request.attempt,
                    request.tweak_target,
                    request.threshold,
                    request.participants.clone(),
                );
            }
        } else if from != request.leader {
            return Ok(vec![]);
        }

        let Some(key_package) = self.db.dkg_key_package(request.session.era) else {
            return Err(anyhow::Error::new(
                crate::engine::roast::RoastErrorKind::MissingKeyPackage,
            ));
        };
        let Some(share) = self.db.dkg_share(request.session.era) else {
            return Err(anyhow::Error::new(
                crate::engine::roast::RoastErrorKind::MissingDkgShare,
            ));
        };

        if *key_package.identifier() != share.identifier {
            return Err(anyhow::Error::new(
                crate::engine::roast::RoastErrorKind::KeyPackageIdentifierMismatch,
            ));
        }
        if *key_package.min_signers() != share.threshold {
            return Err(anyhow::Error::new(
                crate::engine::roast::RoastErrorKind::KeyPackageThresholdMismatch,
            ));
        }
        if share.threshold != request.threshold {
            return Err(anyhow::anyhow!("Request threshold mismatch"));
        }
        let expected_index = request
            .participants
            .iter()
            .position(|addr| *addr == self.self_address)
            .and_then(|idx| idx.checked_add(1))
            .and_then(|idx| u16::try_from(idx).ok())
            .ok_or_else(|| anyhow::anyhow!("Self not in participants list"))?;
        if share.index != expected_index {
            return Err(anyhow::Error::new(
                crate::engine::roast::RoastErrorKind::DkgShareIndexMismatch,
            ));
        }

        let identifiers =
            session::identifiers_for_session(&self.db, request.session.era, &request.participants)?;
        let (pre_nonce, remaining_cache) = self
            .db
            .pre_nonce_cache(request.session.era, request.tweak_target)
            .map_or((None, None), |mut cache| {
                let pre_nonce = cache.pop();
                (pre_nonce, Some(cache))
            });
        tracing::debug!(
            era = request.session.era,
            tweak_target = %request.tweak_target,
            cache_hit = pre_nonce.is_some(),
            "ROAST pre-nonce cache lookup"
        );
        if let Some(cache) = remaining_cache {
            self.db
                .set_pre_nonce_cache(request.session.era, request.tweak_target, cache);
        }
        if self
            .db
            .sign_session_state(request.msg_hash, request.session.era)
            .is_none()
        {
            self.db.set_sign_session_state(
                request.msg_hash,
                request.session.era,
                SignSessionState {
                    request: Some(request.clone()),
                    nonce_commits: vec![],
                    sign_shares: vec![],
                    aggregate: None,
                    completed: false,
                },
            );
        }

        let mut participant = RoastParticipant::new(self.participant_config.clone());
        let actions = participant.process_event(ParticipantEvent::SignRequest {
            request: request.clone(),
            key_package: Box::new(key_package),
            identifiers,
            pre_nonce,
        })?;

        self.participants.insert(session_id, participant);
        self.session_progress.insert(
            session_id,
            SessionProgress {
                last_activity: Instant::now(),
                attempt: request.attempt,
                participants: request.participants.clone(),
                threshold: request.threshold,
                tweak_target: request.tweak_target,
                leader: request.leader,
                leader_request_seen: true,
            },
        );

        actions::participant_actions_to_outbound(actions)
    }

    /// Process nonce commitment (coordinator)
    pub fn process_nonce_commit(&mut self, commit: SignNonceCommit) -> Result<Vec<RoastMessage>> {
        let session_id = roast_session_id(commit.msg_hash, commit.session.era);

        if self
            .db
            .sign_session_state(commit.msg_hash, commit.session.era)
            .is_some_and(|state| state.nonce_commits.iter().any(|c| c.from == commit.from))
        {
            return Ok(vec![]);
        }

        if !self.coordinators.contains_key(&session_id)
            && self
                .db
                .sign_session_state(commit.msg_hash, commit.session.era)
                .is_none()
        {
            return Ok(vec![]);
        }

        let mut messages = vec![];
        if !self.coordinators.contains_key(&session_id) {
            let mut restored = self.restore_coordinator(session_id)?;
            messages.append(&mut restored);
        }

        if let Some(coordinator) = self.coordinators.get_mut(&session_id) {
            if self
                .excluded
                .get(&session_id)
                .is_some_and(|set| set.contains(&commit.from))
            {
                return Ok(messages);
            }
            let actions = coordinator.process_event(CoordinatorEvent::NonceCommit(commit))?;
            if let Some(progress) = self.session_progress.get_mut(&session_id) {
                progress.last_activity = Instant::now();
            }
            let has_signing_package = actions
                .iter()
                .any(|action| matches!(action, CoordinatorAction::BroadcastSigningPackage(_)));
            let mut next = actions::coordinator_actions_to_outbound(actions)?;
            messages.append(&mut next);

            if !has_signing_package
                && let Some(state) = self
                    .db
                    .sign_session_state(session_id.msg_hash, session_id.era)
                && let Some(request) = state.request.clone()
                && state.nonce_commits.len() >= request.threshold as usize
            {
                let mut missing = BTreeSet::new();
                for addr in &request.participants {
                    if !state.nonce_commits.iter().any(|c| &c.from == addr) {
                        missing.insert(*addr);
                    }
                }

                if !missing.is_empty() {
                    let entry = self.excluded.entry(session_id).or_default();
                    entry.extend(missing);

                    let mut participants = request.participants.clone();
                    participants.retain(|addr| !entry.contains(addr));

                    let next_attempt = self
                        .session_progress
                        .get(&session_id)
                        .map(|progress| progress.attempt.saturating_add(1))
                        .unwrap_or(1);

                    if participants.len() >= request.threshold as usize {
                        let next_leader = select_roast_leader(
                            &participants,
                            session_id.msg_hash,
                            session_id.era,
                            next_attempt,
                        );
                        if next_leader == self.self_address {
                            let mut next = self.start_as_coordinator(
                                session_id,
                                next_attempt,
                                request.tweak_target,
                                request.threshold,
                                participants.clone(),
                            )?;
                            messages.append(&mut next);
                        } else {
                            let retry_request = SignSessionRequest {
                                session: request.session,
                                leader: next_leader,
                                attempt: next_attempt,
                                msg_hash: request.msg_hash,
                                tweak_target: request.tweak_target,
                                threshold: request.threshold,
                                participants: participants.clone(),
                                kind: request.kind,
                            };
                            messages.push(RoastMessage::SignSessionRequest(retry_request));
                        }
                        if let Some(progress) = self.session_progress.get_mut(&session_id) {
                            progress.last_activity = Instant::now();
                            progress.attempt = next_attempt;
                            progress.leader = next_leader;
                            progress.participants = participants;
                            progress.leader_request_seen = next_leader == self.self_address;
                        }
                    }
                }
            }
            Ok(messages)
        } else {
            Ok(messages)
        }
    }

    /// Process signing package from coordinator (participant)
    pub fn process_nonce_package(
        &mut self,
        package: SignNoncePackage,
    ) -> Result<Vec<RoastMessage>> {
        if let Some(messages) = super::participant::handle_nonce_package(
            &mut self.participants,
            &mut self.session_progress,
            package,
        )? {
            return Ok(messages);
        }
        Ok(vec![])
    }

    /// Process partial signature (coordinator)
    pub fn process_partial_signature(&mut self, partial: SignShare) -> Result<Vec<RoastMessage>> {
        let session_id = roast_session_id(partial.msg_hash, partial.session.era);

        if self
            .db
            .sign_session_state(partial.msg_hash, partial.session.era)
            .is_some_and(|state| state.sign_shares.iter().any(|s| s.from == partial.from))
        {
            return Ok(vec![]);
        }

        if !self.coordinators.contains_key(&session_id)
            && self
                .db
                .sign_session_state(partial.msg_hash, partial.session.era)
                .is_none()
        {
            return Ok(vec![]);
        }

        let mut messages = vec![];
        if !self.coordinators.contains_key(&session_id) {
            let mut restored = self.restore_coordinator(session_id)?;
            messages.append(&mut restored);
        }

        if let Some(coordinator) = self.coordinators.get_mut(&session_id) {
            if self
                .excluded
                .get(&session_id)
                .is_some_and(|set| set.contains(&partial.from))
            {
                return Ok(messages);
            }
            let actions = coordinator.process_event(CoordinatorEvent::PartialSignature(partial))?;
            if let Some(progress) = self.session_progress.get_mut(&session_id) {
                progress.last_activity = Instant::now();
            }
            let mut next = actions::coordinator_actions_to_outbound(actions)?;
            messages.append(&mut next);
            Ok(messages)
        } else {
            Ok(messages)
        }
    }

    pub fn process_culprits(&mut self, culprits: SignCulprits) -> Result<()> {
        let session_id = roast_session_id(culprits.msg_hash, culprits.session.era);
        let entry = self.excluded.entry(session_id).or_default();
        for culprit in culprits.culprits {
            entry.insert(culprit);
        }
        Ok(())
    }

    fn restore_coordinator(&mut self, session_id: RoastSessionId) -> Result<Vec<RoastMessage>> {
        let Some(state) = self
            .db
            .sign_session_state(session_id.msg_hash, session_id.era)
        else {
            return Ok(vec![]);
        };
        let Some(request) = state.request else {
            return Ok(vec![]);
        };
        if request.leader != self.self_address {
            return Ok(vec![]);
        }

        self.start_as_coordinator(
            session_id,
            request.attempt,
            request.tweak_target,
            request.threshold,
            request.participants,
        )
    }

    pub fn process_timeouts(&mut self) -> Result<Vec<RoastMessage>> {
        let mut messages = vec![];
        let session_ids: Vec<_> = self.coordinators.keys().copied().collect();
        for session_id in session_ids {
            let actions = if let Some(coordinator) = self.coordinators.get_mut(&session_id) {
                coordinator.process_event(CoordinatorEvent::Timeout)?
            } else {
                continue;
            };

            for action in &actions {
                let CoordinatorAction::Complete(RoastResult::Failed(reason)) = action else {
                    continue;
                };
                let Some(state) = self
                    .db
                    .sign_session_state(session_id.msg_hash, session_id.era)
                else {
                    continue;
                };
                let Some(request) = state.request else {
                    continue;
                };

                let mut missing = BTreeSet::new();
                if reason.contains("Nonce") {
                    for addr in &request.participants {
                        if !state
                            .nonce_commits
                            .iter()
                            .any(|commit| &commit.from == addr)
                        {
                            missing.insert(*addr);
                        }
                    }
                } else if reason.contains("Partial") {
                    for addr in &request.participants {
                        if !state.sign_shares.iter().any(|share| &share.from == addr) {
                            missing.insert(*addr);
                        }
                    }
                }

                if missing.is_empty() {
                    continue;
                }

                let entry = self.excluded.entry(session_id).or_default();
                let excluded_count_before = entry.len();
                entry.extend(missing);
                if entry.len() != excluded_count_before {
                    tracing::debug!(
                        era = session_id.era,
                        msg_hash = %session_id.msg_hash,
                        excluded = entry.len(),
                        "Excluded missing signers after timeout"
                    );
                }
            }

            let mut next = actions::coordinator_actions_to_outbound(actions)?;
            messages.append(&mut next);
        }

        let now = Instant::now();
        let sessions: Vec<_> = self.session_progress.keys().copied().collect();
        for session_id in sessions {
            let progress = match self.session_progress.get(&session_id) {
                Some(progress) => progress,
                None => continue,
            };
            let timeout = self
                .db
                .sign_session_state(session_id.msg_hash, session_id.era)
                .map(|state| {
                    if state.sign_shares.is_empty() {
                        self.coordinator_config.nonce_timeout
                    } else {
                        self.coordinator_config.partial_timeout
                    }
                })
                .unwrap_or(self.coordinator_config.nonce_timeout);
            if now.duration_since(progress.last_activity) < timeout {
                continue;
            }

            let next_attempt = progress.attempt.saturating_add(1);
            let mut participants = progress.participants.clone();
            if let Some(excluded) = self.excluded.get(&session_id) {
                participants.retain(|addr| !excluded.contains(addr));
            }
            if participants.len() < progress.threshold as usize {
                tracing::debug!(
                    era = session_id.era,
                    msg_hash = %session_id.msg_hash,
                    participants = participants.len(),
                    threshold = progress.threshold,
                    "Skipping ROAST retry due to insufficient participants"
                );
                continue;
            }

            let next_leader = select_roast_leader(
                &participants,
                session_id.msg_hash,
                session_id.era,
                next_attempt,
            );

            if next_leader == self.self_address {
                let mut next = self.start_as_coordinator(
                    session_id,
                    next_attempt,
                    progress.tweak_target,
                    progress.threshold,
                    participants.clone(),
                )?;
                messages.append(&mut next);
            }

            if let Some(entry) = self.session_progress.get_mut(&session_id) {
                entry.last_activity = now;
                entry.attempt = next_attempt;
                entry.leader = next_leader;
                entry.participants = participants;
                entry.leader_request_seen = next_leader == self.self_address;
            }
        }

        Ok(messages)
    }

    /// Get completed signature
    pub fn get_signature(&self, msg_hash: H256, era: u64) -> Option<SignAggregate> {
        self.db
            .sign_session_state(msg_hash, era)
            .and_then(|state| state.aggregate)
    }

    #[cfg(test)]
    #[cfg(test)]
    #[allow(dead_code)]
    pub fn get_cached_signature(
        &self,
        msg_hash: H256,
        era: u64,
        tweak_target: ActorId,
    ) -> Option<SignAggregate> {
        self.db.signature_cache(era, tweak_target, msg_hash)
    }

    #[allow(dead_code)]
    pub fn get_pre_nonce_cache(
        &self,
        era: u64,
        tweak_target: ActorId,
    ) -> Option<Vec<ethexe_common::crypto::PreNonceCommitment>> {
        self.db.pre_nonce_cache(era, tweak_target)
    }

    #[allow(dead_code)]
    pub fn set_pre_nonce_cache(
        &self,
        era: u64,
        tweak_target: ActorId,
        cache: Vec<ethexe_common::crypto::PreNonceCommitment>,
    ) {
        self.db.set_pre_nonce_cache(era, tweak_target, cache);
    }

    /// Process received aggregate signature
    pub fn process_aggregate(&mut self, aggregate: SignAggregate) -> Result<()> {
        // Store the aggregate signature in the database
        let session_id = roast_session_id(aggregate.msg_hash, aggregate.session.era);

        // Update session state with the aggregate
        let mut state = self
            .db
            .sign_session_state(session_id.msg_hash, session_id.era)
            .unwrap_or_default();
        let request = state.request.clone();
        state.aggregate = Some(aggregate.clone());
        self.db
            .set_sign_session_state(session_id.msg_hash, session_id.era, state);
        if let Some(request) = request {
            cache::store_aggregate(
                &self.db,
                session_id.era,
                request.tweak_target,
                session_id.msg_hash,
                aggregate,
            );
        }

        self.coordinators.remove(&session_id);
        self.participants.remove(&session_id);
        self.session_progress.remove(&session_id);

        Ok(())
    }
}
