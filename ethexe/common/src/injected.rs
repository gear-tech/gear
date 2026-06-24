// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::{Address, EitherHashOf, HashOf, ToDigest, ecdsa::SignedMessage};
use alloc::{
    string::{String, ToString},
    vec::Vec,
};
#[cfg(feature = "shielded")]
use ark_serialize::CanonicalSerialize;
use core::hash::Hash;
use gear_core::{limited::LimitedVec, rpc::ReplyInfo};
#[cfg(feature = "shielded")]
use gear_tdec::{
    Result as TdecResult,
    bls12_381::{Ciphertext, DkgPublicKey, SharedSecret},
    rand_utils::Rng,
};
use gprimitives::{ActorId, H256, MessageId};
use gsigner::Signature;
use parity_scale_codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sha3::{Digest as _, Keccak256};

/// Recent block hashes window size used to check transaction mortality.
pub const VALIDITY_WINDOW: u8 = 32;

/// Maximum size of single injected transaction payload.
///
/// Limited by the maximum injected transactions size per MB.
/// Currently is 126 KiB.
pub const MAX_INJECTED_TX_PAYLOAD_SIZE: usize = 126 * 1024;

/// Maximum size of injected transaction salt.
pub const MAX_INJECTED_TX_SALT_SIZE: usize = 32;

/// Maximum cumulative SCALE-encoded size of [`SignedInjectedTransaction`]s
/// that a single MB may carry. 127 KiB leaves ~1 KiB of headroom over the
/// per-tx [`MAX_INJECTED_TX_PAYLOAD_SIZE`] for signature and other
/// envelope bytes, so at least one tx of the maximum payload size is
/// always admissible.
pub const MAX_INJECTED_TRANSACTIONS_SIZE_PER_MB: usize = 127 * 1024;

// TODO: rename this type to just `TransactionAcceptance`
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

/// IMPORTANT: message id == tx hash.
#[cfg_attr(feature = "std", derive(serde::Deserialize, serde::Serialize))]
#[cfg_attr(feature = "serde", derive(Hash))]
#[derive(Debug, Clone, Encode, Decode, MaxEncodedLen, TypeInfo, PartialEq, Eq)]
pub struct InjectedTransaction {
    /// Destination program inside `Vara.eth`.
    pub destination: ActorId,
    /// Payload of the message.
    #[cfg_attr(feature = "std", serde(with = "limited_vec_hex"))]
    pub payload: LimitedVec<u8, MAX_INJECTED_TX_PAYLOAD_SIZE>,
    /// Value attached to the message.
    /// NOTE: at this moment will be zero.
    pub value: u128,
    /// Reference block number.
    pub reference_block: H256,
    /// Arbitrary bytes to allow multiple synonymous
    /// transactions to be sent simultaneously.
    /// NOTE: this is also a salt for MessageId generation.
    #[cfg_attr(feature = "std", serde(with = "limited_vec_hex"))]
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
    pub fn to_hash(&self) -> HashOf<Self> {
        let hashable_bytes = self.to_hashable_bytes();
        unsafe { HashOf::new(gear_core::utils::hash(hashable_bytes.as_ref()).into()) }
    }

    /// Creates [`MessageId`] from [`InjectedTransaction`].
    pub fn to_message_id(&self) -> MessageId {
        MessageId::new(self.to_hash().inner().0)
    }

    #[cfg(feature = "shielded")]
    pub fn shield(
        self,
        public_key: &DkgPublicKey,
        rng: &mut impl Rng,
    ) -> TdecResult<ShieldedTransaction> {
        let shielded_fields = ShieldedFields {
            destination: self.destination,
            payload: self.payload,
            value: self.value,
        };
        // AAD is a keccak256 hash over shielded fields
        let aad = shielded_fields.to_digest();
        let ciphertext = gear_tdec::encrypt(&shielded_fields, aad.as_ref(), public_key, rng)?;

        Ok(ShieldedTransaction {
            ciphertext,
            aad,
            reference_block: self.reference_block,
            salt: self.salt,
        })
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

mod sealed {
    pub trait Sealed {}

    impl Sealed for super::Promise {}
    impl Sealed for super::CompactPromise {}
}

pub trait PromiseKind: sealed::Sealed {
    fn tx_hash(&self) -> HashOf<InjectedTransaction>;
}

impl PromiseKind for Promise {
    fn tx_hash(&self) -> HashOf<InjectedTransaction> {
        self.tx_hash
    }
}

impl PromiseKind for CompactPromise {
    fn tx_hash(&self) -> HashOf<InjectedTransaction> {
        self.tx_hash
    }
}

/// Receipt for [InjectedTransaction].
///
/// This type generic over promise type in purpose to support both
/// [CompactPromise] and [Promise].
///
/// [CompactPromise] generic uses only for transport purposes. End user
/// always receives the full version.
///
/// **Important**: `Receipt<CompactPromise>` and `Receipt<Promise>` have the same
///     digest. So it helps to reuses the producer's signature to construct the full
///     version from compact.
#[cfg(feature = "shielded")]
#[derive(
    Debug, Clone, PartialEq, Eq, Encode, Decode, derive_more::IsVariant, derive_more::Unwrap,
)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub enum Receipt<P> {
    Promise(P),
    /// No promise, transaction wasn't executed.
    Purged(PurgedTransaction),
}

#[cfg(feature = "shielded")]
impl<P: PromiseKind> Receipt<P> {
    pub fn tx_hash(&self) -> TransactionHash {
        match self {
            Self::Promise(promise) => TransactionHash::Left(promise.tx_hash()),
            Self::Purged(purged) => purged.tx_hash,
        }
    }
}

#[cfg(feature = "shielded")]
impl<P: ToDigest> ToDigest for Receipt<P> {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        match self {
            Self::Promise(promise) => {
                hasher.update([0]);
                promise.update_hasher(hasher);
            }
            Self::Purged(err) => {
                hasher.update([1]);
                err.update_hasher(hasher);
            }
        }
    }
}

/// Signed [Receipt] with a [Promise] generic.
/// End RPC user always receives this object.
#[cfg(feature = "shielded")]
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, derive_more::From, derive_more::Deref)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "std", serde(transparent))]
pub struct SignedTxReceipt(pub SignedMessage<Receipt<Promise>>);

/// Signed [Receipt] with a [CompactPromise] generic.
/// It is used as a lightweight transfer type
#[cfg(feature = "shielded")]
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, derive_more::Deref, derive_more::From)]
pub struct SignedCompactTxReceipt(SignedMessage<Receipt<CompactPromise>>);

/// The result of [upgrade](SignedCompactTxReceipt::upgrade) function.
/// [Ready](Self::Ready) means that receipt contains an error and was upgraded
/// to full version.
/// [Pending](Self::Pending) means that receipt contains a promise and requires the
/// full promise body to restore receipt.
#[cfg(feature = "shielded")]
#[derive(Debug, PartialEq, Eq, derive_more::From)]
pub enum UpgradedReceipt {
    Pending(UnfilledPromiseReceipt),
    Ready(SignedTxReceipt),
}

#[cfg(feature = "shielded")]
impl SignedCompactTxReceipt {
    /// Upgrades the compact receipt to its full version ([SignedTxReceipt]).
    pub fn upgrade(self) -> UpgradedReceipt {
        let (receipt, signature, address) = self.0.into_parts_full();

        match receipt {
            Receipt::Promise(compact) => {
                UpgradedReceipt::Pending(UnfilledPromiseReceipt(compact, signature, address))
            }
            Receipt::Purged(purged) => UpgradedReceipt::Ready(unsafe {
                // SAFETY: Receipt::Purged has the same digest representation for both
                // Promise and CompactPromise generics, so the signature remains valid.
                SignedMessage::from_parts_unchecked(Receipt::Purged(purged), signature, address)
                    .into()
            }),
        }
    }
}

/// Intermediate type between receipt's states [SignedCompactTxReceipt] and [SignedTxReceipt].
/// Use method [try_fill_with](Self::try_fill_with) to build the [SignedTxReceipt].
#[derive(Debug, Clone, PartialEq, Eq, derive_more::Deref)]
pub struct UnfilledPromiseReceipt(#[deref] CompactPromise, Signature, Address);

/// The result of [try_fill_with](UnfilledPromiseReceipt::try_fill_with) function.
/// [Filled](Self::Filled) means the successful result.
/// [HashesMismatch](Self::HashesMismatch) means that raw promise body and stored compact are not the same promise.
#[cfg(feature = "shielded")]
pub enum TryFillPromiseResult {
    Filled(SignedTxReceipt),
    HashesMismatch(UnfilledPromiseReceipt),
}

#[cfg(feature = "shielded")]
impl UnfilledPromiseReceipt {
    pub fn try_fill_with(self, promise: Promise) -> TryFillPromiseResult {
        if self.0 != promise.to_compact() {
            return TryFillPromiseResult::HashesMismatch(self);
        }
        let Self(.., signature, address) = self;
        TryFillPromiseResult::Filled(unsafe {
            SignedMessage::from_parts_unchecked(Receipt::Promise(promise), signature, address)
                .into()
        })
    }
}

/// Represents the reason why transaction was not included.
#[cfg(feature = "shielded")]
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, derive_more::Display)]
#[cfg_attr(feature = "std", derive(serde::Deserialize, serde::Serialize))]
#[display("Injected transaction wasn't executed: tx_hash={tx_hash}, reason={reason}")]
pub struct PurgedTransaction {
    /// Has of [InjectedTransaction] or [ShieldedTransaction].
    pub tx_hash: TransactionHash,
    /// Reason why transaction was purged from mempool.
    pub reason: TransactionPurgedReason,
}

#[cfg(feature = "shielded")]
impl ToDigest for PurgedTransaction {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        let Self { tx_hash, reason } = self;
        tx_hash.update_hasher(hasher);
        hasher.update([reason.variant_index()]);
    }
}

/// Reason why transaction was not executed in chain.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, derive_more::Display)]
#[cfg_attr(feature = "std", derive(serde::Deserialize, serde::Serialize))]
pub enum TransactionPurgedReason {
    /// The transaction references an outdated block and cannot be included.
    #[display("transaction reference block is outdated")]
    Outdated = 1,
    /// The transaction references a block that is not known locally.
    #[display("transaction reference block is unknown")]
    UnknownReferenceBlock = 2,
    /// The shielded transaction could not be decrypted.
    #[display("failed to decrypt shielded transaction")]
    DecryptionFailed = 3,

    /// The transaction has a non-zero value, which is not supported yet.
    ///
    /// Note: keep this variant at the end of the enum. The `u8::MAX`
    /// discriminant intentionally leaves values `3..=254` available for
    /// future purge reasons, including non-zero-value injected transactions.
    #[display("transaction value must be zero")]
    NonZeroValue = u8::MAX,
}

impl TransactionPurgedReason {
    pub fn variant_index(&self) -> u8 {
        *self as u8
    }
}

#[cfg(feature = "shielded")]
#[cfg_attr(feature = "serde", derive(Hash))]
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct ShieldedFields {
    pub destination: ActorId,
    pub value: u128,
    pub payload: LimitedVec<u8, MAX_INJECTED_TX_PAYLOAD_SIZE>,
}

#[cfg(feature = "shielded")]
impl ToDigest for ShieldedFields {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        let Self {
            destination,
            value,
            payload,
        } = &self;
        hasher.update(destination);
        hasher.update(value.to_be_bytes());
        hasher.update(payload);
    }
}

#[cfg(feature = "shielded")]
#[cfg_attr(feature = "std", derive(serde::Deserialize, serde::Serialize))]
#[cfg_attr(feature = "serde", derive(Hash))]
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ShieldedTransaction {
    /// Encrypted fields of initial [InjectedTransaction].
    pub ciphertext: Ciphertext<ShieldedFields>,
    /// Keccak256 hash over [ShieldedFields].
    #[cfg_attr(feature = "std", serde(with = "digest_hex"))]
    pub aad: gsigner::Digest,
    /// Reference block number.
    pub reference_block: H256,
    /// Arbitrary bytes to allow multiple synonymous
    /// transactions to be sent simultaneously.
    /// NOTE: this is also a salt for MessageId generation.
    #[cfg_attr(feature = "std", serde(with = "limited_vec_hex"))]
    pub salt: LimitedVec<u8, MAX_INJECTED_TX_SALT_SIZE>,
}

#[cfg(feature = "shielded")]
impl ShieldedTransaction {
    fn append_compressed_point<P: CanonicalSerialize>(buffer: &mut Vec<u8>, point: &P) {
        point
            .serialize_compressed(buffer)
            .expect("serializing to Vec should not fail");
    }

    pub(crate) fn to_hashable_bytes(&self) -> Vec<u8> {
        let mut buffer = Vec::with_capacity(
            self.ciphertext.commitment.compressed_size()
                + self.ciphertext.auth_tag.compressed_size()
                + size_of::<H256>()
                + size_of::<gsigner::Digest>()
                + size_of::<H256>()
                + size_of::<H256>(),
        );

        Self::append_compressed_point(&mut buffer, &self.ciphertext.commitment);
        Self::append_compressed_point(&mut buffer, &self.ciphertext.auth_tag);
        buffer.extend_from_slice(gear_core::utils::hash(&self.ciphertext.ciphertext).as_ref());
        buffer.extend_from_slice(self.aad.as_ref());
        buffer.extend_from_slice(self.reference_block.0.as_ref());
        buffer.extend_from_slice(gear_core::utils::hash(&self.salt).as_ref());

        buffer
    }

    /// Constructs blake2b hash over [ShieldedTransaction].
    pub fn to_hash(&self) -> HashOf<Self> {
        let hashable_bytes = self.to_hashable_bytes();
        unsafe { HashOf::new(gear_core::utils::hash(hashable_bytes.as_ref()).into()) }
    }
}

#[cfg(feature = "shielded")]
pub type SignedShieldedTransaction = SignedMessage<ShieldedTransaction>;

#[cfg(feature = "shielded")]
impl ToDigest for ShieldedTransaction {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        hasher.update(self.to_hashable_bytes());
    }
}

#[cfg(feature = "shielded")]
impl ShieldedTransaction {
    /// Decrypts [Ciphertext] with provided [SharedSecret].
    /// Returns initial [InjectedTransaction].
    pub fn unshield(self, shared_secret: &SharedSecret) -> TdecResult<InjectedTransaction> {
        let unshielded_fields =
            gear_tdec::decrypt(&self.ciphertext, self.aad.as_ref(), shared_secret)?;

        if unshielded_fields.to_digest() != self.aad {
            return Err(gear_tdec::Error::CiphertextVerificationFailed);
        }

        Ok(InjectedTransaction {
            destination: unshielded_fields.destination,
            payload: unshielded_fields.payload,
            value: unshielded_fields.value,
            reference_block: self.reference_block,
            salt: self.salt,
        })
    }
}

#[cfg(feature = "shielded")]
#[cfg_attr(feature = "std", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, Encode, Decode, Eq, PartialEq, derive_more::From)]
#[allow(clippy::large_enum_variant)]
pub enum Transaction {
    Injected(SignedInjectedTransaction),
    Shielded(SignedShieldedTransaction),
}

#[cfg(feature = "shielded")]
/// Type alias over [EitherHashOf].
pub type TransactionHash = EitherHashOf<InjectedTransaction, ShieldedTransaction>;

#[cfg(feature = "shielded")]
impl Transaction {
    pub fn as_ref(&self) -> TransactionRef<'_> {
        match self {
            Self::Injected(tx) => TransactionRef::Injected(tx),
            Self::Shielded(tx) => TransactionRef::Shielded(tx),
        }
    }

    pub fn as_injected(&self) -> Option<&SignedInjectedTransaction> {
        match self {
            Self::Injected(tx) => Some(tx),
            Self::Shielded(_) => None,
        }
    }
}

/// Mirroring [Transaction] type, but stores internally references to
/// transactions variants.
///
/// # Usage
/// This type must be used to transform [Operation] type into [Option<TransactionRef>].
///
/// [Operation]: crate::malachite::Operation
#[cfg(feature = "shielded")]
#[derive(Clone, Copy)]
pub enum TransactionRef<'op> {
    Injected(&'op SignedInjectedTransaction),
    Shielded(&'op SignedShieldedTransaction),
}

#[cfg(feature = "shielded")]
impl<'t> TransactionRef<'t> {
    pub fn hash(&self) -> TransactionHash {
        match self {
            Self::Injected(tx) => TransactionHash::Left(tx.data().to_hash()),
            Self::Shielded(tx) => TransactionHash::Right(tx.data().to_hash()),
        }
    }

    pub fn reference_block(&self) -> H256 {
        match self {
            Self::Injected(tx) => tx.data().reference_block,
            Self::Shielded(tx) => tx.data().reference_block,
        }
    }
}

/// Encoding and decoding of [LimitedVec<u8, N>] as hex string.
#[cfg(feature = "std")]
mod limited_vec_hex {
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

#[cfg(feature = "std")]
mod digest_hex {
    use gsigner::Digest;

    pub fn serialize<S>(digest: &Digest, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        alloy_primitives::hex::serialize(digest.0, serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Digest, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        alloy_primitives::hex::deserialize::<D, [u8; 32]>(deserializer).map(Digest)
    }
}

#[cfg(all(test, feature = "mock"))]
mod tests {
    use std::ops::Mul;

    use ark_ec::{AffineRepr, pairing::Pairing};
    use gear_tdec::bls12_381::{E as Bls12_381, Fr};
    use gsigner::PrivateKey;

    use super::*;
    use crate::mock::Mock;

    /// You can use this JavaScript code to reproduce serialize/deserialize paths.
    /// ```no_run,ignore
    /// import { bls12_381 } from '@noble/curves/bls12-381.js';
    /// const { G1, G2 } = bls12_381;
    ///
    /// function bytesToHex(bytes) {
    ///     return Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('');
    /// }
    /// function dumpPoint(name, point) {
    ///     const compressed = point.toBytes(true);
    ///     console.log(`\n${name}`);
    ///     console.log(`compressed hex: 0x${bytesToHex(compressed)}`);
    /// }
    ///
    /// dumpPoint('G1 * 123', G1.Point.BASE.multiply(123n));
    /// dumpPoint('G2 * 123', G2.Point.BASE.multiply(123n));
    /// ```
    #[test]
    fn ark_noble_js_compatible_serialization() {
        const NOBLE_JS_G1_123_COMPRESSED_SERIALIZED: &str = r#""0xa0ec3e71a719a25208adc97106b122809210faf45a17db24f10ffb1ac014fac1ab95a4a1967e55b185d4df622685b9e8""#;
        const NOBLE_JS_G2_123_COMPRESSED_SERIALIZED: &str = r#""0x95e18bbdb8b7bd39ea677ee923d7e87af449c45209e635907a4a8a2e4c65fff97c46d038cff53a994da273310ac85866096a5e13fd3ebf4e140e26f6ddfac66651e04e530e6045572acab753bb1bcef990fe14b4426caee41016af69d313750d""#;

        #[derive(serde::Serialize, serde::Deserialize)]
        #[serde(transparent)]
        struct G1Wrapper {
            #[serde(with = "gear_tdec::serialization::ark_serde_hex")]
            pub point: <Bls12_381 as Pairing>::G1,
        }

        let g1_123 = <Bls12_381 as Pairing>::G1Affine::generator().mul(Fr::from(123));
        let wrapped_g1 = G1Wrapper { point: g1_123 };
        assert_eq!(
            serde_json::to_string(&wrapped_g1).unwrap(),
            NOBLE_JS_G1_123_COMPRESSED_SERIALIZED
        );

        #[derive(serde::Serialize, serde::Deserialize)]
        #[serde(transparent)]
        struct G2Wrapper {
            #[serde(with = "gear_tdec::serialization::ark_serde_hex")]
            pub point: <Bls12_381 as Pairing>::G2,
        }

        let g2_123 = <Bls12_381 as Pairing>::G2Affine::generator().mul(Fr::from(123));
        let wrapped_g2 = G2Wrapper { point: g2_123 };
        assert_eq!(
            serde_json::to_string(&wrapped_g2).unwrap(),
            NOBLE_JS_G2_123_COMPRESSED_SERIALIZED
        );
    }

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

    /// Ported from master's `tx_pool::tests::validate_max_tx_size`.
    /// One full-size [`SignedInjectedTransaction`] must always fit within
    /// the per-MB cumulative size cap; otherwise the largest legal tx
    /// could never be admitted.
    #[test]
    fn max_signed_injected_tx_fits_per_mb_cap() {
        assert!(
            SignedInjectedTransaction::max_encoded_len() <= MAX_INJECTED_TRANSACTIONS_SIZE_PER_MB
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
    fn tx_receipt_has_the_same_hash_for_promise() {
        let pk = PrivateKey::random();
        let promise = Promise::mock(());
        let compact_promise = promise.to_compact();

        let receipt_promise = Receipt::Promise(promise);
        let receipt_compact_promise = Receipt::Promise(compact_promise);
        assert_eq!(
            receipt_promise.to_digest(),
            receipt_compact_promise.to_digest()
        );

        let signed_receipt = SignedMessage::create(pk.clone(), receipt_promise).unwrap();
        let signed_compact_receipt = SignedMessage::create(pk, receipt_compact_promise).unwrap();

        assert_eq!(
            *signed_receipt.signature(),
            *signed_compact_receipt.signature()
        );
        assert_eq!(signed_receipt.address(), signed_compact_receipt.address());
    }

    #[test]
    fn tx_receipt_has_the_same_hash_for_error() {
        let purged = PurgedTransaction {
            tx_hash: unsafe { TransactionHash::Left(HashOf::new(H256::random())) },
            reason: TransactionPurgedReason::Outdated,
        };
        let receipt1 = Receipt::<Promise>::Purged(purged.clone());
        let receipt2 = Receipt::<CompactPromise>::Purged(purged);

        assert_eq!(receipt1.to_digest(), receipt2.to_digest());
    }

    #[test]
    fn shielded_tx_serde() {
        let injected_tx = InjectedTransaction::mock(());
        let mut rng = gear_tdec::rand_utils::test_rng();
        let dealer_out = gear_tdec::deal::<gear_tdec::bls12_381::E>(3, 2, &mut rng);

        let shielded_tx = injected_tx
            .shield(&dealer_out.public_key, &mut rng)
            .unwrap();

        let serialized = serde_json::to_string_pretty(&shielded_tx).unwrap();
        let deserialized: ShieldedTransaction = serde_json::from_str(&serialized).unwrap();
        assert_eq!(shielded_tx, deserialized);
    }

    #[test]
    fn signed_message_and_shielded_transactions() {
        let injected_tx = InjectedTransaction::mock(());
        let mut rng = gear_tdec::rand_utils::test_rng();
        let dealer_out = gear_tdec::deal::<gear_tdec::bls12_381::E>(3, 2, &mut rng);
        let shielded_tx = injected_tx
            .shield(&dealer_out.public_key, &mut rng)
            .unwrap();

        let signed_tx =
            SignedShieldedTransaction::create(PrivateKey::random(), shielded_tx).unwrap();

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
    fn mock_display() {
        let hash = InjectedTransaction::mock(()).to_hash();
        let h = TransactionHash::Left(hash);
        println!("{h}");
    }
}
