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

use super::super::{RoastManager, actions, cache, types::RoastMessage};
use crate::{
    engine::{
        roast::storage::coordinator::{CoordinatorAction, CoordinatorEvent},
        storage::RoastStore,
    },
    policy::{
        RoastSessionId, RoastTimeoutStage, build_roast_retry_plan, missing_signers_after_timeout,
        roast_session_id,
    },
};
use anyhow::Result;
use ethexe_common::crypto::{
    SignAggregate, SignCulprits, SignNonceCommit, SignSessionRequest, SignShare,
};
use std::time::Instant;

impl<DB> RoastManager<DB>
where
    DB: RoastStore,
{
    /// Processes a nonce commitment on the coordinator path.
    pub fn process_nonce_commit(&mut self, commit: SignNonceCommit) -> Result<Vec<RoastMessage>> {
        let session_id = roast_session_id(commit.msg_hash, commit.session.era);
        // Ignore commits for unknown sessions unless a coordinator is active or persisted.

        // Drop duplicate commits from the same signer.
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
        // Restore coordinator state from DB if needed.
        if !self.coordinators.contains_key(&session_id) {
            let mut restored = self.restore_coordinator(session_id)?;
            messages.append(&mut restored);
        }

        if let Some(coordinator) = self.coordinators.get_mut(&session_id) {
            // Ignore excluded signers for this retry attempt.
            if self
                .excluded
                .get(&session_id)
                .is_some_and(|set| set.contains(&commit.from))
            {
                return Ok(messages);
            }
            // Drive coordinator state machine with the new commit.
            let actions = coordinator.process_event(CoordinatorEvent::NonceCommit(commit))?;
            if let Some(progress) = self.session_progress.get_mut(&session_id) {
                progress.last_activity = Instant::now();
            }
            let has_signing_package = actions
                .iter()
                .any(|action| matches!(action, CoordinatorAction::BroadcastSigningPackage(_)));
            let mut next = actions::coordinator_actions_to_outbound(actions)?;
            messages.append(&mut next);

            // If threshold reached but no package broadcast, consider retry plan.
            // If enough commits arrived but we didn't broadcast a package, retry.
            if !has_signing_package
                && let Some(state) = self
                    .db
                    .sign_session_state(session_id.msg_hash, session_id.era)
                && let Some(request) = state.request.as_ref()
                && state.nonce_commits.len() >= request.threshold as usize
            {
                let missing =
                    missing_signers_after_timeout(RoastTimeoutStage::Nonce, request, &state);

                if !missing.is_empty() {
                    let entry = self.excluded.entry(session_id).or_default();
                    entry.extend(missing);

                    let current_attempt = self
                        .session_progress
                        .get(&session_id)
                        .map(|progress| progress.attempt)
                        .unwrap_or(request.attempt);

                    if let Some(plan) = build_roast_retry_plan(
                        session_id,
                        current_attempt,
                        &request.participants,
                        request.threshold,
                        Some(entry),
                    ) {
                        let participants = plan.participants;
                        if plan.leader == self.self_address {
                            let mut next = self.start_as_coordinator(
                                session_id,
                                plan.attempt,
                                request.tweak_target,
                                request.threshold,
                                participants.clone(),
                            )?;
                            messages.append(&mut next);
                        } else {
                            let retry_request = SignSessionRequest {
                                session: request.session,
                                leader: plan.leader,
                                attempt: plan.attempt,
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
                            progress.attempt = plan.attempt;
                            progress.leader = plan.leader;
                            progress.participants = participants;
                            progress.leader_request_seen = plan.leader == self.self_address;
                        }
                    }
                }
            }
            Ok(messages)
        } else {
            Ok(messages)
        }
    }

    /// Processes a partial signature on the coordinator path.
    pub fn process_partial_signature(&mut self, partial: SignShare) -> Result<Vec<RoastMessage>> {
        let session_id = roast_session_id(partial.msg_hash, partial.session.era);
        // Ignore partials for unknown sessions unless a coordinator is active or persisted.

        // Drop duplicate partials from the same signer.
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
        // Restore coordinator state from DB if needed.
        if !self.coordinators.contains_key(&session_id) {
            let mut restored = self.restore_coordinator(session_id)?;
            messages.append(&mut restored);
        }

        if let Some(coordinator) = self.coordinators.get_mut(&session_id) {
            // Ignore excluded signers for this retry attempt.
            if self
                .excluded
                .get(&session_id)
                .is_some_and(|set| set.contains(&partial.from))
            {
                return Ok(messages);
            }
            // Drive coordinator state machine with the new partial.
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

    /// Records culprits for exclusion during retries.
    pub fn process_culprits(&mut self, culprits: SignCulprits) -> Result<()> {
        // Exclude malicious signers for future retries.
        let session_id = roast_session_id(culprits.msg_hash, culprits.session.era);
        let entry = self.excluded.entry(session_id).or_default();
        for culprit in culprits.culprits {
            entry.insert(culprit);
        }
        Ok(())
    }

    /// Process received aggregate signature.
    pub fn process_aggregate(&mut self, aggregate: SignAggregate) -> Result<()> {
        // Store the aggregate signature in the database.
        let session_id = roast_session_id(aggregate.msg_hash, aggregate.session.era);

        // Update session state with the aggregate.
        let mut state = self
            .db
            .sign_session_state(session_id.msg_hash, session_id.era)
            .unwrap_or_default();
        let tweak_target = state.request.as_ref().map(|request| request.tweak_target);
        state.aggregate = Some(aggregate.clone());
        self.db
            .set_sign_session_state(session_id.msg_hash, session_id.era, state);
        // Cache aggregate for quick lookup in future sessions.
        if let Some(tweak_target) = tweak_target {
            cache::store_aggregate(
                &self.db,
                session_id.era,
                tweak_target,
                session_id.msg_hash,
                aggregate,
            );
        }

        // Clean up in-memory state once aggregate is persisted.
        self.coordinators.remove(&session_id);
        self.participants.remove(&session_id);
        self.session_progress.remove(&session_id);

        Ok(())
    }

    /// Restores coordinator state for a session when persisted state exists.
    fn restore_coordinator(&mut self, session_id: RoastSessionId) -> Result<Vec<RoastMessage>> {
        // Attempt to rebuild coordinator from persisted session state.
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
}
