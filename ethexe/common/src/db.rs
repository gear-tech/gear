// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
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

//! ethexe common db types and traits.

// TODO (gsobol): move types to another module(s)

use crate::{events::BlockEvent, gear::StateTransition};
use alloc::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    vec::Vec,
};
use gear_core::{
    code::InstrumentedCode,
    ids::{ActorId, CodeId, ProgramId},
};
use gprimitives::{MessageId, H256};
use parity_scale_codec::{Decode, Encode};

/// RemoveFromMailbox key; (msgs sources program (mailbox and queue provider), destination user id)
pub type Rfm = (ProgramId, ActorId);

/// SendDispatch key; (msgs destinations program (stash and queue provider), message id)
pub type Sd = (ProgramId, MessageId);

/// SendUserMessage key; (msgs sources program (mailbox and stash provider))
pub type Sum = ProgramId;

/// NOTE: generic keys differs to Vara and have been chosen dependent on storage organization of ethexe.
pub type ScheduledTask = gear_core::tasks::ScheduledTask<Rfm, Sd, Sum>;

#[derive(Debug, Clone, Default, Encode, Decode, PartialEq, Eq)]
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

#[derive(Debug, Clone, Default, Encode, Decode, PartialEq, Eq)]
pub struct CodeInfo {
    pub timestamp: u64,
    pub tx_hash: H256,
}

pub type Schedule = BTreeMap<u32, BTreeSet<ScheduledTask>>;

pub trait BlockMetaStorage: Send + Sync {
    fn block_computed(&self, block_hash: H256) -> bool;
    fn set_block_computed(&self, block_hash: H256);

    fn block_commitment_queue(&self, block_hash: H256) -> Option<VecDeque<H256>>;
    fn set_block_commitment_queue(&self, block_hash: H256, queue: VecDeque<H256>);

    fn block_codes_queue(&self, block_hash: H256) -> Option<VecDeque<CodeId>>;
    fn set_block_codes_queue(&self, block_hash: H256, queue: VecDeque<CodeId>);

    fn previous_not_empty_block(&self, block_hash: H256) -> Option<H256>;
    fn set_previous_not_empty_block(&self, block_hash: H256, prev_commitment: H256);

    fn block_program_states(&self, block_hash: H256) -> Option<BTreeMap<ActorId, H256>>;
    fn set_block_program_states(&self, block_hash: H256, map: BTreeMap<ActorId, H256>);

    fn block_outcome(&self, block_hash: H256) -> Option<Vec<StateTransition>>;
    fn set_block_outcome(&self, block_hash: H256, outcome: Vec<StateTransition>);
    fn block_outcome_is_empty(&self, block_hash: H256) -> Option<bool> {
        self.block_outcome(block_hash)
            .map(|outcome| outcome.is_empty())
    }

    fn block_schedule(&self, block_hash: H256) -> Option<Schedule>;
    fn set_block_schedule(&self, block_hash: H256, map: Schedule);

    fn latest_computed_block(&self) -> Option<(H256, BlockHeader)>;
    fn set_latest_computed_block(&self, block_hash: H256, header: BlockHeader);
}

pub trait CodesStorage: Send + Sync {
    fn original_code_exists(&self, code_id: CodeId) -> bool;

    fn original_code(&self, code_id: CodeId) -> Option<Vec<u8>>;
    fn set_original_code(&self, code: &[u8]) -> CodeId;

    fn program_code_id(&self, program_id: ProgramId) -> Option<CodeId>;
    fn set_program_code_id(&self, program_id: ProgramId, code_id: CodeId);
    fn program_ids(&self) -> BTreeSet<ProgramId>;

    fn instrumented_code(&self, runtime_id: u32, code_id: CodeId) -> Option<InstrumentedCode>;
    fn set_instrumented_code(&self, runtime_id: u32, code_id: CodeId, code: InstrumentedCode);

    fn code_valid(&self, code_id: CodeId) -> Option<bool>;
    fn set_code_valid(&self, code_id: CodeId, valid: bool);
}

pub trait OnChainStorage: Send + Sync {
    fn block_header(&self, block_hash: H256) -> Option<BlockHeader>;
    fn set_block_header(&self, block_hash: H256, header: BlockHeader);

    fn block_events(&self, block_hash: H256) -> Option<Vec<BlockEvent>>;
    fn set_block_events(&self, block_hash: H256, events: &[BlockEvent]);

    fn code_blob_info(&self, code_id: CodeId) -> Option<CodeInfo>;
    fn set_code_blob_info(&self, code_id: CodeId, code_info: CodeInfo);

    fn block_is_synced(&self, block_hash: H256) -> bool;
    fn set_block_is_synced(&self, block_hash: H256);

    fn latest_synced_block_height(&self) -> Option<u32>;
    fn set_latest_synced_block_height(&self, height: u32);
}
