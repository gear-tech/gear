// This file is part of Gear.
//
// Copyright (C) 2026 Gear Technologies Inc.
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

//! Application-level block shape produced by the Malachite sequencer
//! and consumed by the ethexe executor.
//!
//! A [`SequencerBlock`] is just an ordered list of [`Transaction`]s.
//! The producer (Malachite) decides the sequence; the executor
//! (ethexe-processor) interprets each transaction in order. The types
//! live here (rather than inside `ethexe-malachite`) so that
//! `ethexe-processor` can accept them without depending on the
//! consensus layer.

use crate::injected::SignedInjectedTransaction;
use alloc::vec::Vec;
use gprimitives::H256;
use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;
use sha3::{Digest, Keccak256};

#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};

/// A single transaction in the sequencer block.
///
/// The enum is deliberately small for MVP — it will grow as the
/// execution side of ethexe gets wired in. Only [`Transaction::Injected`]
/// carries user-supplied data; the rest are service transactions
/// produced by the block producer.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub enum Transaction {
    /// Advance the executor's view of the canonical Ethereum chain up
    /// to (and including) the block at `eth_block_hash`. Producer picks
    /// the block that has just passed the ethexe quarantine window.
    AdvanceTillEthereumBlock { eth_block_hash: H256 },

    /// Progress any pending scheduled tasks (mailbox expiry, waitlist
    /// wake-ups, reservation cleanups, etc.) subject to `limits`.
    ///
    /// `limits` is intentionally left empty for now — concrete
    /// parameters (time / gas budget) will be filled in later.
    ProgressTasks { limits: ProgressTasksLimits },

    /// Run one drain of the message queues subject to `limits`
    /// (minimum: `gas_allowance`). Producer emits this at the very
    /// end of each sequencer block.
    ProcessQueues { limits: ProcessQueuesLimits },

    /// A user-submitted transaction picked from the mempool.
    Injected(SignedInjectedTransaction),
}

/// Placeholder limits for [`Transaction::ProgressTasks`] — shape will
/// be nailed down once executor-side plumbing lands.
#[derive(Clone, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct ProgressTasksLimits {}

/// Placeholder limits for [`Transaction::ProcessQueues`]. Minimum
/// intended payload: a gas allowance.
#[derive(Clone, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct ProcessQueuesLimits {}

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

/// A block of sequencer transactions produced by BFT consensus.
///
/// Self-contained: `parent` is the hash of the previous finalized
/// sequencer block (zero for the genesis MB at height 1), so a peer
/// or executor can verify chain continuity without consulting an
/// external index. The producer fills it in from its
/// `latest_finalized_mb_hash`; validators reject proposals whose
/// `parent` doesn't match their own.
///
/// Tendermint commits exactly one block per height in order, so
/// `parent` is fully determined by `height-1` — the field is here for
/// self-containment and forward-compat with non-linear BFT, not for
/// safety beyond what consensus itself already provides.
///
/// Other than `parent`, the block is intentionally lightweight: no
/// Ethereum anchor field, no gas allowance — those are represented
/// as individual [`Transaction::AdvanceTillEthereumBlock`] /
/// [`Transaction::ProcessQueues`] entries inside `transactions`.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct SequencerBlock {
    /// Hash of the previous finalized [`SequencerBlock`] (`H256::zero()`
    /// for the genesis MB at height 1).
    pub parent: H256,
    pub transactions: Vec<Transaction>,
}

impl SequencerBlock {
    pub fn new(parent: H256, transactions: Vec<Transaction>) -> Self {
        Self {
            parent,
            transactions,
        }
    }

    /// Keccak-256 over the SCALE-encoded block — used as the value id
    /// in Malachite consensus and as the MB hash that keys
    /// post-execution state in the ethexe DB.
    pub fn hash(&self) -> H256 {
        let mut h = Keccak256::new();
        h.update(self.encode());
        H256::from_slice(&h.finalize())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_block(parent: H256) -> SequencerBlock {
        SequencerBlock::new(
            parent,
            alloc::vec![
                Transaction::ProgressTasks {
                    limits: ProgressTasksLimits::default(),
                },
                Transaction::ProcessQueues {
                    limits: ProcessQueuesLimits::default(),
                },
            ],
        )
    }

    #[test]
    fn hash_is_deterministic_for_same_content() {
        let p = H256::from_low_u64_be(1);
        let a = empty_block(p);
        let b = empty_block(p);
        assert_eq!(a.hash(), b.hash());
    }

    #[test]
    fn hash_changes_when_parent_changes() {
        // Two blocks with identical transaction lists but different
        // parents must hash to different values — this is what gives
        // the chain structural integrity (a peer can't substitute a
        // sibling block's parent without changing the hash).
        let a = empty_block(H256::from_low_u64_be(1));
        let b = empty_block(H256::from_low_u64_be(2));
        assert_ne!(a.hash(), b.hash());
    }

    #[test]
    fn hash_changes_when_transactions_change() {
        let parent = H256::from_low_u64_be(1);
        let mut a = empty_block(parent);
        let b = empty_block(parent);
        a.transactions.push(Transaction::AdvanceTillEthereumBlock {
            eth_block_hash: H256::from_low_u64_be(0xEB),
        });
        assert_ne!(a.hash(), b.hash());
    }

    #[test]
    fn transaction_tag_distinguishes_variants() {
        // `tag()` is used in logs all over app.rs / service.rs;
        // pin its mapping so renames are caught.
        let advance = Transaction::AdvanceTillEthereumBlock {
            eth_block_hash: H256::zero(),
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
        // SequencerBlock travels over JSON for malachite codec and
        // SCALE for DB storage — make sure SCALE round-trip is
        // hash-preserving (we identify decoded blocks by hash on the
        // executor side).
        use parity_scale_codec::Decode;

        let original = SequencerBlock::new(
            H256::from_low_u64_be(0x42),
            alloc::vec![Transaction::AdvanceTillEthereumBlock {
                eth_block_hash: H256::from_low_u64_be(0xEB)
            }],
        );
        let encoded = original.encode();
        let decoded = SequencerBlock::decode(&mut encoded.as_slice()).expect("decode");
        assert_eq!(original, decoded);
        assert_eq!(original.hash(), decoded.hash());
    }
}
