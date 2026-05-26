// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::{Address, HashOf, ToDigest, ecdsa::SignedMessage};
use alloc::string::{String, ToString};
use core::hash::Hash;
use gear_core::{limited::LimitedVec, rpc::ReplyInfo};
use gprimitives::{ActorId, H256, MessageId};
use gsigner::Signature;
use parity_scale_codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sha3::{Digest, Keccak256};

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

#[cfg_attr(feature = "std", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, Encode, Decode, Eq, PartialEq)]
pub enum InjectedTransactionAcceptance {
    Accept,
    /// Mempool already holds (or has recently committed) this tx. The promise
    /// will still fire — the subscription should stay open and fan-out should
    /// prefer this over a `Reject`.
    AlreadyPooled {
        reason: String,
    },
    Reject {
        reason: String,
    },
}

impl InjectedTransactionAcceptance {
    /// Either fresh acceptance or duplicate of a pooled tx — the caller's
    /// promise subscription will receive the reply in both cases.
    pub fn is_promise_bound(&self) -> bool {
        matches!(self, Self::Accept | Self::AlreadyPooled { .. })
    }
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
#[derive(
    Debug, Clone, PartialEq, Eq, Encode, Decode, derive_more::IsVariant, derive_more::Unwrap,
)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub enum Receipt<P> {
    Promise(P),
    /// No promise, transaction wasn't executed.
    Error(TransactionError),
}

impl<P: PromiseKind> Receipt<P> {
    pub fn tx_hash(&self) -> HashOf<InjectedTransaction> {
        match self {
            Self::Promise(promise) => promise.tx_hash(),
            Self::Error(err) => err.tx_hash,
        }
    }
}

impl<P: ToDigest> ToDigest for Receipt<P> {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        match self {
            Self::Promise(promise) => {
                hasher.update([0]);
                hasher.update(promise.to_digest().0);
            }
            Self::Error(err) => {
                hasher.update([1]);
                hasher.update(err.to_digest().0);
            }
        }
    }
}

/// Signed [Receipt] with a [Promise] generic.
/// End RPC user always receives this object.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, derive_more::From, derive_more::Deref)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "std", serde(transparent))]
pub struct SignedTxReceipt(SignedMessage<Receipt<Promise>>);

/// Signed [Receipt] with a [CompactPromise] generic.
/// It is used as a lightweight transfer type
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, derive_more::Deref, derive_more::From)]
pub struct SignedCompactTxReceipt(SignedMessage<Receipt<CompactPromise>>);

/// The result of [upgrade](SignedCompactTxReceipt::upgrade) function.
/// [Ready](Self::Ready) means that receipt contains an error and was upgraded
/// to full version.
/// [Pending](Self::Pending) means that receipt contains a promise and requires the
/// full promise body to restore receipt.
#[derive(Debug, PartialEq, Eq, derive_more::From)]
pub enum UpgradedReceipt {
    Pending(UnfilledPromiseReceipt),
    Ready(SignedTxReceipt),
}

impl SignedCompactTxReceipt {
    /// Upgrades the compact receipt to its full version ([SignedTxReceipt]).
    pub fn upgrade(self) -> UpgradedReceipt {
        let (receipt, signature, address) = self.0.into_parts_full();

        match receipt {
            Receipt::Promise(compact) => {
                UpgradedReceipt::Pending(UnfilledPromiseReceipt(compact, signature, address))
            }
            Receipt::Error(error) => UpgradedReceipt::Ready(unsafe {
                SignedMessage::from_parts_unchecked(Receipt::Error(error), signature, address)
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
pub enum TryFillPromiseResult {
    Filled(SignedTxReceipt),
    HashesMismatch(UnfilledPromiseReceipt),
}

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

/// Represents the reason why [InjectedTransaction] was not included.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, derive_more::Display)]
#[cfg_attr(feature = "std", derive(serde::Deserialize, serde::Serialize))]
#[display("Injected transaction wasn't executed: tx_hash={tx_hash}, reason={reason}")]
pub struct TransactionError {
    pub tx_hash: HashOf<InjectedTransaction>,
    pub reason: TransactionErrorReason,
}

impl ToDigest for TransactionError {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        let Self { tx_hash, reason } = self;
        hasher.update(tx_hash.inner().0);
        hasher.update([reason.variant_index()]);
    }
}

/// Reason why transaction was not executed in chain.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, derive_more::Display)]
#[cfg_attr(feature = "std", derive(serde::Deserialize, serde::Serialize))]
pub enum TransactionErrorReason {
    /// Transaction is outdated and can not be included.
    #[display("Transaction is outdated")]
    Outdated = 1,

    // Important: Keep it in the end of enum.
    //      In future we will support non zero value injected txs.
    #[display("Transaction's value must be zero")]
    NonZeroValue = 2,
}

impl TransactionErrorReason {
    pub fn variant_index(&self) -> u8 {
        unsafe { (self as *const TransactionErrorReason as *const u8).read() }
    }
}

/// Encoding and decoding of [LimitedVec<u8, N>] as hex string.
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
        let error = TransactionError {
            tx_hash: unsafe { HashOf::new(H256::random()) },
            reason: TransactionErrorReason::Outdated,
        };
        let receipt1 = Receipt::<Promise>::Error(error.clone());
        let receipt2 = Receipt::<CompactPromise>::Error(error);

        assert_eq!(receipt1.to_digest(), receipt2.to_digest());
    }
}
