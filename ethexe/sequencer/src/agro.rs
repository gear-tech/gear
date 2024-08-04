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

//! Abstract commitment aggregator.

use anyhow::Result;
use ethexe_common::{BlockCommitment, CodeCommitment, OutgoingMessage, StateTransition};
use ethexe_signer::{Address, PublicKey, Signature, Signer};
use gprimitives::{MessageId, H256};
use parity_scale_codec::{Decode, Encode};
use std::fmt;

pub trait SeqHash {
    fn hash(&self) -> H256;
}

#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq, Hash)]
pub struct AggregatedCommitments<D: SeqHash> {
    pub commitments: Vec<D>,
    pub signature: Signature,
}

impl SeqHash for H256 {
    fn hash(&self) -> H256 {
        *self
    }
}

// TODO: REMOVE THIS IMPL. SeqHash makes sense only for `ethexe_ethereum` types.
// identity hashing
impl SeqHash for CodeCommitment {
    fn hash(&self) -> H256 {
        ethexe_signer::hash(&self.encode())
    }
}

// TODO: REMOVE THIS IMPL. SeqHash makes sense only for `ethexe_ethereum` types.
impl SeqHash for StateTransition {
    fn hash(&self) -> H256 {
        let mut outgoing_bytes =
            Vec::with_capacity(self.outgoing_messages.len() * size_of::<H256>());

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

            outgoing_bytes.extend_from_slice(ethexe_signer::hash(&outgoing_message).as_bytes());
        }

        let mut message = Vec::with_capacity(size_of::<Address>() + (3 * size_of::<H256>()));

        message.extend_from_slice(&self.actor_id.into_bytes()[12..]);
        message.extend_from_slice(self.old_state_hash.as_bytes());
        message.extend_from_slice(self.new_state_hash.as_bytes());
        message.extend_from_slice(ethexe_signer::hash(&outgoing_bytes).as_bytes());

        ethexe_signer::hash(&message)
    }
}

impl SeqHash for BlockCommitment {
    fn hash(&self) -> H256 {
        let mut message = Vec::with_capacity(
            size_of::<H256>()
                + size_of::<H256>()
                + size_of::<H256>()
                + self.transitions.len() * size_of::<H256>(),
        );

        message.extend_from_slice(self.block_hash.as_bytes());
        message.extend_from_slice(self.allowed_pred_block_hash.as_bytes());
        message.extend_from_slice(self.allowed_prev_commitment_hash.as_bytes());
        message.extend_from_slice(self.transitions.hash().as_bytes());

        ethexe_signer::hash(&message)
    }
}

impl<T: SeqHash> SeqHash for &[T] {
    fn hash(&self) -> H256 {
        let buffer: Vec<u8> = self
            .iter()
            .map(SeqHash::hash)
            .flat_map(H256::to_fixed_bytes)
            .collect();
        ethexe_signer::hash(&buffer)
    }
}

impl<T: SeqHash> SeqHash for Vec<T> {
    fn hash(&self) -> H256 {
        self.as_slice().hash()
    }
}

impl<T: SeqHash> SeqHash for AggregatedCommitments<T> {
    fn hash(&self) -> H256 {
        self.commitments.hash()
    }
}

impl<T: SeqHash> AggregatedCommitments<T> {
    pub fn aggregate_commitments(
        commitments: Vec<T>,
        signer: &Signer,
        pub_key: PublicKey,
        router_address: Address,
    ) -> Result<AggregatedCommitments<T>> {
        let signature =
            Self::sign_commitments(commitments.hash(), signer, pub_key, router_address)?;

        Ok(AggregatedCommitments {
            commitments,
            signature,
        })
    }

    pub fn sign_commitments(
        commitments_hash: H256,
        signer: &Signer,
        pub_key: PublicKey,
        router_address: Address,
    ) -> Result<Signature> {
        let buffer = Self::buffer(commitments_hash, router_address);
        signer.sign_digest(pub_key, ethexe_signer::hash(&buffer).to_fixed_bytes())
    }

    pub fn recover_digest(
        digest: H256,
        signature: Signature,
        router_address: Address,
    ) -> Result<Address> {
        let buffer = Self::buffer(digest, router_address);
        let digest = ethexe_signer::hash(&buffer).to_fixed_bytes();
        signature.recover_digest(digest).map(|k| k.to_address())
    }

    pub fn len(&self) -> usize {
        self.commitments.len()
    }

    pub fn is_empty(&self) -> bool {
        self.commitments.is_empty()
    }

    pub fn verify_origin(&self, router_address: Address, origin: Address) -> Result<bool> {
        let buffer = Self::buffer(self.commitments.hash(), router_address);
        Ok(self
            .signature
            .recover_digest(ethexe_signer::hash(&buffer).to_fixed_bytes())?
            .to_address()
            == origin)
    }

    fn buffer(commitments_hash: H256, router_address: Address) -> Vec<u8> {
        [
            [0x19, 0x00].as_ref(),
            router_address.0.as_ref(),
            commitments_hash.as_ref(),
        ]
        .concat()
    }
}

#[derive(Clone)]
pub struct MultisignedCommitments<D> {
    pub commitments: Vec<D>,
    pub sources: Vec<Address>,
    pub signatures: Vec<Signature>,
}

impl<D: fmt::Debug> fmt::Debug for MultisignedCommitments<D> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "MultisignedCommitments {{ commitments: {:?}, sources: {:?}, signatures: {:?} }}",
            self.commitments, self.sources, self.signatures
        )
    }
}

// #[cfg(test)]
// mod tests {

//     use super::*;
//     use ethexe_signer::{Address, Signature};
//     use gear_core::ids::ActorId;

//     #[derive(Clone, Debug)]
//     pub struct MyComm([u8; 2]);

//     impl SeqHash for MyComm {
//         fn hash(&self) -> H256 {
//             ethexe_signer::hash(&self.0[..])
//         }
//     }

//     fn signer(id: u8) -> Address {
//         let mut array = [0; 20];
//         array[0] = id;
//         Address(array)
//     }

//     fn signature(id: u8) -> Signature {
//         let mut array = [0; 65];
//         array[0] = id;
//         Signature::from(array)
//     }

//     #[allow(unused)]
//     fn block_hash(id: u8) -> H256 {
//         let mut array = [0; 32];
//         array[0] = id;
//         H256::from(array)
//     }

//     #[allow(unused)]
//     fn pid(id: u8) -> ActorId {
//         let mut array = [0; 32];
//         array[0] = id;
//         ActorId::from(array)
//     }

//     #[allow(unused)]
//     fn state_id(id: u8) -> H256 {
//         let mut array = [0; 32];
//         array[0] = id;
//         H256::from(array)
//     }

//     fn gen_commitment(
//         signature_id: u8,
//         commitments: Vec<(u8, u8)>,
//     ) -> AggregatedCommitments<MyComm> {
//         let commitments = commitments
//             .into_iter()
//             .map(|v| MyComm([v.0, v.1]))
//             .collect();

//         AggregatedCommitments {
//             commitments,
//             signature: signature(signature_id),
//         }
//     }

//     #[test]
//     fn simple() {
//         // aggregator with threshold 1
//         let mut aggregator = Aggregator::new(1);

//         aggregator.push(signer(1), gen_commitment(0, vec![(1, 1)]));

//         let root = aggregator
//             .find_root()
//             .expect("Failed to generate root commitment");

//         assert_eq!(root.signatures.len(), 1);
//         assert_eq!(root.commitments.len(), 1);

//         // aggregator with threshold 1
//         let mut aggregator = Aggregator::new(1);

//         aggregator.push(signer(1), gen_commitment(0, vec![(1, 1)]));
//         aggregator.push(signer(1), gen_commitment(1, vec![(1, 1), (2, 2)]));

//         let root = aggregator
//             .find_root()
//             .expect("Failed to generate root commitment");

//         assert_eq!(root.signatures.len(), 1);

//         // should be latest commitment
//         assert_eq!(root.commitments.len(), 2);
//     }

//     #[test]
//     fn more_threshold() {
//         // aggregator with threshold 2
//         let mut aggregator = Aggregator::new(2);

//         aggregator.push(signer(1), gen_commitment(0, vec![(1, 1)]));
//         aggregator.push(signer(2), gen_commitment(0, vec![(1, 1)]));
//         aggregator.push(signer(2), gen_commitment(0, vec![(1, 1), (2, 2)]));

//         let root = aggregator
//             .find_root()
//             .expect("Failed to generate root commitment");

//         assert_eq!(root.signatures.len(), 2);
//         assert_eq!(root.commitments.len(), 1); // only (1, 1) is committed by both aggregators

//         // aggregator with threshold 2
//         let mut aggregator = Aggregator::new(2);

//         aggregator.push(signer(1), gen_commitment(0, vec![(1, 1)]));
//         aggregator.push(signer(2), gen_commitment(0, vec![(1, 1)]));
//         aggregator.push(signer(2), gen_commitment(0, vec![(1, 1), (2, 2)]));
//         aggregator.push(signer(1), gen_commitment(0, vec![(1, 1), (2, 2)]));

//         let root = aggregator
//             .find_root()
//             .expect("Failed to generate root commitment");

//         assert_eq!(root.signatures.len(), 2);
//         assert_eq!(root.commitments.len(), 2); // both (1, 1) and (2, 2) is committed by both aggregators
//     }
// }
