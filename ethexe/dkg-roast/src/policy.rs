// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
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

use ethexe_common::{
    Address,
    crypto::{DkgSessionId, SignSessionRequest},
    db::SignSessionState,
};
use gprimitives::H256;
use std::{
    collections::BTreeSet,
    time::{Duration, Instant},
};

/// Lightweight identifier for ROAST sessions (era + message hash).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct RoastSessionId {
    pub msg_hash: H256,
    pub era: u64,
}

/// Builds a ROAST session id from message hash and era.
pub(crate) fn roast_session_id(msg_hash: H256, era: u64) -> RoastSessionId {
    RoastSessionId { msg_hash, era }
}

/// Builds the DKG session id for an era.
pub(crate) fn dkg_session_id(era: u64) -> DkgSessionId {
    DkgSessionId { era }
}

/// Deterministically selects a ROAST leader for the given attempt.
pub fn select_roast_leader(
    participants: &[Address],
    msg_hash: H256,
    era: u64,
    attempt: u32,
) -> Address {
    let mut participants = participants.to_vec();
    participants.sort();
    let mut leader = ethexe_common::crypto::frost::elect_leader(&participants, &msg_hash, era);
    for _ in 0..attempt {
        leader = ethexe_common::crypto::frost::next_leader(leader, &participants);
    }
    leader
}

/// Retry plan containing the next leader and participant set.
#[derive(Debug, Clone)]
pub(crate) struct RoastRetryPlan {
    pub attempt: u32,
    pub leader: Address,
    pub participants: Vec<Address>,
}

/// Timeout stage used to identify missing signers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RoastTimeoutStage {
    Nonce,
    Partial,
}

/// Builds a retry plan for a session, excluding missing signers.
pub(crate) fn build_roast_retry_plan(
    session_id: RoastSessionId,
    current_attempt: u32,
    participants: &[Address],
    threshold: u16,
    excluded: Option<&BTreeSet<Address>>,
) -> Option<RoastRetryPlan> {
    let next_attempt = current_attempt.saturating_add(1);
    let mut participants = participants.to_vec();
    if let Some(excluded) = excluded {
        // Remove excluded signers from the retry set.
        participants.retain(|addr| !excluded.contains(addr));
    }
    // Abort retries if threshold is no longer achievable.
    if participants.len() < threshold as usize {
        return None;
    }
    let leader = select_roast_leader(
        &participants,
        session_id.msg_hash,
        session_id.era,
        next_attempt,
    );
    Some(RoastRetryPlan {
        attempt: next_attempt,
        leader,
        participants,
    })
}

/// Returns the set of signers missing for the given timeout stage.
pub(crate) fn missing_signers_after_timeout(
    stage: RoastTimeoutStage,
    request: &SignSessionRequest,
    state: &SignSessionState,
) -> BTreeSet<Address> {
    let mut missing = BTreeSet::new();
    match stage {
        RoastTimeoutStage::Nonce => {
            for addr in &request.participants {
                if !state
                    .nonce_commits
                    .iter()
                    .any(|commit| &commit.from == addr)
                {
                    missing.insert(*addr);
                }
            }
        }
        RoastTimeoutStage::Partial => {
            for addr in &request.participants {
                if !state.sign_shares.iter().any(|share| &share.from == addr) {
                    missing.insert(*addr);
                }
            }
        }
    }
    missing
}

/// Maps timeout error strings to timeout stages.
pub(crate) fn roast_timeout_stage_from_reason(reason: &str) -> Option<RoastTimeoutStage> {
    if reason.contains("Nonce") {
        Some(RoastTimeoutStage::Nonce)
    } else if reason.contains("Partial") {
        Some(RoastTimeoutStage::Partial)
    } else {
        None
    }
}

/// Infers timeout stage from persisted session state.
pub(crate) fn roast_timeout_stage_from_state(
    state: Option<&SignSessionState>,
) -> RoastTimeoutStage {
    if state.is_some_and(|state| !state.sign_shares.is_empty()) {
        RoastTimeoutStage::Partial
    } else {
        RoastTimeoutStage::Nonce
    }
}

/// Returns the duration for a timeout stage.
pub(crate) fn roast_timeout_duration(
    stage: RoastTimeoutStage,
    nonce_timeout: Duration,
    partial_timeout: Duration,
) -> Duration {
    match stage {
        RoastTimeoutStage::Nonce => nonce_timeout,
        RoastTimeoutStage::Partial => partial_timeout,
    }
}

/// Returns Some(duration) if the timeout has elapsed.
pub(crate) fn roast_timeout_elapsed(
    now: Instant,
    last_activity: Instant,
    stage: RoastTimeoutStage,
    nonce_timeout: Duration,
    partial_timeout: Duration,
) -> Option<Duration> {
    let timeout = roast_timeout_duration(stage, nonce_timeout, partial_timeout);
    if now.duration_since(last_activity) >= timeout {
        Some(timeout)
    } else {
        None
    }
}

/// Returns true if a ROAST request error should trigger DKG restart.
/// Returns true when a ROAST error warrants a retry/restart path.
pub fn is_recoverable_roast_request_error(err: &anyhow::Error) -> bool {
    use crate::engine::roast::{RoastErrorExt, RoastErrorKind};

    // Recoverable errors trigger a DKG restart to refresh key material.
    matches!(
        err.roast_error_kind(),
        Some(
            RoastErrorKind::MissingKeyPackage
                | RoastErrorKind::MissingDkgShare
                | RoastErrorKind::KeyPackageIdentifierMismatch
                | RoastErrorKind::KeyPackageThresholdMismatch
                | RoastErrorKind::DkgShareIndexMismatch
        )
    )
}

/// Policy decision for DKG errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// High-level decision for DKG error handling.
pub enum DkgPolicyDecision {
    Restart,
    Ignore,
}

/// Determines whether to restart DKG after an error.
/// Maps a DKG error into a policy decision.
pub fn dkg_error_policy(err: &anyhow::Error) -> DkgPolicyDecision {
    use crate::engine::dkg::{DkgErrorExt, DkgErrorKind};

    match err.dkg_error_kind() {
        Some(DkgErrorKind::AlreadyInProgress) => DkgPolicyDecision::Ignore,
        _ => DkgPolicyDecision::Restart,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ethexe_common::crypto::SignShare;

    fn sample_share() -> SignShare {
        SignShare {
            session: DkgSessionId { era: 1 },
            from: Address([1u8; 20]),
            msg_hash: H256::zero(),
            partial_sig: Vec::new(),
            next_commitments: Vec::new(),
        }
    }

    #[test]
    fn roast_timeout_stage_from_reason_matches_expected() {
        assert_eq!(
            roast_timeout_stage_from_reason("Nonce timeout"),
            Some(RoastTimeoutStage::Nonce)
        );
        assert_eq!(
            roast_timeout_stage_from_reason("Partial timeout"),
            Some(RoastTimeoutStage::Partial)
        );
        assert_eq!(roast_timeout_stage_from_reason("Other"), None);
    }

    #[test]
    fn roast_timeout_stage_from_state_prefers_partial_after_shares() {
        let empty_state = SignSessionState::default();
        assert_eq!(
            roast_timeout_stage_from_state(Some(&empty_state)),
            RoastTimeoutStage::Nonce
        );

        let mut with_shares = SignSessionState::default();
        with_shares.sign_shares.push(sample_share());
        assert_eq!(
            roast_timeout_stage_from_state(Some(&with_shares)),
            RoastTimeoutStage::Partial
        );
        assert_eq!(
            roast_timeout_stage_from_state(None),
            RoastTimeoutStage::Nonce
        );
    }

    #[test]
    fn roast_timeout_elapsed_tracks_stage_deadlines() {
        let now = Instant::now();
        let last_activity = now - Duration::from_secs(5);
        let nonce_timeout = Duration::from_secs(2);
        let partial_timeout = Duration::from_secs(10);

        let elapsed = roast_timeout_elapsed(
            now,
            last_activity,
            RoastTimeoutStage::Nonce,
            nonce_timeout,
            partial_timeout,
        );
        assert_eq!(elapsed, Some(nonce_timeout));

        let not_elapsed = roast_timeout_elapsed(
            now,
            last_activity,
            RoastTimeoutStage::Partial,
            nonce_timeout,
            partial_timeout,
        );
        assert_eq!(not_elapsed, None);
    }

    #[test]
    fn build_roast_retry_plan_filters_excluded_and_bumps_attempt() {
        let session_id = RoastSessionId {
            msg_hash: H256::zero(),
            era: 7,
        };
        let participants = vec![Address([1u8; 20]), Address([2u8; 20]), Address([3u8; 20])];
        let mut excluded = BTreeSet::new();
        excluded.insert(Address([2u8; 20]));

        let plan =
            build_roast_retry_plan(session_id, 0, &participants, 2, Some(&excluded)).unwrap();
        assert_eq!(plan.attempt, 1);
        assert_eq!(plan.participants.len(), 2);
        assert!(!plan.participants.contains(&Address([2u8; 20])));
    }

    #[test]
    fn build_roast_retry_plan_returns_none_when_below_threshold() {
        let session_id = RoastSessionId {
            msg_hash: H256::zero(),
            era: 7,
        };
        let participants = vec![Address([1u8; 20]), Address([2u8; 20])];
        let mut excluded = BTreeSet::new();
        excluded.insert(Address([2u8; 20]));

        let plan = build_roast_retry_plan(session_id, 0, &participants, 2, Some(&excluded));
        assert!(plan.is_none());
    }
}
