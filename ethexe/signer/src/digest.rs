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

//! Keccak256 digest type.
//!
//! Implements `ToDigest` hashing for ethexe common types.

use core::fmt;
use ethexe_common::{
    gear::{
        BatchCommitment, BlockCommitment, CodeCommitment, Message, StateTransition, ValueClaim,
    },
    ProducerBlock,
};
use parity_scale_codec::{Decode, Encode};
use sha3::Digest as _;

/// Common digest type for the ethexe.
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
pub struct Digest(pub(crate) [u8; 32]);

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

impl<T: ToDigest> ToDigest for [T] {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        for item in self {
            hasher.update(item.to_digest().as_ref());
        }
    }
}

impl<T: ToDigest> ToDigest for Vec<T> {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        self.as_slice().update_hasher(hasher);
    }
}

impl ToDigest for [u8] {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        hasher.update(self);
    }
}

impl<T: ToDigest + ?Sized> ToDigest for &T {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        (*self).update_hasher(hasher);
    }
}

impl ToDigest for CodeCommitment {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        // To avoid missing incorrect hashing while developing.
        let Self {
            id,
            timestamp,
            valid,
        } = self;

        hasher.update(id.into_bytes().as_slice());
        hasher.update(ethexe_common::u64_into_uint48_be_bytes_lossy(*timestamp).as_slice());
        hasher.update([*valid as u8]);
    }
}

impl ToDigest for StateTransition {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        // To avoid missing incorrect hashing while developing.
        let Self {
            actor_id,
            new_state_hash,
            inheritor,
            value_to_receive,
            value_claims,
            messages,
        } = self;

        hasher.update(actor_id.to_address_lossy().as_bytes());
        hasher.update(new_state_hash.as_bytes());
        hasher.update(inheritor.to_address_lossy().as_bytes());
        hasher.update(value_to_receive.to_be_bytes().as_slice());

        let mut value_hasher = sha3::Keccak256::new();
        for value_claim in value_claims {
            // To avoid missing incorrect hashing while developing.
            let ValueClaim {
                message_id,
                destination,
                value,
            } = value_claim;

            value_hasher.update(message_id.as_ref());
            value_hasher.update(destination.to_address_lossy().as_bytes());
            value_hasher.update(value.to_be_bytes().as_slice());
        }
        hasher.update(value_hasher.finalize().as_slice());

        hasher.update(messages.to_digest().as_ref());
    }
}

impl ToDigest for Message {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        // To avoid missing incorrect hashing while developing.
        let Self {
            id,
            destination,
            payload,
            value,
            reply_details,
        } = self;

        let (reply_details_to, reply_details_code) = reply_details.unwrap_or_default().into_parts();

        hasher.update(id.as_ref());
        hasher.update(destination.to_address_lossy().as_bytes());
        hasher.update(payload.as_slice());
        hasher.update(value.to_be_bytes().as_slice());
        hasher.update(reply_details_to.as_ref());
        hasher.update(reply_details_code.to_bytes().as_slice());
    }
}

impl ToDigest for BlockCommitment {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        // To avoid missing incorrect hashing while developing.
        let Self {
            hash,
            timestamp,
            previous_committed_block,
            predecessor_block,
            transitions,
        } = self;

        hasher.update(hash.as_bytes());
        hasher.update(ethexe_common::u64_into_uint48_be_bytes_lossy(*timestamp).as_slice());
        hasher.update(previous_committed_block.as_bytes());
        hasher.update(predecessor_block.as_bytes());
        hasher.update(transitions.to_digest().as_ref());
    }
}

impl ToDigest for BatchCommitment {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        // To avoid missing incorrect hashing while developing.
        let Self {
            code_commitments,
            block_commitments,
        } = self;

        hasher.update(block_commitments.to_digest().as_ref());
        hasher.update(code_commitments.to_digest().as_ref());
        hasher.update([0u8; 0].to_digest().as_ref()); // Placeholder for the rewards commitment
    }
}

impl ToDigest for ProducerBlock {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        hasher.update(self.block_hash.as_bytes());
        hasher.update(self.gas_allowance.encode().as_slice());
        hasher.update(self.off_chain_transactions.encode().as_slice());
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
            timestamp: 0,
            valid: true,
        }
        .to_digest();

        let state_transition = StateTransition {
            actor_id: ActorId::from(0),
            new_state_hash: H256::from([1; 32]),
            inheritor: ActorId::from(0),
            value_to_receive: 0,
            value_claims: vec![],
            messages: vec![Message {
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
            hash: H256::from([0; 32]),
            timestamp: 0,
            previous_committed_block: H256::from([2; 32]),
            predecessor_block: H256::from([1; 32]),
            transitions: transitions.clone(),
        };
        let _digest = block_commitment.to_digest();
    }
}
