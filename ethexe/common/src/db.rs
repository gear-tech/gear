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

// TODO #4547: move types to another module(s)

use crate::{
    BlockHeader, BlockMeta, CodeBlobInfo, Digest, ProgramStates, Schedule, events::BlockEvent,
    gear::StateTransition,
};
use alloc::{
    collections::{BTreeSet, VecDeque},
    vec::Vec,
};
use gear_core::{
    code::{CodeMetadata, InstrumentedCode},
    ids::{ActorId, CodeId},
};
use gprimitives::H256;
use parity_scale_codec::{Decode, Encode};

#[derive(Debug, Clone, Eq, PartialEq, Encode, Decode, derive_more::From, derive_more::Unwrap)]
pub enum BlockOutcome {
    Transitions(Vec<StateTransition>),
    /// The actual outcome is not available, but it must be considered non-empty.
    ForcedNonEmpty,
}

impl BlockOutcome {
    pub fn is_empty(&self) -> bool {
        match self {
            BlockOutcome::Transitions(transitions) => transitions.is_empty(),
            BlockOutcome::ForcedNonEmpty => false,
        }
    }

    pub fn into_transitions(self) -> Option<Vec<StateTransition>> {
        match self {
            BlockOutcome::Transitions(transitions) => Some(transitions),
            BlockOutcome::ForcedNonEmpty => None,
        }
    }
}

pub trait BlockMetaStorageRead {
    /// NOTE: if `BlockMeta` doesn't exist in the database, it will return the default value.
    fn block_meta(&self, block_hash: H256) -> BlockMeta;

    fn block_commitment_queue(&self, block_hash: H256) -> Option<VecDeque<H256>>;
    fn block_codes_queue(&self, block_hash: H256) -> Option<VecDeque<CodeId>>;
    fn previous_non_empty_block(&self, block_hash: H256) -> Option<H256>;
    fn last_committed_batch(&self, block_hash: H256) -> Option<Digest>;
    fn block_program_states(&self, block_hash: H256) -> Option<ProgramStates>;
    fn block_outcome(&self, block_hash: H256) -> Option<BlockOutcome>;
    fn block_schedule(&self, block_hash: H256) -> Option<Schedule>;
    fn latest_computed_block(&self) -> Option<(H256, BlockHeader)>;
}

pub trait BlockMetaStorageWrite {
    /// NOTE: if `BlockMeta` doesn't exist in the database,
    /// it will be created with default values and then will be mutated.
    fn mutate_block_meta<F>(&self, block_hash: H256, f: F)
    where
        F: FnOnce(&mut BlockMeta);

    fn set_block_commitment_queue(&self, block_hash: H256, queue: VecDeque<H256>);
    fn set_block_codes_queue(&self, block_hash: H256, queue: VecDeque<CodeId>);
    fn set_previous_not_empty_block(&self, block_hash: H256, prev_commitment: H256);
    fn set_last_committed_batch(&self, block_hash: H256, batch: Digest);
    fn set_block_program_states(&self, block_hash: H256, map: ProgramStates);
    fn set_block_outcome(&self, block_hash: H256, outcome: Vec<StateTransition>);
    fn set_block_schedule(&self, block_hash: H256, map: Schedule);
    fn set_latest_computed_block(&self, block_hash: H256, header: BlockHeader);
}

pub trait CodesStorageRead {
    fn original_code_exists(&self, code_id: CodeId) -> bool;
    fn original_code(&self, code_id: CodeId) -> Option<Vec<u8>>;
    fn program_code_id(&self, program_id: ActorId) -> Option<CodeId>;
    fn instrumented_code_exists(&self, runtime_id: u32, code_id: CodeId) -> bool;
    fn instrumented_code(&self, runtime_id: u32, code_id: CodeId) -> Option<InstrumentedCode>;
    fn code_metadata(&self, code_id: CodeId) -> Option<CodeMetadata>;
    fn code_valid(&self, code_id: CodeId) -> Option<bool>;
}

pub trait CodesStorageWrite {
    fn set_original_code(&self, code: &[u8]) -> CodeId;
    fn set_program_code_id(&self, program_id: ActorId, code_id: CodeId);
    fn set_instrumented_code(&self, runtime_id: u32, code_id: CodeId, code: InstrumentedCode);
    fn set_code_metadata(&self, code_id: CodeId, code_metadata: CodeMetadata);
    fn set_code_valid(&self, code_id: CodeId, valid: bool);
    fn valid_codes(&self) -> BTreeSet<CodeId>;
}

pub trait OnChainStorageRead {
    fn block_header(&self, block_hash: H256) -> Option<BlockHeader>;
    fn block_events(&self, block_hash: H256) -> Option<Vec<BlockEvent>>;
    fn code_blob_info(&self, code_id: CodeId) -> Option<CodeBlobInfo>;
    fn latest_synced_block_height(&self) -> Option<u32>;
}

pub trait OnChainStorageWrite {
    fn set_block_header(&self, block_hash: H256, header: BlockHeader);
    fn set_block_events(&self, block_hash: H256, events: &[BlockEvent]);
    fn set_code_blob_info(&self, code_id: CodeId, code_info: CodeBlobInfo);
    fn set_latest_synced_block_height(&self, height: u32);
}
