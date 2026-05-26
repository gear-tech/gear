// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Application-level block shape produced by the Malachite sequencer
//! and consumed by the ethexe executor.
//!
//! [`Transactions`] is the application's `BlockPayload` — an ordered
//! list of [`Transaction`]s. Block-level identity (parent linkage,
//! height) lives in [`crate::db::CompactMb`], indexed by the
//! `ethexe_malachite_core::Block` envelope hash. The transaction list
//! itself is stored in the content-addressed half of the ethexe db
//! and referenced by `CompactMb::transactions_hash`.
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
}
