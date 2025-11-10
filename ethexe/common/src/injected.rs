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

use crate::{Address, HashOf, ToDigest, ecdsa::SignedData};
use core::hash::Hash;
use gear_core::rpc::ReplyInfo;
use gprimitives::{ActorId, H256, MessageId};
use parity_scale_codec::{Decode, Encode};
use sha3::Keccak256;
use sp_core::Bytes;

/// Recent block hashes window size used to check transaction mortality.
pub const VALIDITY_WINDOW: u8 = 32;

pub type SignedInjectedTransaction = SignedData<InjectedTransaction>;

#[cfg_attr(feature = "std", derive(serde::Deserialize, serde::Serialize))]
#[cfg_attr(feature = "serde", derive(Hash))]
#[derive(Debug, Clone, Encode, Decode, PartialEq, Eq)]
pub struct InjectedTransaction {
    /// Address of validator the transaction intended for
    pub recipient: Address,
    /// Destination program inside `Vara.eth`.
    pub destination: ActorId,
    /// Payload of the message.
    pub payload: Bytes,
    /// Value attached to the message.
    /// NOTE: at this moment will be zero.
    pub value: u128,
    /// Reference block number.
    pub reference_block: H256,
    /// Arbitrary bytes to allow multiple synonymous
    /// transactions to be sent simultaneously.
    /// NOTE: this is also a salt for MessageId generation.
    pub salt: Bytes,
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

impl InjectedTransaction {
    /// Returns the hash of [`InjectedTransaction`].
    pub fn hash(&self) -> HashOf<InjectedTransaction> {
        // Safety because of implementation.
        unsafe { HashOf::new(gear_core::utils::hash(&self.encode()).into()) }
    }

    /// Creates [`MessageId`] from [`InjectedTransaction`].
    pub fn message_id(&self) -> MessageId {
        MessageId::new(self.hash().inner().0)
    }
}

/// [`InjectedPromise`] represents the guaranteed reply for [`InjectedTransaction`].
/// It contains the `payload` and the resulting `state_hash` after processing the transaction.
///
/// Note: Validator must ensure the validity of the promise, because of it can be slashed for
/// providing an invalid promise.
#[derive(Debug, Clone, Encode, Decode, PartialEq, Eq, Hash)]
pub struct InjectedPromise {
    /// Hash of the injected transaction this reply corresponds to.
    pub tx_hash: HashOf<InjectedTransaction>,
    /// Reply data for injected message.
    pub reply: ReplyInfo,
}

impl ToDigest for ReplyInfo {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        let Self {
            payload,
            code,
            value,
        } = self;

        payload.update_hasher(hasher);
        code.to_bytes().update_hasher(hasher);
        value.to_be_bytes().update_hasher(hasher);
    }
}

/// Signed wrapped on top of [`InjectedPromise`].
/// It will be shared among other validators as a proof of promise.
pub type SignedInjectedPromise = SignedData<InjectedPromise>;

impl ToDigest for InjectedPromise {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        let Self { tx_hash, reply } = self;

        tx_hash.inner().0.update_hasher(hasher);
        reply.update_hasher(hasher);
    }
}
