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

use alloc::vec::Vec;
use core::fmt;
use gprimitives::{H160, H256};
use parity_scale_codec::{Decode, Encode};

/// Ethexe transaction with a signature.
#[derive(Clone, Encode, Decode, PartialEq, Eq)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct SignedOffchainTransaction {
    pub signature: Vec<u8>,
    pub transaction: OffchainTransaction,
}

impl SignedOffchainTransaction {
    /// Ethexe transaction blake2b256 hash.
    pub fn tx_hash(&self) -> H256 {
        gear_core::utils::hash(&self.encode()).into()
    }

    /// Ethexe transaction reference block hash
    ///
    /// Reference block hash is used for a transaction mortality check.
    pub fn reference_block(&self) -> H256 {
        self.transaction.reference_block
    }
}

impl fmt::Debug for SignedOffchainTransaction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SignedOffchainTransaction")
            .field("signature", &hex::encode(&self.signature))
            .field("transaction", &self.transaction)
            .finish()
    }
}

impl fmt::Display for SignedOffchainTransaction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "SignedOffchainTransaction {{ signature: 0x{}, transaction: {} }}",
            hex::encode(&self.signature),
            self.transaction
        )
    }
}

/// Ethexe offchain transaction with a reference block for mortality.
#[derive(Debug, Clone, Encode, Decode, PartialEq, Eq)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct OffchainTransaction {
    pub raw: RawOffchainTransaction,
    pub reference_block: H256,
}

impl OffchainTransaction {
    /// Recent block hashes window size used to check transaction mortality.
    ///
    /// ### Rationale
    /// The constant could have been defined in the `ethexe-db`,
    /// but defined here to ease upgrades without invalidation of the transactions
    /// stores.
    pub const BLOCK_HASHES_WINDOW_SIZE: u32 = 32;
}

impl fmt::Display for OffchainTransaction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "OffchainTransaction {{ raw: {}, reference_block: {} }}",
            self.raw, self.reference_block
        )
    }
}

/// Raw ethexe offchain transaction.
///
/// A particular job to be processed without external specifics.
#[derive(Debug, Clone, Encode, Decode, PartialEq, Eq)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub enum RawOffchainTransaction {
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

impl fmt::Display for RawOffchainTransaction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RawOffchainTransaction::SendMessage {
                program_id,
                payload,
            } => f
                .debug_struct("SendMessage")
                .field("program_id", program_id)
                .field("payload", &hex::encode(payload))
                .finish(),
        }
    }
}
