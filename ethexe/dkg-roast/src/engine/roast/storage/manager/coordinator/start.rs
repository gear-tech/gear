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

use super::super::{
    RoastManager, actions, cache,
    types::{RoastMessage, SessionProgress},
};
use crate::{
    engine::{
        roast::{
            core::SessionConfig,
            storage::coordinator::{CoordinatorEvent, RoastCoordinator},
        },
        storage::RoastStore,
    },
    policy::{RoastSessionId, dkg_session_id, roast_session_id, select_roast_leader},
};
use anyhow::Result;
use ethexe_common::{
    Address,
    crypto::{SignKind, SignSessionRequest},
};
use gprimitives::{ActorId, H256};
use std::time::Instant;

impl<DB> RoastManager<DB>
where
    DB: RoastStore,
{
    /// Starts a ROAST signing session (leader or participant path).
    pub fn start_signing(
        &mut self,
        msg_hash: H256,
        era: u64,
        tweak_target: ActorId,
        threshold: u16,
        mut participants: Vec<Address>,
    ) -> Result<Vec<RoastMessage>> {
        // Prune caches by era to keep memory bounded.
        cache::prune_caches_if_needed(&self.db, era);
        tracing::debug!(
            era,
            msg_hash = %msg_hash,
            participants = participants.len(),
            threshold,
            "ROAST start signing requested"
        );

        // Serve from cache when aggregate is already available.
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

        // Compute session id and elect leader deterministically.
        let session_id = roast_session_id(msg_hash, era);
        let attempt = 0;
        participants.sort();
        let leader = select_roast_leader(&participants, msg_hash, era, attempt);

        if leader == self.self_address {
            tracing::debug!(
                era,
                msg_hash = %msg_hash,
                leader = %leader,
                "ROAST start signing as coordinator"
            );
            // Leader starts as coordinator locally.
            self.start_as_coordinator(session_id, attempt, tweak_target, threshold, participants)
        } else {
            tracing::debug!(
                era,
                msg_hash = %msg_hash,
                leader = %leader,
                "ROAST start signing as participant; broadcasting request"
            );
            // Broadcast request to the elected leader.
            let request = SignSessionRequest {
                session: dkg_session_id(era),
                leader,
                attempt,
                msg_hash,
                tweak_target,
                threshold,
                participants: participants.clone(),
                kind: SignKind::ArbitraryHash,
            };
            // Track session progress to dedupe and gate retries.
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

    /// Starts a signing session as coordinator (leader-only).
    pub(crate) fn start_as_coordinator(
        &mut self,
        session_id: RoastSessionId,
        attempt: u32,
        tweak_target: ActorId,
        threshold: u16,
        participants: Vec<Address>,
    ) -> Result<Vec<RoastMessage>> {
        // Ensure this node is still the elected leader for this attempt.
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

        // Build coordinator state machine with tweaked key.
        let mut coordinator =
            RoastCoordinator::new(self.db.clone(), self.coordinator_config.clone());

        let participants = config.participants.clone();
        let threshold = config.threshold;
        let tweak_target = config.tweak_target;
        let actions = coordinator.process_event(CoordinatorEvent::Start(config))?;
        self.coordinators.insert(session_id, coordinator);
        self.session_progress.insert(
            session_id,
            SessionProgress {
                last_activity: Instant::now(),
                attempt,
                participants,
                threshold,
                tweak_target,
                leader: self.self_address,
                leader_request_seen: true,
            },
        );

        actions::coordinator_actions_to_outbound(actions)
    }
}
