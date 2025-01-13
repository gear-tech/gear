// This file is part of Gear.

// Copyright (C) 2024 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::{Config, Error, MessageNonce};
use binary_merkle_tree::MerkleProof;
use frame_support::{ensure, traits::Get};
use gprimitives::{ActorId, H160, H256, U256};
use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_runtime::traits::{Hash, Keccak256};
use sp_std::vec::Vec;

/// Type representing merkle proof of message's inclusion into bridging queue.
#[derive(Clone, Debug, Default, Encode, Decode, PartialEq, Eq, PartialOrd, Ord, TypeInfo)]
#[cfg_attr(feature = "std", derive(serde::Deserialize, serde::Serialize))]
pub struct Proof {
    /// Merkle root of the tree this proof associated with.
    pub root: H256,
    /// Proof itself: collection of hashes required for verification.
    pub proof: Vec<H256>,
    /// Number of leaves in the tree.
    pub number_of_leaves: u64,
    /// Leaf index we're proving inclusion.
    pub leaf_index: u64,
    /// Leaf value for inclusion proving.
    pub leaf: H256,
}

impl From<MerkleProof<H256, H256>> for Proof {
    fn from(value: MerkleProof<H256, H256>) -> Self {
        Self {
            root: value.root,
            proof: value.proof,
            number_of_leaves: value.number_of_leaves as u64,
            leaf_index: value.leaf_index as u64,
            leaf: value.leaf,
        }
    }
}

/// Type representing message being bridged from gear to eth.
#[derive(Clone, Debug, Default, Encode, Decode, PartialEq, Eq, PartialOrd, Ord, TypeInfo)]
pub struct EthMessage {
    nonce: U256,
    source: H256,
    destination: H160,
    payload: Vec<u8>,
}

impl EthMessage {
    /// Creates a new [`EthMessage`].
    #[cfg(test)]
    pub fn new(nonce: U256, source: ActorId, destination: H160, payload: Vec<u8>) -> Self {
        Self {
            nonce,
            source: source.into_bytes().into(),
            destination,
            payload,
        }
    }

    pub(crate) fn try_new<T: Config>(
        source: ActorId,
        destination: H160,
        payload: Vec<u8>,
    ) -> Result<Self, Error<T>> {
        ensure!(
            payload.len() <= T::MaxPayloadSize::get() as usize,
            Error::<T>::MaxPayloadSizeExceeded
        );

        let nonce = MessageNonce::<T>::mutate(|nonce| {
            let res = *nonce;
            *nonce = nonce.saturating_add(U256::one());
            res
        });

        Ok(Self {
            nonce,
            source: source.into_bytes().into(),
            destination,
            payload,
        })
    }

    /// Message's nonce getter.
    pub fn nonce(&self) -> U256 {
        self.nonce
    }

    /// Message's source getter.
    pub fn source(&self) -> H256 {
        self.source
    }

    /// Message's destination getter.
    pub fn destination(&self) -> H160 {
        self.destination
    }

    /// Message's payload bytes getter.
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    /// Returns hash of the message using `Keccak256` hasher.
    ///
    /// Has `pub(crate)` visibility due to dependency on substrate
    /// runtime interface (keccak hashing).
    pub(crate) fn hash(&self) -> H256 {
        let mut nonce = [0; 32];
        self.nonce.to_little_endian(&mut nonce);

        let bytes = [
            nonce.as_ref(),
            self.source.as_bytes(),
            self.destination.as_bytes(),
            self.payload.as_ref(),
        ]
        .concat();

        Keccak256::hash(&bytes)
    }
}
