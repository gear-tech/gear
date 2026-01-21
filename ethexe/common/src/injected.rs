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

use crate::{Address, Announce, HashOf, ToDigest, ecdsa::SignedMessage};
use alloc::vec::Vec;
use core::hash::Hash;
use gear_core::rpc::ReplyInfo;
use gprimitives::{ActorId, H256, MessageId};
use parity_scale_codec::{Decode, Encode};
use sha3::{Digest, Keccak256};
use sp_core::Bytes;

/// Recent block hashes window size used to check transaction mortality.
pub const VALIDITY_WINDOW: u8 = 32;

pub type SignedInjectedTransaction = SignedMessage<InjectedTransaction>;

#[cfg_attr(feature = "std", derive(serde::Deserialize, serde::Serialize))]
#[cfg_attr(feature = "serde", derive(Hash))]
#[derive(Debug, Clone, Encode, Decode, PartialEq, Eq)]
pub struct RpcOrNetworkInjectedTx {
    /// Address of validator the transaction intended for
    pub recipient: Address,
    pub tx: SignedInjectedTransaction,
}

/// IMPORTANT: message id == tx hash == blake2b256 hash of the struct fields concat.
#[cfg_attr(feature = "std", derive(serde::Deserialize, serde::Serialize))]
#[cfg_attr(feature = "serde", derive(Hash))]
#[derive(Debug, Clone, Encode, Decode, PartialEq, Eq)]
pub struct InjectedTransaction {
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
            destination,
            payload,
            value,
            reference_block,
            salt,
        } = self;

        destination.into_bytes().update_hasher(hasher);
        payload.update_hasher(hasher);
        value.to_be_bytes().update_hasher(hasher);
        reference_block.0.update_hasher(hasher);
        salt.update_hasher(hasher);
    }
}

impl InjectedTransaction {
    /// Returns the hash of [`InjectedTransaction`].
    pub fn to_hash(&self) -> HashOf<InjectedTransaction> {
        // Safe because we hash corresponding type itself
        let bytes = [
            self.destination.as_ref(),
            self.payload.as_ref(),
            &self.value.to_be_bytes(),
            &self.reference_block.0,
            self.salt.as_ref(),
        ]
        .concat();
        unsafe { HashOf::new(gear_core::utils::hash(&bytes).into()) }
    }

    /// Creates [`MessageId`] from [`InjectedTransaction`].
    pub fn to_message_id(&self) -> MessageId {
        MessageId::new(self.to_hash().inner().0)
    }
}

/// [`Promise`] represents the guaranteed reply for [`InjectedTransaction`].
///
/// Note: Validator must ensure the validity of the promise, because of it can be slashed for
/// providing an invalid promise.
#[cfg_attr(feature = "std", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, Encode, Decode, PartialEq, Eq, Hash)]
pub struct Promise {
    /// Hash of the injected transaction this reply corresponds to.
    pub tx_hash: HashOf<InjectedTransaction>,
    /// Reply data for injected message.
    pub reply: ReplyInfo,
}

/// Signed wrapper on top of [`Promise`].
/// It will be shared among other validators as a proof of promise.
pub type SignedPromise = SignedMessage<Promise>;

impl ToDigest for Promise {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        let Self { tx_hash, reply } = self;

        hasher.update(tx_hash.inner());
        reply.update_hasher(hasher);
    }
}

/// The maximum size of gossip message in bytes.
/// Currently set to 1 MB.
pub const MAX_GOSSIP_MESSAGE_SIZE: usize = 1024 * 1024;

/// The bundle of promises which will be distributed in network in a single gossip message.  #[cfg_attr(feature = "std", derive(serde::Deserialize, serde::Serialize))]
///
/// The purpose of this bundle is to fit promises into one gossip message, if their total size exceeds.
#[derive(Debug, Clone, Encode, Decode, PartialEq, Eq, Default)]
pub struct PromisesBundle {
    /// The announce hash corresponding to the bundle.
    pub announce_hash: HashOf<Announce>,
    /// The sequence number of the bundle in the announce promises.
    pub seqnum: u32,
    /// Indicates whether this bundle is the last one for the announce.
    pub is_last: bool,
    /// The promises of the bundle.
    pub promises: Vec<SignedPromise>,
}

impl PromisesBundle {
    /// SCALE encoded size offset of the bundle without the promises.
    /// This is needed, because of the size of the encoded data is not constant and depends of the data.
    ///
    /// `100` bytes is approximately three times exceeding the actual size of the bundle without promises.
    pub fn encoding_offset() -> usize {
        100
    }
}

/// This function splits the given promises into multiple [`PromisesBundle`]s, where
/// each bundle does not exceed the maximum gossip message size.
///  
/// It is needed to prepare the promises for sending via gossip messages.
pub fn split_promises_into_bundles(
    promises: Vec<SignedPromise>,
    announce_hash: HashOf<Announce>,
) -> Vec<PromisesBundle> {
    let mut bundles = Vec::new();
    let mut seqnum = 0;

    // Set the initial size counter with the bundle size except promises.
    let mut size_counter = PromisesBundle::encoding_offset();
    let mut current_bundle_promises = Vec::new();

    for promise in promises {
        let promise_size = promise.encoded_size();
        if size_counter + promise_size > MAX_GOSSIP_MESSAGE_SIZE {
            bundles.push(PromisesBundle {
                promises: current_bundle_promises.drain(..).collect(),
                announce_hash,
                seqnum,
                is_last: false,
            });
            seqnum += 1;
            size_counter = PromisesBundle::encoding_offset();
        } else {
            current_bundle_promises.push(promise);
            size_counter += promise_size;
        }
    }

    // Handle the remaining promises in the last bundle.
    if !current_bundle_promises.is_empty() {
        bundles.push(PromisesBundle {
            promises: current_bundle_promises,
            announce_hash,
            seqnum,
            is_last: true,
        });
    } else {
        if let Some(bundle) = bundles.last_mut() {
            bundle.is_last = true
        }
    }

    bundles
}
