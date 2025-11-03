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

//! ethexe tx pool types

use crate::{ToDigest, ecdsa::SignedData};
use alloc::vec::Vec;
use derive_more::{Debug, Display};
use gprimitives::{H160, H256};
use parity_scale_codec::{Decode, Encode};
use sha3::Digest as _;

pub type SignedOffchainTransaction = SignedData<OffchainTransaction>;

impl SignedOffchainTransaction {
    /// Ethexe transaction blake2b256 hash.
    pub fn tx_hash(&self) -> H256 {
        gear_core::utils::hash(&self.encode()).into()
    }

    /// Ethexe transaction reference block hash
    ///
    /// Reference block hash is used for a transaction mortality check.
    pub fn reference_block(&self) -> H256 {
        self.data().reference_block
    }
}

/// Ethexe offchain transaction with a reference block for mortality.
#[derive(Clone, Encode, Decode, PartialEq, Eq, Debug, Display)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
#[display("OffchainTransaction {{ raw: {raw}, reference_block: {reference_block} }}")]
pub struct OffchainTransaction {
    pub raw: RawOffchainTransaction,
    pub reference_block: H256,
}

impl ToDigest for OffchainTransaction {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        hasher.update(self.encode());
    }
}

/// Raw ethexe offchain transaction.
///
/// A particular job to be processed without external specifics.
#[derive(Clone, Encode, Decode, PartialEq, Eq, Debug, Display)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub enum RawOffchainTransaction {
    #[display(
        "SendMessage {{ program_id: {program_id}, payload: {} }}",
        hex::encode(payload)
    )]
    SendMessage { program_id: H160, payload: Vec<u8> },
}

impl RawOffchainTransaction {
    /// Gets the program id of the transaction.
    pub fn program_id(&self) -> H160 {
        match self {
            RawOffchainTransaction::SendMessage { program_id, .. } => *program_id,
        }
    }

    /// Gets the payload of the transaction.
    pub fn payload(&self) -> &[u8] {
        match self {
            RawOffchainTransaction::SendMessage { payload, .. } => payload,
        }
    }
}
