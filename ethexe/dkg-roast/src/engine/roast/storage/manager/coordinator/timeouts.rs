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

use super::super::{RoastManager, actions, types::RoastMessage};
use crate::{
    engine::{
        roast::{
            core::RoastResult,
            storage::coordinator::{CoordinatorAction, CoordinatorEvent},
        },
        storage::RoastStore,
    },
    policy::{
        build_roast_retry_plan, dkg_session_id, missing_signers_after_timeout,
        roast_timeout_elapsed, roast_timeout_stage_from_reason, roast_timeout_stage_from_state,
    },
};
use anyhow::Result;
use ethexe_common::crypto::{SignKind, SignSessionRequest};
use std::time::Instant;

impl<DB> RoastManager<DB>
where
    DB: RoastStore,
{
    /// Drives timeout-based retries for coordinator and participant sessions.
    pub fn process_timeouts(&mut self) -> Result<Vec<RoastMessage>> {
        if !self.coordinators.is_empty()
            || !self.participants.is_empty()
            || !self.session_progress.is_empty()
        {
            // Only log when there is active ROAST state to process.
            tracing::debug!(
                coordinators = self.coordinators.len(),
                participants = self.participants.len(),
                sessions = self.session_progress.len(),
                "ROAST timeout sweep"
            );
        }
        let mut messages = vec![];
        // First pass: drive coordinator timeouts and track missing signers.
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
                // Record missing signers for retry plan when a timeout fails a session.
                let Some(state) = self
                    .db
                    .sign_session_state(session_id.msg_hash, session_id.era)
                else {
                    continue;
                };
                let Some(ref request) = state.request else {
                    continue;
                };

                let Some(stage) = roast_timeout_stage_from_reason(reason) else {
                    continue;
                };
                let missing = missing_signers_after_timeout(stage, request, &state);

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

        // Second pass: drive participant-side session progress timeouts.
        let now = Instant::now();
        let sessions: Vec<_> = self.session_progress.keys().copied().collect();
        for session_id in sessions {
            let progress = match self.session_progress.get(&session_id) {
                Some(progress) => progress,
                None => continue,
            };
            let state = self
                .db
                .sign_session_state(session_id.msg_hash, session_id.era);
            // Determine whether we are waiting for nonces or partials.
            let stage = roast_timeout_stage_from_state(state.as_ref());
            let Some(timeout) = roast_timeout_elapsed(
                now,
                progress.last_activity,
                stage,
                self.coordinator_config.nonce_timeout,
                self.coordinator_config.partial_timeout,
            ) else {
                continue;
            };

            tracing::debug!(
                era = session_id.era,
                msg_hash = %session_id.msg_hash,
                attempt = progress.attempt,
                timeout = ?timeout,
                "ROAST session timeout triggered"
            );

            // Reset participant state before retrying.
            self.participants.remove(&session_id);

            // Build retry plan with exclusions; skip if below threshold.
            let Some(plan) = build_roast_retry_plan(
                session_id,
                progress.attempt,
                &progress.participants,
                progress.threshold,
                self.excluded.get(&session_id),
            ) else {
                tracing::debug!(
                    era = session_id.era,
                    msg_hash = %session_id.msg_hash,
                    participants = progress.participants.len(),
                    threshold = progress.threshold,
                    "Skipping ROAST retry due to insufficient participants"
                );
                continue;
            };

            tracing::debug!(
                era = session_id.era,
                msg_hash = %session_id.msg_hash,
                next_attempt = plan.attempt,
                next_leader = %plan.leader,
                participants = plan.participants.len(),
                "ROAST retry leader selected"
            );

            let participants = plan.participants;
            if plan.leader == self.self_address {
                // Resume as coordinator if we are the new leader.
                let mut next = self.start_as_coordinator(
                    session_id,
                    plan.attempt,
                    progress.tweak_target,
                    progress.threshold,
                    participants.clone(),
                )?;
                messages.append(&mut next);
            } else {
                let request = SignSessionRequest {
                    session: dkg_session_id(session_id.era),
                    leader: plan.leader,
                    attempt: plan.attempt,
                    msg_hash: session_id.msg_hash,
                    tweak_target: progress.tweak_target,
                    threshold: progress.threshold,
                    participants: participants.clone(),
                    kind: SignKind::ArbitraryHash,
                };
                tracing::debug!(
                    era = session_id.era,
                    msg_hash = %session_id.msg_hash,
                    next_attempt = plan.attempt,
                    next_leader = %plan.leader,
                    "ROAST broadcasting retry sign request"
                );
                messages.push(RoastMessage::SignSessionRequest(request));
            }

            // Update session progress tracking for the new attempt.
            if let Some(entry) = self.session_progress.get_mut(&session_id) {
                entry.last_activity = now;
                entry.attempt = plan.attempt;
                entry.leader = plan.leader;
                entry.participants = participants;
                entry.leader_request_seen = plan.leader == self.self_address;
            }
        }

        Ok(messages)
    }
}
