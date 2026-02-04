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

use super::{RoastManager, actions, cache, session, types::RoastMessage};
use crate::{
    engine::{
        prelude::{ParticipantEvent, RoastParticipant},
        storage::RoastStore,
    },
    policy::{roast_session_id, select_roast_leader},
};
use anyhow::Result;
use ethexe_common::{
    Address,
    crypto::{SignNoncePackage, SignSessionRequest},
    db::SignSessionState,
};
use std::time::Instant;

impl<DB> RoastManager<DB>
where
    DB: RoastStore,
{
    /// Processes a sign request on the participant path.
    pub fn process_sign_request(
        &mut self,
        from: Address,
        request: SignSessionRequest,
    ) -> Result<Vec<RoastMessage>> {
        // Keep caches bounded for the target era.
        cache::prune_caches_if_needed(&self.db, request.session.era);

        // Return cached aggregate when available.
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

        // Drop stale attempts or conflicting leader requests.
        if let Some(progress) = self.session_progress.get(&session_id) {
            tracing::debug!(
                era = request.session.era,
                msg_hash = %request.msg_hash,
                from = %from,
                attempt = request.attempt,
                progress_attempt = progress.attempt,
                progress_leader = %progress.leader,
                request_leader = %request.leader,
                "ROAST sign request progress check"
            );
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

        // Enforce deterministic participant ordering and leader election.
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
            // If we are the leader, but the request came from elsewhere, restart locally.
            if from != request.leader {
                tracing::debug!(
                    era = request.session.era,
                    msg_hash = %request.msg_hash,
                    attempt = request.attempt,
                    "ROAST sign request restart as coordinator"
                );
                return self.start_as_coordinator(
                    roast_session_id(request.msg_hash, request.session.era),
                    request.attempt,
                    request.tweak_target,
                    request.threshold,
                    request.participants,
                );
            }
        } else if from != request.leader {
            // Track progress for visibility but do not participate.
            let should_update = self
                .session_progress
                .get(&session_id)
                .map(|progress| request.attempt >= progress.attempt)
                .unwrap_or(true);
            if should_update {
                tracing::debug!(
                    era = request.session.era,
                    msg_hash = %request.msg_hash,
                    attempt = request.attempt,
                    leader = %request.leader,
                    "ROAST sign request observed from different peer; tracking progress"
                );
                self.session_progress.insert(
                    session_id,
                    super::types::SessionProgress {
                        last_activity: Instant::now(),
                        attempt: request.attempt,
                        participants: request.participants,
                        threshold: request.threshold,
                        tweak_target: request.tweak_target,
                        leader: request.leader,
                        leader_request_seen: false,
                    },
                );
            }
            return Ok(vec![]);
        }

        // Load DKG material required to participate.
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
        // Check that our share index matches the participant list.
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

        // Build identifier map (from DKG state when available).
        let identifiers =
            session::identifiers_for_session(&self.db, request.session.era, &request.participants)?;
        // Consume one pre-generated nonce if available.
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
        let attempt = request.attempt;
        let threshold = request.threshold;
        let tweak_target = request.tweak_target;
        let leader = request.leader;
        let participants = request.participants.clone();

        // Ensure sign session state exists for coordinator recovery.
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
            request,
            key_package: Box::new(key_package),
            identifiers,
            pre_nonce,
        })?;

        self.participants.insert(session_id, participant);
        self.session_progress.insert(
            session_id,
            super::types::SessionProgress {
                last_activity: Instant::now(),
                attempt,
                participants,
                threshold,
                tweak_target,
                leader,
                leader_request_seen: true,
            },
        );

        actions::participant_actions_to_outbound(actions)
    }

    /// Processes a signing package from the coordinator (participant).
    pub fn process_nonce_package(
        &mut self,
        package: SignNoncePackage,
    ) -> Result<Vec<RoastMessage>> {
        let session_id = roast_session_id(package.msg_hash, package.session.era);

        if let Some(participant) = self.participants.get_mut(&session_id) {
            let actions = participant.process_event(ParticipantEvent::SigningPackage(package))?;
            if let Some(progress) = self.session_progress.get_mut(&session_id) {
                progress.last_activity = Instant::now();
            }
            return actions::participant_actions_to_outbound(actions);
        }
        Ok(vec![])
    }
}
