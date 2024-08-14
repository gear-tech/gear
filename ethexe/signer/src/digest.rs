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
use ethexe_common::router::{
    BlockCommitment, CodeCommitment, OutgoingMessage, StateTransition, ValueClaim,
};
use gprimitives::{MessageId, H256};
use parity_scale_codec::{Decode, Encode};
use sha3::Digest as _;

use crate::Address;

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

/// Trait for hashing types into a Digest using Keccak256.
pub trait AsDigest {
    fn as_digest(&self) -> Digest;
}

impl AsDigest for Digest {
    fn as_digest(&self) -> Digest {
        *self
    }
}

impl<T: AsDigest> AsDigest for [T] {
    fn as_digest(&self) -> Digest {
        let mut message = Vec::with_capacity(self.len() * size_of::<Digest>());

        for item in self.iter() {
            message.extend_from_slice(item.as_digest().as_ref());
        }

        message.as_digest()
    }
}

impl<T: AsDigest> AsDigest for Vec<T> {
    fn as_digest(&self) -> Digest {
        self.as_slice().as_digest()
    }
}

impl AsDigest for [u8] {
    fn as_digest(&self) -> Digest {
        Digest(sha3::Keccak256::digest(self).into())
    }
}

impl AsDigest for CodeCommitment {
    fn as_digest(&self) -> Digest {
        self.encode().as_digest()
    }
}

impl AsDigest for StateTransition {
    fn as_digest(&self) -> Digest {
        // State transition basic fields.

        let state_transition_size = // concat of fields:
            // actorId
            size_of::<Address>()
            // newStateHash
            + size_of::<H256>()
            // valueToReceive
            + size_of::<u128>()
            // valueClaimsBytes digest
            + size_of::<Digest>()
            // messagesHashesBytes digest
            + size_of::<H256>();

        let mut state_transition_bytes = Vec::with_capacity(state_transition_size);

        state_transition_bytes.extend_from_slice(self.actor_id.to_address_lossy().as_bytes());
        state_transition_bytes.extend_from_slice(self.new_state_hash.as_bytes());
        state_transition_bytes.extend_from_slice(self.value_to_receive.to_be_bytes().as_slice());

        // TODO (breathx): consider SeqHash for ValueClaim, so hashing of inner fields.
        // Value claims field.

        let value_claim_size = // concat of fields:
            // messageId
            size_of::<MessageId>()
            // destination
            + size_of::<Address>()
            // value
            + size_of::<u128>();

        let mut value_claims_bytes = Vec::with_capacity(self.value_claims.len() * value_claim_size);

        for ValueClaim {
            message_id,
            destination,
            value,
        } in &self.value_claims
        {
            value_claims_bytes.extend_from_slice(message_id.as_ref());
            value_claims_bytes.extend_from_slice(destination.to_address_lossy().as_bytes());
            // TODO (breathx): double check if we should use BIG endian.
            value_claims_bytes.extend_from_slice(value.to_be_bytes().as_slice())
        }

        let value_claims_digest = value_claims_bytes.as_digest();
        state_transition_bytes.extend_from_slice(value_claims_digest.as_ref());

        // Messages field.

        let messages_digest = self.messages.as_digest();
        state_transition_bytes.extend_from_slice(messages_digest.as_ref());

        state_transition_bytes.as_digest()
    }
}

impl AsDigest for OutgoingMessage {
    fn as_digest(&self) -> Digest {
        let message_size = // concat of fields:
            // id
            size_of::<MessageId>()
            // destination
            + size_of::<Address>()
            // payload
            + self.payload.len()
            // value
            + size_of::<u128>()
            // replyDetails.to
            + size_of::<MessageId>()
            // replyDetails.code
            + size_of::<[u8; 4]>();

        let mut message = Vec::with_capacity(message_size);

        message.extend_from_slice(self.id.as_ref());
        message.extend_from_slice(self.destination.to_address_lossy().as_bytes());
        message.extend_from_slice(&self.payload);
        // TODO (breathx): double check big endian.
        message.extend_from_slice(self.value.to_be_bytes().as_slice());

        let (reply_details_to, reply_details_code) =
            self.reply_details.unwrap_or_default().into_parts();

        message.extend_from_slice(reply_details_to.as_ref());
        message.extend_from_slice(reply_details_code.to_bytes().as_slice());

        message.as_digest()
    }
}

impl AsDigest for BlockCommitment {
    fn as_digest(&self) -> Digest {
        let block_commitment_size = // concat of fields:
            // blockHash
            size_of::<H256>()
            // prevCommitmentHash
            + size_of::<H256>()
            // predBlockHash
            + size_of::<H256>()
            // hash(transitionsHashesBytes)
            + size_of::<H256>();

        let mut block_commitment_bytes = Vec::with_capacity(block_commitment_size);

        block_commitment_bytes.extend_from_slice(self.block_hash.as_bytes());
        block_commitment_bytes.extend_from_slice(self.prev_commitment_hash.as_bytes());
        block_commitment_bytes.extend_from_slice(self.pred_block_hash.as_bytes());
        block_commitment_bytes.extend_from_slice(self.transitions.as_digest().as_ref());

        block_commitment_bytes.as_digest()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gprimitives::{ActorId, CodeId};
    use std::vec;

    #[test]
    fn as_digest() {
        let _digest = CodeCommitment {
            id: CodeId::from(0),
            valid: true,
        }
        .as_digest();

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
        let _digest = state_transition.as_digest();

        let transitions = vec![state_transition.clone(), state_transition];

        let block_commitment = BlockCommitment {
            block_hash: H256::from([0; 32]),
            pred_block_hash: H256::from([1; 32]),
            prev_commitment_hash: H256::from([2; 32]),
            transitions: transitions.clone(),
        };
        let _digest = block_commitment.as_digest();
    }
}
