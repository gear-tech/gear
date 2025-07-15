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
    events::BlockEvent, gear::StateTransition, AnnounceHash, BlockHeader, CodeBlobInfo, Digest,
    ProducerBlock, ProgramStates, Schedule,
};
use gear_core::{
    code::{CodeMetadata, InstrumentedCode},
    ids::{ActorId, CodeId},
};
use gprimitives::H256;
use parity_scale_codec::{Decode, Encode};

#[derive(Clone, Debug, Default, Encode, Decode, PartialEq, Eq)]
pub struct BlockMeta {
    pub prepared: bool,
    pub announces: Option<Vec<AnnounceHash>>,
    pub codes_queue: Option<VecDeque<CodeId>>,
    pub last_committed_batch: Option<Digest>,
}

impl BlockMeta {
    pub fn default_prepared() -> Self {
        Self {
            prepared: true,
            announces: Some(Default::default()),
            codes_queue: Some(Default::default()),
            last_committed_batch: Some(Default::default()),
        }
    }
}

pub trait BlockMetaStorageRead {
    /// NOTE: if `BlockMeta` doesn't exist in the database, it will return the default value.
    fn block_meta(&self, block_hash: H256) -> BlockMeta;
}

pub trait BlockMetaStorageWrite {
    /// NOTE: if `BlockMeta` doesn't exist in the database,
    /// it will be created with default values and then will be mutated.
    fn mutate_block_meta(&self, block_hash: H256, f: impl FnOnce(&mut BlockMeta));
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
    fn block_synced(&self, block_hash: H256) -> bool;
}

pub trait OnChainStorageWrite {
    fn set_block_header(&self, block_hash: H256, header: BlockHeader);
    fn set_block_events(&self, block_hash: H256, events: &[BlockEvent]);
    fn set_code_blob_info(&self, code_id: CodeId, code_info: CodeBlobInfo);
    fn set_block_synced(&self, block_hash: H256);
}

#[derive(Debug, Clone, Default, Encode, Decode, PartialEq, Eq)]
pub struct AnnounceMeta {
    pub computed: bool,
    pub announces_queue: Option<VecDeque<Option<AnnounceHash>>>,
}

pub trait AnnounceStorageRead {
    fn announce(&self, hash: AnnounceHash) -> Option<ProducerBlock>;
    fn announce_program_states(&self, announce_hash: AnnounceHash) -> Option<ProgramStates>;
    fn announce_outcome(&self, announce_hash: AnnounceHash) -> Option<Vec<StateTransition>>;
    fn announce_schedule(&self, announce_hash: AnnounceHash) -> Option<Schedule>;
    fn announce_meta(&self, announce_hash: AnnounceHash) -> AnnounceMeta;
}

pub trait AnnounceStorageWrite {
    fn set_announce(&self, announce: ProducerBlock);
    fn set_announce_program_states(
        &self,
        announce_hash: AnnounceHash,
        program_states: ProgramStates,
    );
    fn set_announce_outcome(&self, announce_hash: AnnounceHash, outcome: Vec<StateTransition>);
    fn set_announce_schedule(&self, announce_hash: AnnounceHash, schedule: Schedule);
    fn mutate_announce_meta(&self, announce_hash: AnnounceHash, f: impl FnOnce(&mut AnnounceMeta));
}

#[derive(Debug, Clone, Default, Encode, Decode, PartialEq, Eq)]
pub struct LatestData {
    pub latest_synced_block_height: Option<u32>,
    pub latest_prepared_block_hash: Option<H256>,
    pub latest_computed_announce_hash: Option<AnnounceHash>,
}

pub trait LatestDataStorageRead {
    fn latest_data(&self) -> LatestData;
}
pub trait LatestDataStorageWrite {
    fn mutate_latest_data(&self, f: impl FnOnce(&mut LatestData));
}
