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
use alloc::vec::Vec;
use derive_more::{Deref, DerefMut, IntoIterator};
use gprimitives::H256;
use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;

#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};

/// A single operation in the malachite block.
#[derive(Clone, Debug, PartialEq, Eq, TypeInfo, derive_more::IsVariant)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[repr(u32)]
pub enum Operation {
    /// Pin executor's view to a quarantine-passed Ethereum block.
    AdvanceTillEthereumBlock { block_hash: H256 } = 0,

    /// Progress scheduled tasks (mailbox/waitlist/reservation cleanup).
    ProgressTasks = 1,

    /// Drain message queues within `gas_allowance`; producer emits last.
    ProcessQueues { gas_allowance: u64 } = 2,

    /// User-submitted transaction from the mempool.
    Injected(SignedInjectedTransaction) = 3,
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
        match self {
            Self::AdvanceTillEthereumBlock { .. } => 0,
            Self::ProgressTasks => 1,
            Self::ProcessQueues { .. } => 2,
            Self::Injected(_) => 3,
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
        }
    }
}

/// Ordered list of [`Operation`]s; CAS key = Blake2b-256 of the SCALE-encoded list.
#[derive(
    Clone, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo, Deref, DerefMut, IntoIterator,
)]
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

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_txs() -> Operations {
        Operations::new(alloc::vec![
            Operation::ProgressTasks,
            Operation::ProcessQueues {
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
        let queues = Operation::ProcessQueues {
            gas_allowance: 1234,
        };
        assert!(advance.is_advance_till_ethereum_block());
        assert!(progress.is_progress_tasks());
        assert!(queues.is_process_queues());
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

        // Unknown tag must be rejected by `Decode`, not interpreted.
        use parity_scale_codec::DecodeAll;
        assert!(Operation::decode_all(&mut [4u8, 0, 0, 0].as_slice()).is_err());
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
