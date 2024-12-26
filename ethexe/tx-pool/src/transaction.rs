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

//! Tx pool transaction related types.

use anyhow::Result;
use ethexe_signer::{Address, Signature, ToDigest};
use gprimitives::{H160, H256};
use parity_scale_codec::{Decode, Encode};
use std::fmt;

/// Ethexe transaction behaviour.
pub trait Transaction:
    Clone + TxReferenceBlockHash + TxSignature + TxHashBlake2b256 + Encode
{
}

/// Ethexe transaction reference block hash
///
/// Reference block hash is used for a transcation mortality check.
pub trait TxReferenceBlockHash {
    fn reference_block_hash(&self) -> H256;
}

/// Ethexe transaction signature.
pub trait TxSignature {
    fn signature(&self) -> Result<Signature>;
}

/// Ethexe transaction blake2b256 hash.
pub trait TxHashBlake2b256 {
    fn tx_hash(&self) -> H256;
}

/// Ethexe transaction with a signature.
#[derive(Clone, Encode, Decode, PartialEq, Eq)]
pub struct SignedEthexeTransaction {
    pub signature: Vec<u8>,
    pub transaction: EthexeTransaction,
}

impl SignedEthexeTransaction {
    /// Gets source of the `SendMessage` transaction recovering it from the signature.
    pub fn send_message_source(&self) -> Result<H160> {
        Signature::try_from(self.signature.as_ref())
            .and_then(|signature| {
                signature.recover_from_digest(self.transaction.encode().to_digest())
            })
            .map(|public_key| H160::from(Address::from(public_key).0))
            .map_err(Into::into)
    }
}

impl fmt::Debug for SignedEthexeTransaction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SignedEthexeTransaction")
            .field("signature", &hex::encode(&self.signature))
            .field("transaction", &self.transaction)
            .finish()
    }
}

impl fmt::Display for SignedEthexeTransaction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "SignedEthexeTransaction {{ signature: 0x{}, transaction: {} }}",
            hex::encode(&self.signature),
            self.transaction
        )
    }
}

/// Ethexe with a reference block for mortality.
#[derive(Debug, Clone, Encode, Decode, PartialEq, Eq)]
pub struct EthexeTransaction {
    pub raw: RawEthexeTransacton,
    pub reference_block: H256,
}

impl fmt::Display for EthexeTransaction {
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
pub enum RawEthexeTransacton {
    SendMessage {
        program_id: H160,
        payload: Vec<u8>,
        value: u128,
    },
}

impl RawEthexeTransacton {
    /// Gets the program id of the transaction.
    pub fn program_id(&self) -> H160 {
        match self {
            RawEthexeTransacton::SendMessage { program_id, .. } => *program_id,
        }
    }

    /// Gets the payload of the transaction.
    pub fn payload(&self) -> &[u8] {
        match self {
            RawEthexeTransacton::SendMessage { payload, .. } => payload,
        }
    }

    /// Gets the value of the transaction.
    pub fn value(&self) -> u128 {
        match self {
            RawEthexeTransacton::SendMessage { value, .. } => *value,
        }
    }
}

impl fmt::Display for RawEthexeTransacton {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RawEthexeTransacton::SendMessage {
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

impl Transaction for SignedEthexeTransaction {}

impl TxHashBlake2b256 for SignedEthexeTransaction {
    fn tx_hash(&self) -> H256 {
        ethexe_db::hash(&self.encode())
    }
}

impl TxSignature for SignedEthexeTransaction {
    fn signature(&self) -> Result<Signature> {
        Signature::try_from(self.signature.as_ref())
    }
}

impl TxReferenceBlockHash for SignedEthexeTransaction {
    fn reference_block_hash(&self) -> H256 {
        self.transaction.reference_block
    }
}

impl TxHashBlake2b256 for EthexeTransaction {
    fn tx_hash(&self) -> H256 {
        ethexe_db::hash(&self.encode())
    }
}

impl TxReferenceBlockHash for EthexeTransaction {
    fn reference_block_hash(&self) -> H256 {
        self.reference_block
    }
}

impl TxHashBlake2b256 for () {
    fn tx_hash(&self) -> H256 {
        H256::zero()
    }
}

impl TxSignature for () {
    fn signature(&self) -> Result<Signature> {
        Signature::try_from(vec![0u8; 65].as_ref())
    }
}

impl TxReferenceBlockHash for () {
    fn reference_block_hash(&self) -> H256 {
        H256::random()
    }
}
