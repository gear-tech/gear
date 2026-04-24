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

//! Application-level block shape produced by the Malachite sequencer.
//!
//! A [`SequencerBlock`] is just an ordered list of [`Transaction`]s.
//! The producer decides what sequence to put in; the executor side of
//! ethexe (for now outside this crate) will interpret each transaction
//! when the block has been finalized.

use ethexe_common::injected::SignedInjectedTransaction;
use gprimitives::H256;
use parity_scale_codec::{Decode, Encode};
use serde::{Deserialize, Serialize};
use sha3::{Digest, Keccak256};

/// A single transaction in the sequencer block.
///
/// The enum is deliberately small for MVP — it will grow as the
/// execution side of ethexe gets wired in. Only [`Transaction::Injected`]
/// carries user-supplied data; the rest are service transactions
/// produced by the block producer.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, Serialize, Deserialize)]
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
#[derive(Clone, Debug, Default, PartialEq, Eq, Encode, Decode, Serialize, Deserialize)]
pub struct ProgressTasksLimits {}

/// Placeholder limits for [`Transaction::ProcessQueues`]. Minimum
/// intended payload: a gas allowance.
#[derive(Clone, Debug, Default, PartialEq, Eq, Encode, Decode, Serialize, Deserialize)]
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
/// The block is intentionally lightweight: no Ethereum anchor field,
/// no gas allowance — those are represented as individual
/// [`Transaction::AdvanceTillEthereumBlock`] / [`Transaction::ProcessQueues`]
/// entries inside `transactions`.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, Serialize, Deserialize)]
pub struct SequencerBlock {
    pub transactions: Vec<Transaction>,
}

impl SequencerBlock {
    pub fn new(transactions: Vec<Transaction>) -> Self {
        Self { transactions }
    }

    /// Keccak-256 over the SCALE-encoded block — used as the value id
    /// and as the block hash in [`MalachiteEvent::BlockFinalized`].
    pub fn hash(&self) -> H256 {
        let mut h = Keccak256::new();
        h.update(self.encode());
        H256::from_slice(&h.finalize())
    }
}
