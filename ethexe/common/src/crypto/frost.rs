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

//! # FROST Threshold Signature Module
//!
//! This module implements FROST (Flexible Round-Optimized Schnorr Threshold) signatures
//! with ROAST (Robust Asynchronous Schnorr Threshold) coordinator for asynchronous signing.
//!
//! ## Protocol Phases
//!
//! 1. **Session Initiation**: Leader broadcasts signing request
//! 2. **Nonce Commitment**: Participants commit to nonces
//! 3. **Partial Signatures**: Participants compute and send partial signatures
//! 4. **Aggregation**: Leader aggregates partials into final signature
//!
//! ## Key Features
//!
//! - **Key Tweaking**: Support for ActorId-specific key tweaking
//! - **Leader Election**: Deterministic leader selection with failover
//! - **Byzantine Tolerance**: Invalid partials are rejected, signing continues

use super::dkg::DkgSessionId;
use crate::{Address, ToDigest};
use alloc::vec::Vec;
use gprimitives::{ActorId, H256};
use parity_scale_codec::{Decode, Encode};
use sha3::{Digest as _, Keccak256};

/// Type of message being signed
#[derive(Clone, Copy, Debug, Encode, Decode, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum SignKind {
    /// Signing a batch commitment hash
    BatchCommitment = 0,
    /// Signing an arbitrary pre-hashed message
    ArbitraryHash = 1,
}

/// Session request to initiate FROST threshold signing
///
/// The leader broadcasts this to all participants to start a signing session.
/// Participants use this to synchronize on what message to sign and with which parameters.
///
/// ## Leader Selection
/// Leader is deterministically elected based on:
/// - Validator set for the era
/// - Message hash being signed
/// - Era index
///
/// This ensures all participants agree on who the leader is without coordination.
#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq, Hash)]
pub struct SignSessionRequest {
    /// DKG session (era) to use for signing
    pub session: DkgSessionId,

    /// Elected leader address
    pub leader: Address,

    /// Leader election attempt counter
    pub attempt: u32,

    /// Hash of message to sign
    /// For BatchCommitment: digest of BatchCommitment
    /// For ArbitraryHash: keccak256 of arbitrary data
    pub msg_hash: H256,

    /// ActorId for key tweaking
    /// This allows deriving contract-specific keys from the same base key
    pub tweak_target: ActorId,

    /// Threshold (minimum signatures needed)
    pub threshold: u16,

    /// Sorted list of participants for this session
    /// Used for computing Lagrange coefficients
    pub participants: Vec<Address>,

    /// Type of message being signed
    pub kind: SignKind,
}

impl ToDigest for SignSessionRequest {
    fn update_hasher(&self, hasher: &mut Keccak256) {
        self.session.update_hasher(hasher);
        hasher.update(self.leader);
        hasher.update(self.attempt.to_be_bytes());
        hasher.update(self.msg_hash.as_bytes());
        hasher.update(self.tweak_target.as_ref());
        hasher.update(self.threshold.to_be_bytes());
        for participant in &self.participants {
            hasher.update(participant);
        }
        hasher.update([self.kind as u8]);
    }
}

/// Nonce commitment from participant
///
/// Each participant generates a random nonce and commits to it by sending
/// the nonce commitment (R_i = k_i * G) to the leader.
///
/// ## Security
/// - Nonce MUST be fresh and never reused
/// - Nonce is kept secret until partial signature phase
/// - Commitment allows verifying partial signature later
#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq, Hash)]
pub struct SignNonceCommit {
    /// DKG session identifier
    pub session: DkgSessionId,

    /// Participant sending this nonce commitment
    pub from: Address,

    /// Message hash being signed
    pub msg_hash: H256,

    /// Serialized signing commitments (hiding + binding)
    pub nonce_commit: Vec<u8>,
}

impl ToDigest for SignNonceCommit {
    fn update_hasher(&self, hasher: &mut Keccak256) {
        self.session.update_hasher(hasher);
        hasher.update(self.from);
        hasher.update(self.msg_hash.as_bytes());
        hasher.update(&self.nonce_commit);
    }
}

/// Signing package from coordinator (selected signer commitments)
#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq, Hash)]
pub struct SignNoncePackage {
    /// DKG session identifier
    pub session: DkgSessionId,

    /// Message hash being signed
    pub msg_hash: H256,

    /// Serialized signing commitments from selected signers
    pub commitments: Vec<(Address, Vec<u8>)>,
}

impl ToDigest for SignNoncePackage {
    fn update_hasher(&self, hasher: &mut Keccak256) {
        self.session.update_hasher(hasher);
        hasher.update(self.msg_hash.as_bytes());
        for (addr, commit) in &self.commitments {
            hasher.update(addr);
            hasher.update(commit);
        }
    }
}

/// Malicious signer report for a signing session
#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq, Hash)]
pub struct SignCulprits {
    /// DKG session identifier
    pub session: DkgSessionId,

    /// Message hash being signed
    pub msg_hash: H256,

    /// Reported malicious participants
    pub culprits: Vec<Address>,
}

impl ToDigest for SignCulprits {
    fn update_hasher(&self, hasher: &mut Keccak256) {
        self.session.update_hasher(hasher);
        hasher.update(self.msg_hash.as_bytes());
        for culprit in &self.culprits {
            hasher.update(culprit);
        }
    }
}

/// Partial signature from participant
///
/// After nonce commitments are collected, each participant computes their
/// partial signature using:
/// - Their secret share (possibly tweaked)
/// - Their nonce
/// - Aggregated nonce commitment from all signers
/// - Lagrange coefficient for their index
///
/// ## Verification
/// Leader verifies each partial before aggregation:
/// g^z_i == R_i + c * λ_i * PK_i
/// where:
/// - z_i is the partial signature
/// - R_i is the nonce commitment
/// - c is the challenge
/// - λ_i is the Lagrange coefficient
/// - PK_i is the participant's public share
#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq, Hash)]
pub struct SignShare {
    /// DKG session identifier
    pub session: DkgSessionId,

    /// Participant sending this partial signature
    pub from: Address,

    /// Message hash being signed
    pub msg_hash: H256,

    /// Serialized FROST signature share
    pub partial_sig: Vec<u8>,

    /// Fresh signing commitments for the next session
    pub next_commitments: Vec<u8>,
}

impl ToDigest for SignShare {
    fn update_hasher(&self, hasher: &mut Keccak256) {
        self.session.update_hasher(hasher);
        hasher.update(self.from);
        hasher.update(self.msg_hash.as_bytes());
        hasher.update(&self.partial_sig);
        hasher.update(&self.next_commitments);
    }
}

/// Aggregated signature from leader
///
/// After collecting threshold partial signatures, the leader aggregates them
/// into a final Schnorr signature.
///
/// ## Signature Format (96 bytes)
/// - R_x: x-coordinate of aggregated nonce (32 bytes, big-endian)
/// - R_y: y-coordinate of aggregated nonce (32 bytes, big-endian)
/// - z: aggregated signature scalar (32 bytes, big-endian)
///
/// ## Verification
/// On-chain verification: R == z*G - c*PK_tweaked
/// where:
/// - R = (R_x, R_y)
/// - PK_tweaked = PK_agg + hash_to_scalar(ActorId) * G
/// - c = hash(R, PK_tweaked, msg_hash)
#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq, Hash)]
pub struct SignAggregate {
    /// DKG session identifier
    pub session: DkgSessionId,

    /// Message hash that was signed
    pub msg_hash: H256,

    /// Tweaked public key used for signing
    pub tweaked_pk: [u8; 33],

    /// Aggregated signature (R_x || R_y || z) - 96 bytes total
    pub signature96: [u8; 96],
}

/// Pre-generated nonce cache entry for a signer.
#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq)]
pub struct PreNonceCommitment {
    /// Serialized signing commitments (hiding + binding).
    pub commitments: Vec<u8>,
    /// Serialized signing nonces (must never be reused).
    pub nonces: Vec<u8>,
}

impl ToDigest for SignAggregate {
    fn update_hasher(&self, hasher: &mut Keccak256) {
        self.session.update_hasher(hasher);
        hasher.update(self.msg_hash.as_bytes());
        hasher.update(self.tweaked_pk);
        hasher.update(self.signature96);
    }
}

impl SignAggregate {
    /// Extracts (R_x, R_y, z) from the 96-byte signature.
    pub fn signature_components(&self) -> ([u8; 32], [u8; 32], [u8; 32]) {
        let mut r_x = [0u8; 32];
        let mut r_y = [0u8; 32];
        let mut z = [0u8; 32];

        r_x.copy_from_slice(&self.signature96[0..32]);
        r_y.copy_from_slice(&self.signature96[32..64]);
        z.copy_from_slice(&self.signature96[64..96]);

        (r_x, r_y, z)
    }

    /// Creates a `SignAggregate` from signature components.
    pub fn from_components(
        session: DkgSessionId,
        msg_hash: H256,
        tweaked_pk: [u8; 33],
        r_x: [u8; 32],
        r_y: [u8; 32],
        z: [u8; 32],
    ) -> Self {
        let mut signature96 = [0u8; 96];
        signature96[0..32].copy_from_slice(&r_x);
        signature96[32..64].copy_from_slice(&r_y);
        signature96[64..96].copy_from_slice(&z);

        Self {
            session,
            msg_hash,
            tweaked_pk,
            signature96,
        }
    }
}

/// Leader election helper
///
/// Deterministically selects a leader from the validator set based on
/// the message hash and era. This ensures all participants agree on
/// the same leader without coordination.
/// Deterministically selects the ROAST leader for a session.
pub fn elect_leader(validators: &[Address], msg_hash: &H256, era: u64) -> Address {
    let mut sorted_validators = validators.to_vec();
    sorted_validators.sort();

    let mut hasher = Keccak256::new();
    hasher.update(b"ROAST_LEADER_ELECTION");
    hasher.update(msg_hash.as_bytes());
    hasher.update(era.to_le_bytes());
    let hash = hasher.finalize();

    let seed = u64::from_be_bytes(hash[0..8].try_into().unwrap());
    let idx = (seed % sorted_validators.len() as u64) as usize;

    sorted_validators[idx]
}

/// Fallback leader selection on timeout
///
/// If the current leader fails to respond, elect the next leader
/// in deterministic round-robin order.
/// Picks the next leader in sorted validator order (round-robin).
pub fn next_leader(current: Address, validators: &[Address]) -> Address {
    let mut sorted = validators.to_vec();
    sorted.sort();

    let current_idx = sorted.iter().position(|&v| v == current).unwrap_or(0);
    let next_idx = (current_idx + 1) % sorted.len();

    sorted[next_idx]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sign_kind_encoding() {
        assert_eq!(SignKind::BatchCommitment as u8, 0);
        assert_eq!(SignKind::ArbitraryHash as u8, 1);
    }

    #[test]
    fn test_sign_session_request_digest() {
        let request = SignSessionRequest {
            session: DkgSessionId { era: 1 },
            leader: Address([1u8; 20]),
            attempt: 0,
            msg_hash: H256([42u8; 32]),
            tweak_target: ActorId::from([1u8; 32]),
            threshold: 3,
            participants: vec![Address([1u8; 20]), Address([2u8; 20])],
            kind: SignKind::BatchCommitment,
        };

        let digest1 = request.to_digest();
        let digest2 = request.to_digest();
        assert_eq!(digest1, digest2, "Digest should be deterministic");
    }

    #[test]
    fn test_signature_components() {
        let r_x = [1u8; 32];
        let r_y = [2u8; 32];
        let z = [3u8; 32];

        let agg = SignAggregate::from_components(
            DkgSessionId { era: 1 },
            H256([0u8; 32]),
            [0u8; 33],
            r_x,
            r_y,
            z,
        );

        let (extracted_rx, extracted_ry, extracted_z) = agg.signature_components();
        assert_eq!(extracted_rx, r_x);
        assert_eq!(extracted_ry, r_y);
        assert_eq!(extracted_z, z);
    }

    #[test]
    fn test_signature96_layout_matches_components() {
        let r_x = [0x11u8; 32];
        let r_y = [0x22u8; 32];
        let z = [0x33u8; 32];

        let aggregate = SignAggregate::from_components(
            DkgSessionId { era: 9 },
            H256([9u8; 32]),
            [9u8; 33],
            r_x,
            r_y,
            z,
        );

        let mut expected = [0u8; 96];
        expected[0..32].copy_from_slice(&r_x);
        expected[32..64].copy_from_slice(&r_y);
        expected[64..96].copy_from_slice(&z);

        assert_eq!(
            aggregate.signature96, expected,
            "signature96 must be R_x || R_y || z"
        );
    }

    #[test]
    fn test_leader_election_deterministic() {
        let validators = vec![Address([1u8; 20]), Address([2u8; 20]), Address([3u8; 20])];
        let msg_hash = H256([42u8; 32]);
        let era = 1;

        let leader1 = elect_leader(&validators, &msg_hash, era);
        let leader2 = elect_leader(&validators, &msg_hash, era);

        assert_eq!(leader1, leader2, "Leader election should be deterministic");
    }

    #[test]
    fn test_leader_election_with_different_order() {
        let validators1 = vec![Address([3u8; 20]), Address([1u8; 20]), Address([2u8; 20])];
        let validators2 = vec![Address([1u8; 20]), Address([2u8; 20]), Address([3u8; 20])];
        let msg_hash = H256([42u8; 32]);
        let era = 1;

        let leader1 = elect_leader(&validators1, &msg_hash, era);
        let leader2 = elect_leader(&validators2, &msg_hash, era);

        assert_eq!(
            leader1, leader2,
            "Leader should be same regardless of input order (sorted internally)"
        );
    }

    #[test]
    fn test_next_leader_rotation() {
        let validators = vec![Address([1u8; 20]), Address([2u8; 20]), Address([3u8; 20])];

        let leader1 = Address([1u8; 20]);
        let leader2 = next_leader(leader1, &validators);
        assert_eq!(leader2, Address([2u8; 20]));

        let leader3 = next_leader(leader2, &validators);
        assert_eq!(leader3, Address([3u8; 20]));

        let leader4 = next_leader(leader3, &validators);
        assert_eq!(leader4, Address([1u8; 20]), "Should wrap around");
    }
}
