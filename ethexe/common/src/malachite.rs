// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Application-level block shape produced by the Malachite sequencer
//! and consumed by the ethexe executor.
//!
//! [`Operations`] is the application schema: an ordered list of
//! [`Operation`]s. The consensus engine ships it SCALE-encoded as an
//! opaque, size-capped byte string (the malachite `Block` payload);
//! the encoding/decoding lives behind the consensus boundary.
//!
//! Protocol evolution is additive: a new behaviour gets a new
//! [`Operation`] variant with the next free `#[repr(u32)]` discriminant
//! (existing discriminants and their payloads are frozen forever, so
//! every historical operation stays decodable). Which operations a
//! validator *accepts* in a fresh proposal is gated separately, on the
//! validator side — older operations can be retired from new blocks
//! without ever losing the ability to decode and replay them.
//!
//! Block-level identity (parent linkage, height) lives in
//! [`crate::db::CompactMb`], indexed by the consensus block envelope
//! hash. The matching [`Operations`] blob is stored in the
//! content-addressed half of the ethexe db and referenced by
//! `CompactMb::operations_hash`.
//!
//! These types live in `ethexe-common` (rather than inside
//! `ethexe-malachite`) so `ethexe-processor` can accept them without
//! depending on the consensus layer.

use crate::injected::SignedInjectedTransaction;
#[cfg(feature = "shielded")]
use crate::{HashOf, ToDigest, injected::ShieldedTransaction};
use alloc::vec::Vec;
use derive_more::{Deref, DerefMut, IntoIterator};
use gprimitives::H256;
use parity_scale_codec::{Decode, Encode};
#[cfg(feature = "shielded")]
use {
    crate::injected::SignedShieldedTransaction,
    gear_tdec::bls12_381::DecryptionShareSimple,
    gsigner::{PublicDecryptionContext, SignedMessage},
    sha3::Keccak256,
};

#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};

/// A single operation in the malachite block.
#[derive(Clone, Debug, PartialEq, Eq, derive_more::IsVariant)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[repr(u32)]
pub enum Operation {
    /// Pin executor's view to a quarantine-passed Ethereum block.
    AdvanceTillEthereumBlock { block_hash: H256 } = 0,

    /// Progress scheduled tasks (mailbox/waitlist/reservation cleanup).
    ProgressTasks = 1,

    /// Execute queued message within `gas_allowance`.
    ProcessQueues { gas_allowance: u64 } = 2,

    /// User-submitted transaction from the mempool.
    Injected(SignedInjectedTransaction) = 3,

    /// Execute queued messages within `gas_allowance`.
    /// V2 - changes mailbox validity, from one week to 15 minutes
    ProcessQueuesV2 { gas_allowance: u64 } = 4,

    /// Execute queued messages within `gas_allowance`.
    /// V3 - auto-replies to Sails event destinations without mailboxing and
    /// emits Ethereum event destinations via transition messages.
    ProcessQueuesV3 { gas_allowance: u64 } = 5,

    /// User-submitted shielded transaction from mempool.
    #[cfg(feature = "shielded")]
    Shielded(SignedShieldedTransaction) = 6, // encrypted transactions
}

impl Operation {
    /// The `u32` discriminant identifying this variant — the value written
    /// first by [`Encode`] and read back by [`Decode`].
    ///
    /// Discriminants are part of the consensus wire format: existing values
    /// are frozen forever (a new operation gets the next free number), so a
    /// node always decodes every historical operation it has ever seen.
    pub fn tag(&self) -> u32 {
        // Mirrors the `#[repr(u32)]` discriminants below and the `Decode`
        // arms. These three must agree; `operation_encoding_is_frozen` pins
        // the bytes so a divergence can't slip through.
        unsafe { (self as *const Operation).cast::<u32>().read() }
    }

    /// Returns `Some` if `Self` contains shielded transaction.
    #[cfg(feature = "shielded")]
    pub fn as_shielded(&self) -> Option<&SignedShieldedTransaction> {
        match self {
            Self::Shielded(tx) => Some(tx),
            _ => None,
        }
    }

    #[cfg(feature = "shielded")]
    pub fn into_shielded(self) -> Option<SignedShieldedTransaction> {
        match self {
            Self::Shielded(tx) => Some(tx),
            _ => None,
        }
    }
}

// Custom encoder/decoder so the discriminant is always a fixed-width `u32`
// tag, sidestepping parity-scale-codec's compact enum-index encoding (which
// only addresses up to 255 variants) and keeping room for many operations.

impl Decode for Operation {
    fn decode<I: parity_scale_codec::Input>(
        input: &mut I,
    ) -> core::result::Result<Self, parity_scale_codec::Error> {
        let tag = u32::decode(input)?;
        match tag {
            0 => Ok(Operation::AdvanceTillEthereumBlock {
                block_hash: H256::decode(input)?,
            }),
            1 => Ok(Operation::ProgressTasks),
            2 => Ok(Operation::ProcessQueues {
                gas_allowance: u64::decode(input)?,
            }),
            3 => Ok(Operation::Injected(SignedInjectedTransaction::decode(
                input,
            )?)),
            4 => Ok(Operation::ProcessQueuesV2 {
                gas_allowance: u64::decode(input)?,
            }),
            5 => Ok(Operation::ProcessQueuesV3 {
                gas_allowance: u64::decode(input)?,
            }),
            #[cfg(feature = "shielded")]
            6 => Ok(Operation::Shielded(SignedShieldedTransaction::decode(
                input,
            )?)),
            _ => Err(parity_scale_codec::Error::from("invalid operation tag")),
        }
    }
}

impl Encode for Operation {
    fn encode_to<T: parity_scale_codec::Output + ?Sized>(&self, dest: &mut T) {
        self.tag().encode_to(dest);
        match self {
            Operation::AdvanceTillEthereumBlock { block_hash } => block_hash.encode_to(dest),
            Operation::ProgressTasks => {}
            Operation::ProcessQueues { gas_allowance } => gas_allowance.encode_to(dest),
            Operation::Injected(signed_tx) => signed_tx.encode_to(dest),
            Operation::ProcessQueuesV2 { gas_allowance } => gas_allowance.encode_to(dest),
            Operation::ProcessQueuesV3 { gas_allowance } => gas_allowance.encode_to(dest),
            #[cfg(feature = "shielded")]
            Operation::Shielded(shielded_tx) => shielded_tx.encode_to(dest),
        }
    }
}

/// Ordered list of [`Operation`]s; CAS key = Blake2b-256 of the SCALE-encoded list.
#[derive(Clone, Debug, Default, PartialEq, Eq, Encode, Decode, Deref, DerefMut, IntoIterator)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct Operations(pub Vec<Operation>);

impl Operations {
    pub fn new(operations: Vec<Operation>) -> Self {
        Self(operations)
    }

    /// CAS key: Blake2b-256 over the SCALE-encoded list.
    pub fn hash(&self) -> H256 {
        gear_core::utils::hash(&self.encode()).into()
    }
}

#[cfg(feature = "shielded")]
#[derive(Debug, Clone)]
pub struct MalachiteTdecContext {
    /// Minimal number of decryption shares required to decrypt transaction.
    pub threshold: u8,
    /// Current validator's public decryption context.
    /// Private data stored in [TdecKeyStore].
    ///
    /// [TdecKeyStore]: gsigner::tdec::TdecKeyStore
    pub my_context: PublicDecryptionContext,
    /// Public contexts of the remaining validators involved in decryption.
    pub others_contexts: Vec<PublicDecryptionContext>,
}

/// One validator's decryption-share payload for one shielded transaction.
/// Holds [`DecryptionShareSimple`] over [`ShieldedTransaction`].
///
/// [ShieldedTransaction]: crate::injected::ShieldedTransaction
#[cfg(feature = "shielded")]
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct ShieldedTxDecryptionShare {
    /// Transaction hash decryption share belongs to.
    pub tx_hash: HashOf<ShieldedTransaction>,
    pub share: DecryptionShareSimple,
}

#[cfg(feature = "shielded")]
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct BlockDecryptionData {
    /// Malachite block hash the decryption shares belong to.
    pub mb_hash: H256,
    /// Decryption shares for [`ShieldedTransaction`]s in the Malachite block.
    pub shares: Vec<ShieldedTxDecryptionShare>,
}

#[cfg(feature = "shielded")]
impl ToDigest for BlockDecryptionData {
    fn update_hasher(&self, hasher: &mut Keccak256) {
        // TODO:
    }
}

/// Validator-signed decryption shares for one Malachite block.
#[cfg(feature = "shielded")]
pub type SignedBlockDecryptionShares = SignedMessage<BlockDecryptionData>;

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_txs() -> Operations {
        Operations::new(alloc::vec![
            Operation::ProgressTasks,
            Operation::ProcessQueuesV3 {
                gas_allowance: 1234,
            },
        ])
    }

    #[test]
    fn hash_is_deterministic_for_same_content() {
        let a = empty_txs();
        let b = empty_txs();
        assert_eq!(a.hash(), b.hash());
    }

    #[test]
    fn hash_changes_when_operations_change() {
        let mut a = empty_txs();
        let b = empty_txs();
        a.push(Operation::AdvanceTillEthereumBlock {
            block_hash: H256::from_low_u64_be(0xEB),
        });
        assert_ne!(a.hash(), b.hash());
    }

    #[test]
    fn operation_tag_distinguishes_variants() {
        let advance = Operation::AdvanceTillEthereumBlock {
            block_hash: H256::zero(),
        };
        let progress = Operation::ProgressTasks;
        let queues = Operation::ProcessQueuesV3 {
            gas_allowance: 1234,
        };
        assert!(advance.is_advance_till_ethereum_block());
        assert!(progress.is_progress_tasks());
        assert!(queues.is_process_queues_v_3());
    }

    #[test]
    fn operation_encoding_is_frozen() {
        // The `Encode`/`Decode` impls hand-roll a fixed-width little-endian
        // `u32` tag, so the SCALE TypeInfo (derived) does NOT describe the real
        // wire format and the type-info-hash guard can't see a tag change. Pin
        // the exact leading tag bytes here: these discriminants are part of the
        // consensus wire format and must stay frozen forever.
        assert_eq!(
            Operation::AdvanceTillEthereumBlock {
                block_hash: H256::zero()
            }
            .tag(),
            0
        );
        assert_eq!(Operation::ProgressTasks.tag(), 1);
        assert_eq!(Operation::ProcessQueues { gas_allowance: 0 }.tag(), 2);
        assert_eq!(Operation::ProcessQueuesV2 { gas_allowance: 0 }.tag(), 4);
        assert_eq!(Operation::ProcessQueuesV3 { gas_allowance: 0 }.tag(), 5);

        assert_eq!(
            &Operation::AdvanceTillEthereumBlock {
                block_hash: H256::zero()
            }
            .encode()[..4],
            &[0, 0, 0, 0],
        );
        assert_eq!(Operation::ProgressTasks.encode(), [1, 0, 0, 0]);
        assert_eq!(
            &Operation::ProcessQueues { gas_allowance: 0 }.encode()[..4],
            &[2, 0, 0, 0],
        );
        assert_eq!(
            &Operation::ProcessQueuesV2 { gas_allowance: 0 }.encode()[..4],
            &[4, 0, 0, 0],
        );
        assert_eq!(
            &Operation::ProcessQueuesV3 { gas_allowance: 0 }.encode()[..4],
            &[5, 0, 0, 0],
        );

        // Unknown tag must be rejected by `Decode`, not interpreted.
        use parity_scale_codec::DecodeAll;
        assert!(Operation::decode_all(&mut [6u8, 0, 0, 0].as_slice()).is_err());
    }

    #[test]
    fn scale_round_trip_preserves_hash() {
        // `Operations` is SCALE-encoded for both the CAS payload
        // and the consensus wire payload — make sure round-trip is
        // hash-preserving so peers and the executor agree on the
        // CAS key.
        use parity_scale_codec::Decode;

        let original = Operations::new(alloc::vec![Operation::AdvanceTillEthereumBlock {
            block_hash: H256::from_low_u64_be(0xEB)
        }]);
        let encoded = original.encode();
        let decoded = Operations::decode(&mut encoded.as_slice()).expect("decode");
        assert_eq!(original, decoded);
        assert_eq!(original.hash(), decoded.hash());
    }
}
