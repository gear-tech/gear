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

use crate::{
    DEFAULT_BLOCK_GAS_LIMIT, HashOf, ToDigest,
    events::BlockEvent,
    injected::{Promise, SignedInjectedTransaction},
};
use alloc::{
    collections::{btree_map::BTreeMap, btree_set::BTreeSet},
    vec::Vec,
};
use gear_core::{ids::prelude::CodeIdExt as _, utils};
use gprimitives::{ActorId, CodeId, H256, MessageId};
use parity_scale_codec::{Decode, Encode};
use sha3::Digest as _;

pub type ProgramStates = BTreeMap<ActorId, StateHashWithQueueSize>;

#[derive(Debug, Clone, Copy, Default, Encode, Decode, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct BlockHeader {
    pub height: u32,
    pub timestamp: u64,
    pub parent_hash: H256,
}

impl BlockHeader {
    pub fn dummy(height: u32) -> Self {
        let mut parent_hash = [0; 32];
        parent_hash[..4].copy_from_slice(&height.to_le_bytes());

        Self {
            height,
            timestamp: height as u64 * 12,
            parent_hash: parent_hash.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockData {
    pub hash: H256,
    pub header: BlockHeader,
    pub events: Vec<BlockEvent>,
}

impl BlockData {
    pub fn to_simple(&self) -> SimpleBlockData {
        SimpleBlockData {
            hash: self.hash,
            header: self.header,
        }
    }
}

#[derive(Debug, derive_more::Display, Copy, Clone, PartialEq, Eq, Encode, Decode, Default)]
#[display("Block(hash: {hash}, height: {}, parent: {}, ts: {})", header.height, header.parent_hash, header.timestamp)]
pub struct SimpleBlockData {
    pub hash: H256,
    pub header: BlockHeader,
}

#[cfg_attr(feature = "serde", derive(Hash))]
#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq, derive_more::Display)]
#[display(
    "Announce(block: {block_hash}, parent: {parent}, gas: {gas_allowance:?}, txs: {injected_transactions:?})"
)]
pub struct Announce {
    pub block_hash: H256,
    pub parent: HashOf<Self>,
    pub gas_allowance: Option<u64>,
    // TODO kuzmindev: remove InjectedTransaction from Announce and store only its hashes.
    // Need to implement `PublicAnnounce` struct which will contain full bodies of injected transactions.
    pub injected_transactions: Vec<SignedInjectedTransaction>,
}

impl Announce {
    pub fn to_hash(&self) -> HashOf<Self> {
        // # Safety because of implementation
        unsafe { HashOf::new(H256(utils::hash(&self.encode()))) }
    }

    pub fn base(block_hash: H256, parent: HashOf<Self>) -> Self {
        Self {
            block_hash,
            parent,
            gas_allowance: None,
            injected_transactions: Vec::new(),
        }
    }

    pub fn with_default_gas(block_hash: H256, parent: HashOf<Self>) -> Self {
        Self {
            block_hash,
            parent,
            gas_allowance: Some(DEFAULT_BLOCK_GAS_LIMIT),
            injected_transactions: Vec::new(),
        }
    }

    pub fn is_base(&self) -> bool {
        self.gas_allowance.is_none() && self.injected_transactions.is_empty()
    }
}

impl ToDigest for Announce {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        hasher.update(self.block_hash);
        hasher.update(self.gas_allowance.encode());
        hasher.update(self.injected_transactions.encode());
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComputedAnnounce {
    pub announce_hash: HashOf<Announce>,
    pub promises: Vec<Promise>,
}

impl ComputedAnnounce {
    pub fn from_announce_hash(announce_hash: HashOf<Announce>) -> Self {
        Self {
            announce_hash,
            promises: Default::default(),
        }
    }

    pub fn merge_promises(&mut self, other: ComputedAnnounce) {
        self.promises.extend(other.promises);
    }
}

#[derive(PartialEq, Eq, Hash, Debug, Clone, Copy, Default, Encode, Decode)]
#[cfg_attr(feature = "std", derive(serde::Serialize))]
pub struct StateHashWithQueueSize {
    pub hash: H256,
    pub canonical_queue_size: u8,
    pub injected_queue_size: u8,
}

impl StateHashWithQueueSize {
    pub fn zero() -> Self {
        Self {
            hash: H256::zero(),
            canonical_queue_size: 0,
            injected_queue_size: 0,
        }
    }
}

#[derive(Debug, Clone, Default, Encode, Decode, PartialEq, Eq)]
pub struct CodeBlobInfo {
    pub timestamp: u64,
    pub tx_hash: H256,
}

#[derive(Clone, PartialEq, Eq, derive_more::Debug)]
pub struct CodeAndIdUnchecked {
    #[debug("{:#x} bytes", code.len())]
    pub code: Vec<u8>,
    pub code_id: CodeId,
}

#[derive(Clone, PartialEq, Eq, derive_more::Debug)]
pub struct CodeAndId {
    #[debug("{:#x} bytes", code.len())]
    code: Vec<u8>,
    code_id: CodeId,
}

impl CodeAndId {
    pub fn new(code: Vec<u8>) -> Self {
        let code_id = CodeId::generate(&code);
        Self { code, code_id }
    }

    pub fn code(&self) -> &[u8] {
        &self.code
    }

    pub fn code_id(&self) -> CodeId {
        self.code_id
    }

    /// Creates a new `CodeAndId` from an unchecked version, asserting that the `code_id` matches the generated one.
    /// # Panics
    ///
    /// If the `code_id` does not match the generated one from the `code`, this function will panic.
    pub fn from_unchecked(code_and_id: CodeAndIdUnchecked) -> Self {
        let CodeAndIdUnchecked { code, code_id } = code_and_id;
        assert_eq!(
            code_id,
            CodeId::generate(&code),
            "CodeId does not match the provided code"
        );
        Self { code, code_id }
    }

    pub fn into_unchecked(self) -> CodeAndIdUnchecked {
        CodeAndIdUnchecked {
            code: self.code,
            code_id: self.code_id,
        }
    }
}

/// GearExe network timelines configuration. Parameters fetched the Router contract.
/// This struct stores in the database, because of using in the multiple places.
///
/// TODO(kuzmindev): `ProtocolTimelines` can store more protocol parameters,
/// for example `max_validators` in election.
#[derive(Debug, Clone, Default, Copy, PartialEq, Eq, Encode, Decode)]
pub struct ProtocolTimelines {
    // The genesis timestamp of the GearExe network in seconds.
    pub genesis_ts: u64,
    // The duration of an era in seconds.
    pub era: u64,
    /// The election duration in seconds before the end of an era when the next set of validators elected.
    ///  (start of era)[ - - - - - - - - - - -  + - - - - ] (end of era)
    ///                                         ^ election
    pub election: u64,
    /// The slot duration in seconds.
    pub slot: u64,
}

impl ProtocolTimelines {
    /// Returns the era index for the given timestamp. Eras starts from 0.
    /// If given `ts` less than `genesis_ts` function returns `0`;
    #[inline(always)]
    pub fn era_from_ts(&self, ts: u64) -> u64 {
        ts.checked_sub(self.genesis_ts)
            .expect("timestamp must be >= genesis_ts")
            / self.era
    }

    /// Returns the timestamp since which the given era started.
    #[inline(always)]
    pub fn era_start_ts(&self, era_index: u64) -> u64 {
        self.genesis_ts + era_index * self.era
    }

    /// Returns the timestamp when election starts in the given era.
    /// NOTE: election starts for the next era validators.
    #[inline(always)]
    pub fn era_election_start_ts(&self, era_index: u64) -> u64 {
        self.era_start_ts(era_index + 1) - self.election
    }
}

/// RemoveFromMailbox key; (msgs sources program (mailbox and queue provider), destination user id)
pub type Rfm = (ActorId, ActorId);

/// SendDispatch key; (msgs destinations program (stash and queue provider), message id)
pub type Sd = (ActorId, MessageId);

/// SendUserMessage key; (msgs sources program (mailbox and stash provider))
pub type Sum = ActorId;

/// NOTE: generic keys differs to Vara and have been chosen dependent on storage organization of ethexe.
pub type ScheduledTask = gear_core::tasks::ScheduledTask<Rfm, Sd, Sum>;

/// Scheduler; (block height, scheduled task)
pub type Schedule = BTreeMap<u32, BTreeSet<ScheduledTask>>;

#[cfg(test)]
mod tests {
    use super::ProtocolTimelines;

    #[test]
    fn test_era_from_ts_calculation() {
        let timelines = ProtocolTimelines {
            genesis_ts: 10,
            era: 234,
            election: 200,
            slot: 10,
        };

        // For 0 era
        assert_eq!(timelines.era_from_ts(10), 0);
        assert_eq!(timelines.era_from_ts(45), 0);
        assert_eq!(timelines.era_from_ts(243), 0);

        // For 1 era
        assert_eq!(timelines.era_from_ts(244), 1);
        assert_eq!(timelines.era_from_ts(333), 1);
    }

    #[should_panic(expected = "timestamp must be >= genesis_ts")]
    #[test]
    fn panic_on_era_from_ts_before_genesis() {
        ProtocolTimelines {
            genesis_ts: 100,
            era: 234,
            election: 200,
            slot: 10,
        }
        .era_from_ts(50);
    }

    #[test]
    fn test_era_start_calculation() {
        let timelines = ProtocolTimelines {
            genesis_ts: 10,
            era: 234,
            election: 200,
            slot: 10,
        };

        // For 0 era
        assert_eq!(timelines.era_start_ts(0), 10);
        assert_eq!(timelines.era_start_ts(0), 10);
        assert_eq!(timelines.era_start_ts(0), 10);

        // For 1 era
        assert_eq!(timelines.era_start_ts(1), 244);
        assert_eq!(timelines.era_start_ts(1), 244);
    }
}
