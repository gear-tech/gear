// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::events::BlockEvent;
use alloc::{
    collections::{btree_map::BTreeMap, btree_set::BTreeSet},
    vec::Vec,
};
use core::num::NonZeroU64;
use gear_core::ids::prelude::CodeIdExt as _;
use gprimitives::{ActorId, CodeId, H256, MessageId};
use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;

pub type ProgramStates = BTreeMap<ActorId, StateHashWithQueueSize>;

#[derive(Debug, Clone, Copy, Default, Encode, Decode, TypeInfo, PartialEq, Eq, Hash)]
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

#[derive(
    Debug, derive_more::Display, Copy, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Default,
)]
#[display("Block(hash: {hash}, height: {}, parent: {}, ts: {})", header.height, header.parent_hash, header.timestamp)]
pub struct SimpleBlockData {
    pub hash: H256,
    pub header: BlockHeader,
}

/// [`PromisePolicy`] tells processor whether should it emits promises or not.
#[derive(Clone, Debug, Copy, Default, PartialEq, Eq, Encode, Decode, derive_more::IsVariant)]
pub enum PromisePolicy {
    /// Emits promises in execution process.
    Enabled,
    // Do not emit promises in execution process.
    #[default]
    Disabled,
}

/// The [PromiseEmissionMode] configures the promise emission mode for the ethexe node
#[derive(Debug, Copy, Clone, PartialEq, Eq, derive_more::IsVariant, Default)]
pub enum PromiseEmissionMode {
    /// Node should always emit promises during MB execution.
    /// Always set [`PromisePolicy::Enabled`].
    AlwaysEmit,
    /// [`PromisePolicy`] is decided per-MB by the consensus / compute layer.
    #[default]
    ConsensusDriven,
}

#[derive(PartialEq, Eq, Hash, Debug, Clone, Copy, Default, Encode, Decode, TypeInfo)]
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

#[derive(Debug, Clone, Default, Encode, Decode, TypeInfo, PartialEq, Eq)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct ProtocolTimelines {
    // The genesis timestamp of the GearExe network in seconds.
    pub genesis_ts: u64,
    // The duration of an era in seconds.
    pub era: NonZeroU64,
    /// The election duration in seconds before the end of an era when the next set of validators elected.
    ///  (start of era)[ - - - - - - - - - - - - + - - - - ] (end of era)
    ///                                          ^ election
    pub election: u64,
    /// The slot duration in seconds.
    pub slot: NonZeroU64,
}

impl ProtocolTimelines {
    /// Returns the era index for the given timestamp. Eras starts from 0.
    ///
    /// Returns `None` if `ts < genesis_ts`
    #[inline(always)]
    pub fn era_from_ts(&self, ts: u64) -> Option<u64> {
        ts.checked_sub(self.genesis_ts)
            .map(|delta| delta / self.era.get())
    }

    /// Returns the timestamp since which the given era started.
    ///
    /// Returns `None` if overflows u64.
    #[inline(always)]
    pub fn era_start_ts(&self, era_index: u64) -> Option<u64> {
        era_index
            .checked_mul(self.era.get())?
            .checked_add(self.genesis_ts)
    }

    /// Returns the timestamp when election starts in the given era.
    /// NOTE: election starts for the next era validators.
    ///
    /// Returns `None` if overflows u64.
    ///
    /// # Panics
    /// Panics if `era duration < election duration`
    #[inline(always)]
    pub fn era_election_start_ts(&self, era_index: u64) -> Option<u64> {
        self.era_start_ts(era_index)?.checked_add(
            self.era
                .get()
                .checked_sub(self.election)
                .expect("Incorrect Timelines - era duration < election duration"),
        )
    }

    /// Returns the slot index for the given timestamp. Slots starts from 0.
    ///
    /// Returns `None` if `ts < genesis_ts`
    #[inline(always)]
    pub fn slot_from_ts(&self, ts: u64) -> Option<u64> {
        ts.checked_sub(self.genesis_ts)
            .map(|delta| delta / self.slot.get())
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
    use super::*;

    fn mock_timelines() -> ProtocolTimelines {
        ProtocolTimelines {
            genesis_ts: 10,
            era: NonZeroU64::new(234).unwrap(),
            election: 200,
            slot: NonZeroU64::new(10).unwrap(),
        }
    }

    #[test]
    fn test_era_from_ts_calculation() {
        let timelines = mock_timelines();

        // For 0 era
        assert_eq!(timelines.era_from_ts(10), Some(0));
        assert_eq!(timelines.era_from_ts(45), Some(0));
        assert_eq!(timelines.era_from_ts(243), Some(0));

        // For 1 era
        assert_eq!(timelines.era_from_ts(244), Some(1));
        assert_eq!(timelines.era_from_ts(333), Some(1));
    }

    #[test]
    fn era_from_ts_returns_none_before_genesis() {
        let result = ProtocolTimelines {
            genesis_ts: 100,
            ..mock_timelines()
        }
        .era_from_ts(50);
        assert_eq!(result, None);
    }

    #[test]
    fn test_era_start_calculation() {
        let timelines = mock_timelines();

        // For 0 era
        assert_eq!(timelines.era_start_ts(0), Some(10));
        assert_eq!(timelines.era_start_ts(0), Some(10));
        assert_eq!(timelines.era_start_ts(0), Some(10));

        // For 1 era
        assert_eq!(timelines.era_start_ts(1), Some(244));
        assert_eq!(timelines.era_start_ts(1), Some(244));
    }
}
