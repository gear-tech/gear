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

use crate::{Address, ToDigest, ecdsa::SignedData};
use alloc::vec::Vec;
use gprimitives::{ActorId, H256};
use parity_scale_codec::{Decode, Encode};
use sha3::Keccak256;

pub type SignedInjectedTransaction = SignedData<InjectedTransaction>;

#[cfg_attr(feature = "std", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, Encode, Decode, PartialEq, Eq)]
pub struct InjectedTransaction {
    /// Address of validator the transaction intended for
    pub recipient: Address,
    /// Destination program inside `Gear.exe`.
    pub destination: ActorId,
    /// Payload of the message.
    pub payload: Vec<u8>,
    /// Value attached to the message.
    ///
    /// NOTE: at this moment will be zero.
    pub value: u128,
    /// Reference block number.
    pub reference_block: H256,
    /// Arbitrary bytes to allow multiple synonymous
    /// transactions to be sent simultaneously.
    ///
    /// NOTE: this is also a salt for MessageId generation.
    pub salt: Vec<u8>,
}

impl ToDigest for InjectedTransaction {
    fn update_hasher(&self, hasher: &mut Keccak256) {
        let Self {
            recipient,
            destination,
            payload,
            value,
            reference_block,
            salt,
        } = self;

        recipient.0.update_hasher(hasher);
        destination.into_bytes().update_hasher(hasher);
        payload.update_hasher(hasher);
        value.to_be_bytes().update_hasher(hasher);
        reference_block.0.update_hasher(hasher);
        salt.update_hasher(hasher);
    }
}
