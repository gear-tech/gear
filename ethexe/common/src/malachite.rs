// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Application-level block shape produced by the Malachite sequencer
//! and consumed by the ethexe executor.
//!
//! Two layers live here:
//!
//! - [`BlockPayload`] is the opaque, versioned, size-capped wire
//!   envelope the consensus engine ships around. The application
//!   schema — [`Transactions`] (an ordered list of [`Transaction`]s) —
//!   lives SCALE-encoded inside [`BlockPayload::bytes`].
//! - Block-level identity (parent linkage, height) lives in
//!   [`crate::db::CompactMb`], indexed by the consensus block envelope
//!   hash. The matching [`BlockPayload`] / [`Transactions`] blob is
//!   stored in the content-addressed half of the ethexe db and
//!   referenced by `CompactMb::transactions_hash`.
//!
//! These types live in `ethexe-common` (rather than inside
//! `ethexe-malachite`) so `ethexe-processor` can accept them without
//! depending on the consensus layer.

use crate::injected::SignedInjectedTransaction;
use alloc::vec::Vec;
use anyhow::{Result, anyhow};
use derive_more::{Deref, DerefMut, IntoIterator};
use gear_core::limited::LimitedVec;
use gprimitives::H256;
use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;

#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};

/// Per-block payload size cap: 1000 KiB, leaving headroom under the
/// 1 MiB engine block ceiling for the consensus block envelope
/// (parent hash, height, reserved tail) and SCALE framing.
pub const MAX_BLOCK_PAYLOAD_BYTES: usize = 1024 * 1000;

/// Current `BlockPayload::version` written by this code path.
///
/// Bump in lockstep with a wire-format change in how the application
/// interprets [`BlockPayload::bytes`]; decoders MUST tolerate seeing
/// versions strictly less than the current one but MAY reject newer
/// ones.
pub const BLOCK_PAYLOAD_VERSION: u16 = 0;

/// Versioned, size-capped block payload.
///
/// The consensus engine treats `bytes` as an opaque byte string —
/// the application crate is responsible for the schema (today, a
/// SCALE-encoded [`Transactions`]). `version` exists so a future
/// protocol bump can change the `bytes` encoding without breaking the
/// consensus block wire shape: decoders inspect `version` and
/// dispatch accordingly.
#[derive(Clone, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct BlockPayload {
    pub version: u16,
    pub bytes: LimitedVec<u8, MAX_BLOCK_PAYLOAD_BYTES>,
}

impl BlockPayload {
    /// Wrap raw application bytes at the current
    /// [`BLOCK_PAYLOAD_VERSION`]. Returns `Err` if `bytes` exceeds
    /// [`MAX_BLOCK_PAYLOAD_BYTES`].
    pub fn new(bytes: Vec<u8>) -> Result<Self> {
        let len = bytes.len();
        let bytes = LimitedVec::try_from(bytes).map_err(|_| {
            anyhow!("block payload exceeds {MAX_BLOCK_PAYLOAD_BYTES}-byte cap (got {len})")
        })?;
        Ok(Self {
            version: BLOCK_PAYLOAD_VERSION,
            bytes,
        })
    }
}

/// A single transaction in the malachite block.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub enum Transaction {
    /// Pin executor's view to a quarantine-passed Ethereum block.
    AdvanceTillEthereumBlock { block_hash: H256 },

    /// Progress scheduled tasks (mailbox/waitlist/reservation cleanup).
    ProgressTasks { limits: ProgressTasksLimits },

    /// Drain message queues within `gas_allowance`; producer emits last.
    ProcessQueues { limits: ProcessQueuesLimits },

    /// User-submitted transaction from the mempool.
    Injected(SignedInjectedTransaction),
}

/// Placeholder; shape firms up once executor plumbing lands.
#[derive(Clone, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct ProgressTasksLimits {}

/// Per-MB execution budget, carried on the wire.
#[derive(Clone, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct ProcessQueuesLimits {
    pub gas_allowance: u64,
}

impl Transaction {
    /// Short human-readable tag, used in logs and debug dumps.
    pub fn tag(&self) -> &'static str {
        match self {
            Self::AdvanceTillEthereumBlock { .. } => "advance-eth-block",
            Self::ProgressTasks { .. } => "progress-tasks",
            Self::ProcessQueues { .. } => "process-queues",
            Self::Injected(_) => "injected",
        }
    }
}

/// `BlockPayload`: ordered transactions; CAS key = Blake2b-256 of the SCALE-encoded list.
#[derive(
    Clone, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo, Deref, DerefMut, IntoIterator,
)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct Transactions(pub Vec<Transaction>);

impl Transactions {
    pub fn new(transactions: Vec<Transaction>) -> Self {
        Self(transactions)
    }

    /// CAS key: Blake2b-256 over the SCALE-encoded list.
    pub fn hash(&self) -> H256 {
        gear_core::utils::hash(&self.encode()).into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_txs() -> Transactions {
        Transactions::new(alloc::vec![
            Transaction::ProgressTasks {
                limits: ProgressTasksLimits::default(),
            },
            Transaction::ProcessQueues {
                limits: ProcessQueuesLimits::default(),
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
    fn hash_changes_when_transactions_change() {
        let mut a = empty_txs();
        let b = empty_txs();
        a.push(Transaction::AdvanceTillEthereumBlock {
            block_hash: H256::from_low_u64_be(0xEB),
        });
        assert_ne!(a.hash(), b.hash());
    }

    #[test]
    fn transaction_tag_distinguishes_variants() {
        let advance = Transaction::AdvanceTillEthereumBlock {
            block_hash: H256::zero(),
        };
        let progress = Transaction::ProgressTasks {
            limits: ProgressTasksLimits::default(),
        };
        let queues = Transaction::ProcessQueues {
            limits: ProcessQueuesLimits::default(),
        };
        assert_eq!(advance.tag(), "advance-eth-block");
        assert_eq!(progress.tag(), "progress-tasks");
        assert_eq!(queues.tag(), "process-queues");
    }

    #[test]
    fn scale_round_trip_preserves_hash() {
        // `Transactions` is SCALE-encoded for both the CAS payload
        // and the consensus wire payload — make sure round-trip is
        // hash-preserving so peers and the executor agree on the
        // CAS key.
        use parity_scale_codec::Decode;

        let original = Transactions::new(alloc::vec![Transaction::AdvanceTillEthereumBlock {
            block_hash: H256::from_low_u64_be(0xEB)
        }]);
        let encoded = original.encode();
        let decoded = Transactions::decode(&mut encoded.as_slice()).expect("decode");
        assert_eq!(original, decoded);
        assert_eq!(original.hash(), decoded.hash());
    }

    #[test]
    fn block_payload_new_accepts_at_or_below_cap() {
        BlockPayload::new(alloc::vec![]).expect("empty payload");
        BlockPayload::new(alloc::vec![0u8; MAX_BLOCK_PAYLOAD_BYTES]).expect("payload at cap");
    }

    #[test]
    fn block_payload_new_rejects_above_cap() {
        let err = BlockPayload::new(alloc::vec![0u8; MAX_BLOCK_PAYLOAD_BYTES + 1])
            .expect_err("over-cap must reject");
        assert!(
            err.to_string()
                .contains(&MAX_BLOCK_PAYLOAD_BYTES.to_string()),
            "expected cap-sized error mention, got: {err}",
        );
    }

    #[test]
    fn block_payload_decode_rejects_oversized_bytes_field() {
        // Hand-roll an encoded `BlockPayload` whose `bytes` length
        // exceeds the cap. SCALE prefixes `Vec<u8>` with a `Compact<u32>`
        // length; we use the 4-byte mode for clarity. Decode must reject
        // before allocating the over-cap buffer.
        use parity_scale_codec::DecodeAll;

        let oversize = (MAX_BLOCK_PAYLOAD_BYTES + 1) as u32;
        let mut encoded = alloc::vec::Vec::new();
        encoded.extend_from_slice(&BLOCK_PAYLOAD_VERSION.encode());
        encoded.extend_from_slice(&parity_scale_codec::Compact(oversize).encode());
        encoded.extend(core::iter::repeat_n(0u8, oversize as usize));
        BlockPayload::decode_all(&mut encoded.as_slice())
            .expect_err("decode must reject over-cap payload");
    }
}
