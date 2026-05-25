// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

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

/// IMPORTANT: message id == tx hash.
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

// Destination + payload_hash + value + ref_block + salt_hash
const INJECTED_TX_HASHABLE_SIZE: usize = size_of::<ActorId>()
    + size_of::<H256>()
    + size_of::<u128>()
    + size_of::<H256>()
    + size_of::<H256>();

impl InjectedTransaction {
    /// Helper function that returns bytes of [InjectedTransaction]
    /// that will be hashed by blake2b256 or keccak256.
    fn to_hashable_bytes(&self) -> [u8; INJECTED_TX_HASHABLE_SIZE] {
        let Self {
            destination,
            payload,
            value,
            reference_block,
            salt,
        } = self;

        let mut hashable_bytes = [0u8; INJECTED_TX_HASHABLE_SIZE];
        let mut offset = 0;

        let mut append = |slice: &[u8]| {
            let next_offset = offset + slice.len();
            hashable_bytes[offset..next_offset].copy_from_slice(slice);
            offset = next_offset;
        };

        append(destination.as_ref());
        append(gear_core::utils::hash(payload).as_ref());
        append(value.to_be_bytes().as_ref());
        append(reference_block.0.as_ref());
        append(gear_core::utils::hash(salt).as_ref());

        hashable_bytes
    }

    /// Returns the hash of [`InjectedTransaction`].
    pub fn to_hash(&self) -> HashOf<InjectedTransaction> {
        let hashable_bytes = self.to_hashable_bytes();
        unsafe { HashOf::new(gear_core::utils::hash(hashable_bytes.as_ref()).into()) }
    }

    /// Creates [`MessageId`] from [`InjectedTransaction`].
    pub fn to_message_id(&self) -> MessageId {
        MessageId::new(self.to_hash().inner().0)
    }
}

impl ToDigest for InjectedTransaction {
    fn update_hasher(&self, hasher: &mut Keccak256) {
        let hashable_bytes = self.to_hashable_bytes();
        hasher.update(hashable_bytes);
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

    /// Converts promise to its compact version.
    pub fn to_compact(&self) -> CompactPromise {
        CompactPromise {
            tx_hash: self.tx_hash,
            reply_hash: self.reply_hash(),
        }
    }
}

impl ToDigest for Promise {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        self.to_compact().update_hasher(hasher);
    }
}

/// A signed wrapper on top of [`CompactPromise`].
///
/// [`SignedCompactPromise`] is a lightweight version of [`SignedPromise`], that is
/// needed to reduce the amount of data transferred in network between validators.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, derive_more::Deref, derive_more::From)]
pub struct SignedCompactPromise(SignedMessage<CompactPromise>);

/// The hashes of [`Promise`] parts.
#[derive(Debug, Clone, Encode, Decode, PartialEq, Eq)]
pub struct CompactPromise {
    pub tx_hash: HashOf<InjectedTransaction>,
    pub reply_hash: HashOf<ReplyInfo>,
}

impl ToDigest for CompactPromise {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        let Self {
            tx_hash,
            reply_hash,
        } = self;

        hasher.update(tx_hash.inner());
        hasher.update(reply_hash.inner());
    }
}

impl SignedCompactPromise {
    /// Create the [`SignedCompactPromise`] from private key and hashes.
    pub fn create(private_key: PrivateKey, promise_hashes: CompactPromise) -> SignResult<Self> {
        SignedMessage::create(private_key, promise_hashes).map(SignedCompactPromise)
    }

    pub fn create_from_promise(private_key: PrivateKey, promise: &Promise) -> SignResult<Self> {
        Self::create(private_key, promise.to_compact())
    }

    /// Create the [`SignedCompactPromise`] from a valid [`SignedPromise`].
    ///
    /// # Panics
    /// Panics if the digest of [`Promise`] and [`CompactPromise`] ever diverge.
    /// This must hold by construction; tests enforce the invariant.
    pub fn from_signed_promise(signed_promise: &SignedPromise) -> Self {
        let compact = signed_promise.data().to_compact();
        let (signature, address) = (*signed_promise.signature(), signed_promise.address());

        SignedMessage::try_from_parts(compact, signature, address)
            .expect("SignedPromise and CompactPromise must have identical digest")
            .into()
    }

    /// Tries to restore the [SignedPromise] with provided [Promise] body.
    pub fn restore(&self, promise: Promise) -> Result<SignedPromise, &'static str> {
        SignedMessage::try_from_parts(promise, *self.0.signature(), self.0.address())
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
            "signature": "0x030a25167f5b18aba302c16226a1f5e590bba1adf5c49430040518416d3caac41d7f5b8c5df142d3c6db2a8e36ca0ca3f42640441d980c54b0847ada2580000f1b",
            "address": "0xfb2f65ffad2971b699097990ab7a1d4ac35bd0ff"
        }"#;

        let signed_tx: SignedInjectedTransaction =
            serde_json::from_str(RPC_INPUT).expect("failed to deserialize SignedMessage");

        // AKA tx_hash
        assert_eq!(
            hex::encode(signed_tx.data().to_message_id()),
            "70ab92fb3161d1feefbd4793ed1217574e71c802d4d8af01648863d3ba7e37c1"
        );

        assert_eq!(
            hex::encode(signed_tx.address().0),
            "fb2f65ffad2971b699097990ab7a1d4ac35bd0ff"
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

        assert_eq!(promise.to_digest(), promise.to_compact().to_digest());
    }

    #[test]
    fn shifted_bytes_change_injected_tx_hash() {
        let initial_tx = InjectedTransaction {
            destination: ActorId::zero(),
            payload: vec![1u8, 2u8, 3u8, 4u8].try_into().unwrap(),
            value: 100,
            reference_block: H256::random(),
            salt: vec![1u8, 2u8].try_into().unwrap(),
        };

        let malicious_tx = {
            let mut shifted_tx = initial_tx.clone();

            let mut payload = shifted_tx.payload.into_vec();
            let payload_last_byte = payload.pop().unwrap();
            shifted_tx.payload = payload.try_into().unwrap();

            let mut value_be = shifted_tx.value.to_be_bytes();
            let value_last_byte = value_be[15];
            value_be.copy_within(0..15, 1);
            value_be[0] = payload_last_byte;
            shifted_tx.value = u128::from_be_bytes(value_be);

            let mut ref_block_data = shifted_tx.reference_block.0;
            let last_ref_block = ref_block_data[31];

            ref_block_data.copy_within(0..31, 1);
            ref_block_data[0] = value_last_byte;

            shifted_tx.reference_block = H256(ref_block_data);

            let mut salt = shifted_tx.salt.clone().into_vec();
            salt.insert(0, last_ref_block);
            shifted_tx.salt = salt.try_into().unwrap();

            shifted_tx
        };

        let tx_concat_bytes = |tx: &InjectedTransaction| -> Vec<u8> {
            [
                tx.destination.as_ref(),
                tx.payload.as_ref(),
                tx.value.to_be_bytes().as_ref(),
                tx.reference_block.0.as_ref(),
                tx.salt.as_ref(),
            ]
            .concat()
        };

        // Assert that transactions have the same concatenated bytes.
        // In earlier hash implementation it will lead to the same tx hashes.
        assert_eq!(tx_concat_bytes(&initial_tx), tx_concat_bytes(&malicious_tx));

        // Assert that current hash implementation return different hashes for transactions
        // that have equal concatenated bytes.
        assert_ne!(initial_tx.to_hash(), malicious_tx.to_hash());
    }

    #[test]
    fn signatures_equal_for_promise_and_compact_promise() {
        let private_key = PrivateKey::random();
        let promise = Promise::mock(());

        let signed_promise = SignedPromise::create(private_key.clone(), promise.clone()).unwrap();
        let compact_signed_promise =
            SignedCompactPromise::create_from_promise(private_key, &promise).unwrap();

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

        let compact_signed_promise = SignedCompactPromise::from_signed_promise(&signed_promise);

        assert_eq!(signed_promise.address(), compact_signed_promise.address());
        assert_eq!(
            signed_promise.signature().clone(),
            compact_signed_promise.signature().clone()
        );
    }
}
