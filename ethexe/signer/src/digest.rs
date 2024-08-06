use core::fmt;
use ethexe_common::{BlockCommitment, CodeCommitment, OutgoingMessage, StateTransition};
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
        let mut outgoing_bytes =
            Vec::with_capacity(self.outgoing_messages.len() * size_of::<Digest>());

        for OutgoingMessage {
            message_id,
            destination,
            payload,
            value,
            reply_details,
        } in self.outgoing_messages.iter()
        {
            let reply_details = reply_details.unwrap_or_default();
            let mut outgoing_message = Vec::with_capacity(
                size_of::<MessageId>()
                    + size_of::<Address>()
                    + payload.inner().len()
                    + size_of::<u128>()
                    + size_of::<MessageId>()
                    + size_of::<[u8; 4]>(),
            );

            outgoing_message.extend_from_slice(&message_id.into_bytes());
            outgoing_message.extend_from_slice(&destination.into_bytes()[12..]);
            outgoing_message.extend_from_slice(payload.inner());
            outgoing_message.extend_from_slice(&value.to_be_bytes());
            outgoing_message.extend_from_slice(&reply_details.to_message_id().into_bytes());
            outgoing_message.extend(&reply_details.to_reply_code().to_bytes());

            outgoing_bytes.extend_from_slice(outgoing_message.as_digest().as_ref());
        }

        let mut message =
            Vec::with_capacity(size_of::<Address>() + 2 * size_of::<H256>() + size_of::<Digest>());

        message.extend_from_slice(&self.actor_id.into_bytes()[12..]);
        message.extend_from_slice(self.old_state_hash.as_bytes());
        message.extend_from_slice(self.new_state_hash.as_bytes());
        message.extend_from_slice(outgoing_bytes.as_digest().as_ref());

        message.as_digest()
    }
}

impl AsDigest for BlockCommitment {
    fn as_digest(&self) -> Digest {
        let mut message = Vec::with_capacity(
            size_of::<H256>()
                + size_of::<H256>()
                + size_of::<H256>()
                + self.transitions.len() * size_of::<Digest>(),
        );

        message.extend_from_slice(self.block_hash.as_bytes());
        message.extend_from_slice(self.allowed_pred_block_hash.as_bytes());
        message.extend_from_slice(self.allowed_prev_commitment_hash.as_bytes());
        message.extend_from_slice(self.transitions.as_digest().as_ref());

        message.as_digest()
    }
}

#[cfg(test)]
mod tests {
    use std::vec;

    use ethexe_common::gear_core::message::Payload;
    use gprimitives::{ActorId, CodeId};

    use super::*;

    #[test]
    fn as_digest() {
        let _digest = CodeCommitment {
            code_id: CodeId::from(0),
            approved: true,
        }
        .as_digest();

        let state_transition = StateTransition {
            actor_id: ActorId::from(0),
            old_state_hash: H256::from([0; 32]),
            new_state_hash: H256::from([1; 32]),
            outgoing_messages: vec![OutgoingMessage {
                message_id: MessageId::from(0),
                destination: ActorId::from(0),
                payload: Payload::try_from(b"Hello, World!".to_vec()).unwrap(),
                value: 0,
                reply_details: None,
            }],
        };
        let _digest = state_transition.as_digest();

        let transitions = vec![state_transition.clone(), state_transition];

        let block_commitment = BlockCommitment {
            block_hash: H256::from([0; 32]),
            allowed_pred_block_hash: H256::from([1; 32]),
            allowed_prev_commitment_hash: H256::from([2; 32]),
            transitions: transitions.clone(),
        };
        let _digest = block_commitment.as_digest();
    }
}
