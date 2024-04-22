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

use crate::{merkle_tree::MerkleProof, Hasher};
use frame_support::traits::Get;
use gear_core::message::Payload;
use parity_scale_codec::{Decode, Encode};
use primitive_types::{H160, H256, U256};
use scale_info::TypeInfo;
use sp_runtime::traits::Hash;
use sp_std::vec::Vec;

#[derive(Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
#[cfg_attr(feature = "std", derive(serde::Deserialize, serde::Serialize))]
pub struct Proof {
    root: H256,
    proof: Vec<H256>,
    number_of_leaves: u64,
    leaf_index: u64,
    leaf: H256,
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

/// `OnEmpty` implementation for `Nonce` storage.
pub(crate) struct FirstNonce;

impl Get<U256> for FirstNonce {
    fn get() -> U256 {
        U256::one()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Decode, Encode)]
pub struct EthMessageData {
    destination: H160,
    payload: Payload,
}

impl EthMessageData {
    pub fn new(destination: H160, payload: Payload) -> Self {
        Self {
            destination,
            payload,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Decode, Encode, TypeInfo)]
pub struct EthMessage {
    nonce: U256,
    source: H256,
    destination: H160,
    payload: Vec<u8>,
}

impl EthMessage {
    pub(crate) fn from_data(nonce: U256, source: H256, data: EthMessageData) -> Self {
        let EthMessageData {
            destination,
            payload,
        } = data;
        let payload = payload.into_vec();

        Self {
            nonce,
            source,
            destination,
            payload,
        }
    }

    pub fn hash(&self) -> H256 {
        let mut nonce = [0; 32];

        self.nonce.to_little_endian(&mut nonce);

        let arg = [
            nonce.as_ref(),
            self.source.as_bytes(),
            self.destination.as_bytes(),
            self.payload.as_ref(),
        ]
        .concat();

        Hasher::hash(&arg)
    }
}
