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
use ethexe_signer::Signature;
use gprimitives::{H160, H256};
use parity_scale_codec::{Decode, Encode};

/// Ethexe transaction behaviour.
pub trait Transaction:
    Clone + TxReferenceBlockHash + TxSignature + TxHashBlake2b256 + Encode
{
}

pub trait TxReferenceBlockHash {
    fn reference_block_hash(&self) -> H256;
}

pub trait TxSignature {
    fn signature(&self) -> Result<Signature>;
}

pub trait TxHashBlake2b256 {
    fn tx_hash(&self) -> H256;
}

/// Ethexe transaction with a signature.
#[derive(Debug, Clone, Encode, Decode, PartialEq, Eq)]
pub struct SignedEthexeTransaction {
    pub signature: Vec<u8>,
    pub transaction: EthexeTransaction,
}

impl SignedEthexeTransaction {
    pub fn new(signature: Vec<u8>, transaction: EthexeTransaction) -> Self {
        Self {
            signature,
            transaction,
        }
    }
}

/// Ethexe with a reference block for mortality.
#[derive(Debug, Clone, Encode, Decode, PartialEq, Eq)]
pub struct EthexeTransaction {
    pub raw: RawEthexeTransacton,
    pub reference_block: H256,
}

impl EthexeTransaction {
    pub fn new(raw: RawEthexeTransacton, reference_block: H256) -> Self {
        Self {
            raw,
            reference_block,
        }
    }
}

/// Raw ethexe transaction.
///
/// A particular job to be processed without external specifics.
#[derive(Debug, Clone, Encode, Decode, PartialEq, Eq)]
pub enum RawEthexeTransacton {
    SendMessage {
        source: H160,
        program_id: H160,
        payload: Vec<u8>,
        value: u128,
    },
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
