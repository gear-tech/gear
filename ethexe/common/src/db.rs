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

//! Common db types and traits.

use crate::{
    Announce, BlockHeader, CodeBlobInfo, Digest, HashOf, ProgramStates, ProtocolTimelines,
    Schedule, SimpleBlockData, ValidatorsVec,
    events::BlockEvent,
    gear::StateTransition,
    injected::{InjectedTransaction, SignedInjectedTransaction},
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

/// Ethexe metadata associated with an on-chain block.
#[derive(Clone, Debug, Default, Encode, Decode, PartialEq, Eq, Hash)]
pub struct BlockMeta {
    /// Block has been prepared, meaning:
    /// all metadata is ready, all predecessors till start block are prepared too.
    pub prepared: bool,
    // TODO: #4945 remove announces from here
    /// Set of announces included in the block.
    pub announces: Option<BTreeSet<HashOf<Announce>>>,
    /// Queue of code ids waiting for validation status commitment on-chain.
    pub codes_queue: Option<VecDeque<CodeId>>,
    /// Last committed on-chain batch hash.
    pub last_committed_batch: Option<Digest>,
    /// Last committed on-chain announce hash.
    pub last_committed_announce: Option<HashOf<Announce>>,
}

impl BlockMeta {
    pub fn default_prepared() -> Self {
        Self {
            prepared: true,
            announces: Some(Default::default()),
            codes_queue: Some(Default::default()),
            last_committed_batch: Some(Default::default()),
            last_committed_announce: Some(Default::default()),
        }
    }
}

#[auto_impl::auto_impl(&, Box)]
pub trait HashStorageRO {
    fn read_by_hash(&self, hash: H256) -> Option<Vec<u8>>;
}

#[auto_impl::auto_impl(&, Box)]
pub trait BlockMetaStorageRO {
    /// NOTE: if `BlockMeta` doesn't exist in the database, it will return the default value.
    fn block_meta(&self, block_hash: H256) -> BlockMeta;
}

#[auto_impl::auto_impl(&)]
pub trait BlockMetaStorageRW: BlockMetaStorageRO {
    /// NOTE: if `BlockMeta` doesn't exist in the database,
    /// it will be created with default values and then will be mutated.
    fn mutate_block_meta(&self, block_hash: H256, f: impl FnOnce(&mut BlockMeta));
}

#[auto_impl::auto_impl(&, Box)]
pub trait CodesStorageRO {
    fn original_code_exists(&self, code_id: CodeId) -> bool;
    fn original_code(&self, code_id: CodeId) -> Option<Vec<u8>>;
    fn program_code_id(&self, program_id: ActorId) -> Option<CodeId>;
    fn instrumented_code_exists(&self, runtime_id: u32, code_id: CodeId) -> bool;
    fn instrumented_code(&self, runtime_id: u32, code_id: CodeId) -> Option<InstrumentedCode>;
    fn code_metadata(&self, code_id: CodeId) -> Option<CodeMetadata>;
    fn code_valid(&self, code_id: CodeId) -> Option<bool>;
    fn valid_codes(&self) -> BTreeSet<CodeId>;
}

#[auto_impl::auto_impl(&)]
pub trait CodesStorageRW: CodesStorageRO {
    fn set_original_code(&self, code: &[u8]) -> CodeId;
    fn set_program_code_id(&self, program_id: ActorId, code_id: CodeId);
    fn set_instrumented_code(&self, runtime_id: u32, code_id: CodeId, code: InstrumentedCode);
    fn set_code_metadata(&self, code_id: CodeId, code_metadata: CodeMetadata);
    fn set_code_valid(&self, code_id: CodeId, valid: bool);
}

#[auto_impl::auto_impl(&, Box)]
pub trait OnChainStorageRO {
    fn block_header(&self, block_hash: H256) -> Option<BlockHeader>;
    fn block_events(&self, block_hash: H256) -> Option<Vec<BlockEvent>>;
    fn code_blob_info(&self, code_id: CodeId) -> Option<CodeBlobInfo>;
    fn block_synced(&self, block_hash: H256) -> bool;
    fn validators(&self, era_index: u64) -> Option<ValidatorsVec>;
    // TODO kuzmindev: temporal solution - must move into block meta or something else.
    fn block_validators_committed_for_era(&self, block_hash: H256) -> Option<u64>;
    fn protocol_timelines(&self) -> Option<ProtocolTimelines>;
}

#[auto_impl::auto_impl(&)]
pub trait OnChainStorageRW: OnChainStorageRO {
    fn set_block_header(&self, block_hash: H256, header: BlockHeader);
    fn set_block_events(&self, block_hash: H256, events: &[BlockEvent]);
    fn set_code_blob_info(&self, code_id: CodeId, code_info: CodeBlobInfo);
    fn set_protocol_timelines(&self, timelines: ProtocolTimelines);
    fn set_validators(&self, era_index: u64, validator_set: ValidatorsVec);
    fn set_block_validators_committed_for_era(&self, block_hash: H256, era_index: u64);
    fn set_block_synced(&self, block_hash: H256);
}

#[auto_impl::auto_impl(&)]
pub trait InjectedStorageRO {
    /// Returns the transactions by its hash.
    fn injected_transaction(
        &self,
        hash: HashOf<InjectedTransaction>,
    ) -> Option<SignedInjectedTransaction>;
}

#[auto_impl::auto_impl(&)]
pub trait InjectedStorageRW: InjectedStorageRO {
    fn set_injected_transaction(&self, tx: SignedInjectedTransaction);
}

#[derive(Debug, Clone, Default, Encode, Decode, PartialEq, Eq, Hash)]
pub struct AnnounceMeta {
    pub computed: bool,
}

#[auto_impl::auto_impl(&, Box)]
pub trait AnnounceStorageRO {
    fn announce(&self, hash: HashOf<Announce>) -> Option<Announce>;
    fn announce_program_states(&self, announce_hash: HashOf<Announce>) -> Option<ProgramStates>;
    fn announce_outcome(&self, announce_hash: HashOf<Announce>) -> Option<Vec<StateTransition>>;
    fn announce_schedule(&self, announce_hash: HashOf<Announce>) -> Option<Schedule>;
    fn announce_meta(&self, announce_hash: HashOf<Announce>) -> AnnounceMeta;
}

#[auto_impl::auto_impl(&)]
pub trait AnnounceStorageRW: AnnounceStorageRO {
    fn set_announce(&self, announce: Announce) -> HashOf<Announce>;
    fn set_announce_program_states(
        &self,
        announce_hash: HashOf<Announce>,
        program_states: ProgramStates,
    );
    fn set_announce_outcome(&self, announce_hash: HashOf<Announce>, outcome: Vec<StateTransition>);
    fn set_announce_schedule(&self, announce_hash: HashOf<Announce>, schedule: Schedule);

    fn mutate_announce_meta(
        &self,
        announce_hash: HashOf<Announce>,
        f: impl FnOnce(&mut AnnounceMeta),
    );
}

#[derive(Debug, Clone, Default, Encode, Decode, PartialEq, Eq)]
pub struct LatestData {
    /// Latest synced block
    pub synced_block: SimpleBlockData,
    /// Latest prepared block hash
    pub prepared_block_hash: H256,
    /// Latest computed announce hash
    pub computed_announce_hash: HashOf<Announce>,
    /// Genesis block hash
    pub genesis_block_hash: H256,
    /// Genesis announce hash
    pub genesis_announce_hash: HashOf<Announce>,
    /// Start block hash: genesis or defined by fast-sync
    pub start_block_hash: H256,
    /// Start announce hash: genesis or defined by fast-sync
    pub start_announce_hash: HashOf<Announce>,
}

#[auto_impl::auto_impl(&, Box)]
pub trait LatestDataStorageRO {
    fn latest_data(&self) -> Option<LatestData>;
}

#[auto_impl::auto_impl(&)]
pub trait LatestDataStorageRW: LatestDataStorageRO {
    fn set_latest_data(&self, data: LatestData);
    fn mutate_latest_data(&self, f: impl FnOnce(&mut LatestData)) -> Option<()> {
        if let Some(mut latest_data) = self.latest_data() {
            f(&mut latest_data);
            self.set_latest_data(latest_data);
            Some(())
        } else {
            None
        }
    }
}

pub struct PreparedBlockData {
    pub header: BlockHeader,
    pub events: Vec<BlockEvent>,
    pub latest_era_with_committed_validators: u64,
    pub codes_queue: VecDeque<CodeId>,
    pub announces: BTreeSet<HashOf<Announce>>,
    pub last_committed_batch: Digest,
    pub last_committed_announce: HashOf<Announce>,
}

pub struct ComputedAnnounceData {
    pub announce: Announce,
    pub program_states: ProgramStates,
    pub outcome: Vec<StateTransition>,
    pub schedule: Schedule,
}
