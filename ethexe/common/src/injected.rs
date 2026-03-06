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

use crate::{Address, HashOf, ToDigest, ecdsa::SignedMessage};
use alloc::string::{String, ToString};
use core::hash::Hash;
use gear_core::{limited::LimitedVec, rpc::ReplyInfo};
use gprimitives::{ActorId, H256, MessageId};
use gsigner::{PrivateKey, secp256k1::signature::SignResult};
use parity_scale_codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sha3::{Digest, Keccak256};

/// Recent block hashes window size used to check transaction mortality.
pub const VALIDITY_WINDOW: u8 = 32;

/// Maximum size of single injected transaction payload.
///
/// Limited by the maximum injected transactions size per announce.
/// Currently is 126 KiB.
pub const MAX_INJECTED_TX_PAYLOAD_SIZE: usize = 126 * 1024;

/// Maximum size of injected transaction salt.
pub const MAX_INJECTED_TX_SALT_SIZE: usize = 32;

#[cfg_attr(feature = "std", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, Encode, Decode, Eq, PartialEq)]
pub enum InjectedTransactionAcceptance {
    Accept,
    Reject { reason: String },
}

impl<E: ToString> From<Result<(), E>> for InjectedTransactionAcceptance {
    fn from(value: Result<(), E>) -> Self {
        match value {
            Ok(()) => Self::Accept,
            Err(err) => Self::Reject {
                reason: err.to_string(),
            },
        }
    }
}

pub type SignedInjectedTransaction = SignedMessage<InjectedTransaction>;

#[cfg_attr(feature = "std", derive(serde::Deserialize, serde::Serialize))]
#[cfg_attr(feature = "serde", derive(Hash))]
#[derive(Debug, Clone, Encode, Decode, PartialEq, Eq)]
pub struct AddressedInjectedTransaction {
    /// Address of validator the transaction intended for
    pub recipient: Address,
    pub tx: SignedInjectedTransaction,
}

/// IMPORTANT: message id == tx hash == blake2b256 hash of the struct fields concat.
#[cfg_attr(feature = "std", derive(serde::Deserialize, serde::Serialize))]
#[cfg_attr(feature = "serde", derive(Hash))]
#[derive(Debug, Clone, Encode, Decode, MaxEncodedLen, TypeInfo, PartialEq, Eq)]
pub struct InjectedTransaction {
    /// Destination program inside `Vara.eth`.
    pub destination: ActorId,
    /// Payload of the message.
    #[cfg_attr(feature = "std", serde(with = "serde_hex"))]
    pub payload: LimitedVec<u8, MAX_INJECTED_TX_PAYLOAD_SIZE>,
    /// Value attached to the message.
    /// NOTE: at this moment will be zero.
    pub value: u128,
    /// Reference block number.
    pub reference_block: H256,
    /// Arbitrary bytes to allow multiple synonymous
    /// transactions to be sent simultaneously.
    /// NOTE: this is also a salt for MessageId generation.
    #[cfg_attr(feature = "std", serde(with = "serde_hex"))]
    pub salt: LimitedVec<u8, MAX_INJECTED_TX_SALT_SIZE>,
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

impl Promise {
    /// Calculates the `blake2b` hash from promise's reply.
    pub fn reply_hash(&self) -> HashOf<ReplyInfo> {
        // Safety by implementation
        unsafe { HashOf::new(self.reply.to_hash()) }
    }

    /// Converts promise to its [`PromiseHashes`].
    pub fn to_hashes(&self) -> PromiseHashes {
        PromiseHashes {
            tx_hash: self.tx_hash,
            reply_hash: self.reply_hash(),
        }
    }
}

impl ToDigest for Promise {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        self.to_hashes().update_hasher(hasher);
    }
}

/// A wrapper on top of [`PromiseHashes`].
///
/// [`CompactSignedPromise`] is a lightweight version of [`SignedPromise`], that is
/// needed to reduce the amount of data transferred in network between validators.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, derive_more::Deref, derive_more::From)]
pub struct CompactSignedPromise(SignedMessage<PromiseHashes>);

/// The hashes of [`Promise`] parts.
#[derive(Debug, Clone, Encode, Decode, PartialEq, Eq)]
pub struct PromiseHashes {
    pub tx_hash: HashOf<InjectedTransaction>,
    pub reply_hash: HashOf<ReplyInfo>,
}

impl ToDigest for PromiseHashes {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        let Self {
            tx_hash,
            reply_hash,
        } = self;

        hasher.update(tx_hash.inner());
        hasher.update(reply_hash.inner());
    }
}

impl CompactSignedPromise {
    /// Create the [`CompactSignedPromise`] from private key and hashes.
    pub fn create(private_key: PrivateKey, promise_hashes: PromiseHashes) -> SignResult<Self> {
        SignedMessage::create(private_key, promise_hashes).map(CompactSignedPromise)
    }

    pub fn create_from_promise(private_key: PrivateKey, promise: &Promise) -> SignResult<Self> {
        Self::create(private_key, promise.to_hashes())
    }

    /// Create the [`CompactSignedPromise`] from a valid [`SignedPromise`].
    ///
    /// # Panics
    /// Panics if the digest of [`Promise`] and [`PromiseHashes`] ever diverge.
    /// This must hold by construction; tests enforce the invariant.
    pub fn from_signed_promise_unchecked(signed_promise: &SignedPromise) -> Self {
        Self::try_from(signed_promise)
            .expect("SignedPromise and PromiseHashes must have identical digest")
    }
}

impl TryFrom<&SignedPromise> for CompactSignedPromise {
    type Error = &'static str;

    fn try_from(signed_promise: &SignedPromise) -> Result<Self, Self::Error> {
        SignedMessage::try_from_parts(
            signed_promise.data().to_hashes(),
            *signed_promise.signature(),
            signed_promise.address(),
        )
        .map(CompactSignedPromise)
    }
}

/// Encoding and decoding of `LimitedVec<u8, N>` as hex string.
#[cfg(feature = "std")]
mod serde_hex {
    pub fn serialize<S, const N: usize>(
        data: &super::LimitedVec<u8, N>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        alloy_primitives::hex::serialize(data.to_vec(), serializer)
    }

    pub fn deserialize<'de, D, const N: usize>(
        deserializer: D,
    ) -> Result<super::LimitedVec<u8, N>, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let vec: Vec<u8> = alloy_primitives::hex::deserialize(deserializer)?;
        super::LimitedVec::<u8, N>::try_from(vec)
            .map_err(|_| serde::de::Error::custom("LimitedVec deserialization overflow"))
    }
}

#[cfg(all(test, feature = "mock"))]
mod tests {
    use gsigner::PrivateKey;

    use super::*;
    use crate::mock::Mock;

    #[test]
    fn signed_message_and_injected_transactions() {
        const RPC_INPUT: &str = r#"{
            "data": {
                "destination": "0xede8c947f1ce1a5add6c26c2db01ad1dcd377c72",
                "payload": "0x",
                "value": 0,
                "reference_block": "0xb03574ea84ef2acbdbc8c04f8afb73c9d59f2fbd3bf82f37dcb2aa390372b702",
                "salt": "0x6c6db263a31830e072ea7f083e6a818df3074119be6eee60601a5f2f668db508"
            },
            "signature": "0xfeffc4dfc0d5d49bd036b12a7ff5163132b5a40c93a5d369d0af1f925851ad1412fb33b7632c4dac9c8828d194fcaf417d5a2a2583ba23195c0080e8b6890c0a1c",
            "address": "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266"
        }"#;

        let signed_tx: SignedInjectedTransaction =
            serde_json::from_str(RPC_INPUT).expect("failed to deserialize SignedMessage");

        // AKA tx_hash
        assert_eq!(
            hex::encode(signed_tx.data().to_message_id()),
            "867184f57aa63ceeb4066c061098317388bbacbea309ebd09a7fd228469460ee"
        );

        assert_eq!(
            hex::encode(signed_tx.address().0),
            "f39fd6e51aad88f6f4ce6ab8827279cfffb92266"
        );

        assert_eq!(
            signed_tx
                .signature()
                .recover_message(signed_tx.data())
                .expect("failed to recover message")
                .to_address(),
            signed_tx.address()
        );
    }

    #[test]
    fn promise_hashes_digest_equal_to_promise_digest() {
        let promise = Promise::mock(());

        assert_eq!(promise.to_digest(), promise.to_hashes().to_digest());
    }

    #[test]
    fn signatures_equal_for_promise_and_compact_promise() {
        let private_key = PrivateKey::random();
        let promise = Promise::mock(());

        let signed_promise = SignedPromise::create(private_key.clone(), promise.clone()).unwrap();
        let compact_signed_promise =
            CompactSignedPromise::create_from_promise(private_key, &promise).unwrap();

        assert_eq!(signed_promise.address(), compact_signed_promise.address());
        assert_eq!(
            signed_promise.signature().clone(),
            compact_signed_promise.signature().clone()
        );
    }

    #[test]
    fn compact_signed_promise_correctly_builds_from_signed_promise() {
        let private_key = PrivateKey::random();
        let promise = Promise::mock(());

        let signed_promise = SignedPromise::create(private_key.clone(), promise).unwrap();

        let compact_signed_promise =
            CompactSignedPromise::try_from(&signed_promise).expect("valid signed promise");

        assert_eq!(signed_promise.address(), compact_signed_promise.address());
        assert_eq!(
            signed_promise.signature().clone(),
            compact_signed_promise.signature().clone()
        );
    }
}
