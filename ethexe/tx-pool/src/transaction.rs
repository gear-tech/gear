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

use ethexe_signer::{Signature, ToDigest};
use gprimitives::H256;
use parity_scale_codec::{Decode, Encode};

/// Ethexe transaction behaviour.
pub trait Transaction {
    /// Error type for the trait operations.
    type Error;

    /// Validate transaction.
    fn validate(&self) -> Result<(), Self::Error>;

    /// Get transaction hash.
    fn tx_hash(&self) -> H256;
}

impl Transaction for () {
    type Error = anyhow::Error;

    fn validate(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn tx_hash(&self) -> H256 {
        H256::zero()
    }
}

/// Main ethexe transaction type.
#[derive(Debug, Clone, Encode, Decode, PartialEq, Eq)]
pub enum EthexeTransaction {
    /// Message send transaction
    /// **TEMPORARY**.
    Message {
        raw_message: Vec<u8>,
        signature: Vec<u8>,
    },
}

impl Transaction for EthexeTransaction {
    type Error = anyhow::Error;

    fn validate(&self) -> Result<(), Self::Error> {
        match self {
            EthexeTransaction::Message {
                raw_message,
                signature,
            } => {
                let message_digest = raw_message.to_digest();
                let signature = Signature::try_from(signature.as_ref())?;

                signature.verify_with_public_key_recover(message_digest)
            }
        }
    }

    fn tx_hash(&self) -> H256 {
        ethexe_db::hash(&self.encode())
    }
}
