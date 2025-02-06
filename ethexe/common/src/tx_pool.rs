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
pub struct SignedTransaction {
    pub signature: Vec<u8>,
    pub transaction: Transaction,
}

impl SignedTransaction {
    /// Ethexe transaction blake2b256 hash.
    pub fn tx_hash(&self) -> H256 {
        gear_core::ids::hash(&self.encode()).into()
    }

    /// Ethexe transaction reference block hash
    ///
    /// Reference block hash is used for a transcation mortality check.
    pub fn reference_block_hash(&self) -> H256 {
        self.transaction.reference_block
    }
}

impl fmt::Debug for SignedTransaction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SignedEthexeTransaction")
            .field("signature", &hex::encode(&self.signature))
            .field("transaction", &self.transaction)
            .finish()
    }
}

impl fmt::Display for SignedTransaction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "SignedEthexeTransaction {{ signature: 0x{}, transaction: {} }}",
            hex::encode(&self.signature),
            self.transaction
        )
    }
}

/// Ethexe transaction with a reference block for mortality.
#[derive(Debug, Clone, Encode, Decode, PartialEq, Eq)]
pub struct Transaction {
    pub raw: RawTransacton,
    pub reference_block: H256,
}

impl fmt::Display for Transaction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "EthexeTransaction {{ raw: {}, reference_block: {} }}",
            self.raw, self.reference_block
        )
    }
}

/// Raw ethexe transaction.
///
/// A particular job to be processed without external specifics.
#[derive(Debug, Clone, Encode, Decode, PartialEq, Eq)]
pub enum RawTransacton {
    SendMessage {
        program_id: H160,
        payload: Vec<u8>,
        value: u128,
    },
}

impl RawTransacton {
    /// Gets the program id of the transaction.
    pub fn program_id(&self) -> H160 {
        match self {
            RawTransacton::SendMessage { program_id, .. } => *program_id,
        }
    }

    /// Gets the payload of the transaction.
    pub fn payload(&self) -> &[u8] {
        match self {
            RawTransacton::SendMessage { payload, .. } => payload,
        }
    }

    /// Gets the value of the transaction.
    pub fn value(&self) -> u128 {
        match self {
            RawTransacton::SendMessage { value, .. } => *value,
        }
    }
}

impl fmt::Display for RawTransacton {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RawTransacton::SendMessage {
                program_id,
                payload,
                value,
            } => f
                .debug_struct("SendMessage")
                .field("program_id", program_id)
                .field("payload", &hex::encode(payload))
                .field("value", value)
                .finish(),
        }
    }
}
