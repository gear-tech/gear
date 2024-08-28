// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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

//! Keccak256 digest type. Implements AsDigest hashing for ethexe common types.

use core::fmt;
use ethexe_common::router::{BlockCommitment, CodeCommitment, OutgoingMessage, StateTransition};
use parity_scale_codec::{Decode, Encode};
use sha3::Digest as _;

#[derive(
    Clone,
    Copy,
    PartialOrd,
    Ord,
    PartialEq,
    Eq,
    Hash,
    Encode,
    Decode,
    derive_more::From,
    derive_more::Into,
    derive_more::AsRef,
)]
pub struct Digest([u8; 32]);

impl fmt::Debug for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{}", hex::encode(self.0))
    }
}

impl fmt::Display for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{}", hex::encode(self.0))
    }
}

impl<'a> FromIterator<&'a Digest> for Digest {
    fn from_iter<I: IntoIterator<Item = &'a Digest>>(iter: I) -> Self {
        let mut hasher = sha3::Keccak256::new();
        for digest in iter {
            hasher.update(digest.as_ref());
        }
        Digest(hasher.finalize().into())
    }
}

impl FromIterator<Digest> for Digest {
    fn from_iter<I: IntoIterator<Item = Digest>>(iter: I) -> Self {
        let mut hasher = sha3::Keccak256::new();
        for digest in iter {
            hasher.update(digest.as_ref());
        }
        Digest(hasher.finalize().into())
    }
}

/// Trait for hashing types into a Digest using Keccak256.
pub trait ToDigest {
    fn to_digest(&self) -> Digest {
        let mut hasher = sha3::Keccak256::new();
        self.update_hasher(&mut hasher);
        Digest(hasher.finalize().into())
    }

    fn update_hasher(&self, hasher: &mut sha3::Keccak256);
}

impl<'a, T: ToDigest> FromIterator<&'a T> for Digest {
    fn from_iter<I: IntoIterator<Item = &'a T>>(iter: I) -> Self {
        iter.into_iter().map(|item| item.to_digest()).collect()
    }
}

impl ToDigest for [u8] {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        hasher.update(self);
    }
}

impl ToDigest for CodeCommitment {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        hasher.update(self.encode().as_slice());
    }
}

impl ToDigest for StateTransition {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        hasher.update(self.actor_id.to_address_lossy().as_bytes());
        hasher.update(self.new_state_hash.as_bytes());
        hasher.update(self.value_to_receive.to_be_bytes().as_slice());

        let mut value_hasher = sha3::Keccak256::new();
        for value_claim in &self.value_claims {
            value_hasher.update(value_claim.message_id.as_ref());
            value_hasher.update(value_claim.destination.to_address_lossy().as_bytes());
            value_hasher.update(value_claim.value.to_be_bytes().as_slice());
        }
        hasher.update(value_hasher.finalize().as_slice());

        hasher.update(self.messages.iter().collect::<Digest>().as_ref());
    }
}

impl ToDigest for OutgoingMessage {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        let (reply_details_to, reply_details_code) =
            self.reply_details.unwrap_or_default().into_parts();

        hasher.update(self.id.as_ref());
        hasher.update(self.destination.to_address_lossy().as_bytes());
        hasher.update(self.payload.as_slice());
        hasher.update(self.value.to_be_bytes().as_slice());
        hasher.update(reply_details_to.as_ref());
        hasher.update(reply_details_code.to_bytes().as_slice());
    }
}

impl ToDigest for BlockCommitment {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        hasher.update(self.block_hash.as_bytes());
        hasher.update(self.prev_commitment_hash.as_bytes());
        hasher.update(self.pred_block_hash.as_bytes());
        hasher.update(self.transitions.iter().collect::<Digest>().as_ref());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gprimitives::{ActorId, CodeId, MessageId, H256};
    use std::vec;

    #[test]
    fn as_digest() {
        let _digest = CodeCommitment {
            id: CodeId::from(0),
            valid: true,
        }
        .to_digest();

        let state_transition = StateTransition {
            actor_id: ActorId::from(0),
            new_state_hash: H256::from([1; 32]),
            value_to_receive: 0,
            value_claims: vec![],
            messages: vec![OutgoingMessage {
                id: MessageId::from(0),
                destination: ActorId::from(0),
                payload: b"Hello, World!".to_vec(),
                value: 0,
                reply_details: None,
            }],
        };
        let _digest = state_transition.to_digest();

        let transitions = vec![state_transition.clone(), state_transition];

        let block_commitment = BlockCommitment {
            block_hash: H256::from([0; 32]),
            pred_block_hash: H256::from([1; 32]),
            prev_commitment_hash: H256::from([2; 32]),
            transitions: transitions.clone(),
        };
        let _digest = block_commitment.to_digest();
    }
}
