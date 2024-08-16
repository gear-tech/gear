// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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

use crate::{router::StateTransition, BlockEvent};
use alloc::{
    collections::{BTreeMap, VecDeque},
    vec::Vec,
};
use gear_core::{
    code::InstrumentedCode,
    ids::{ActorId, CodeId, ProgramId},
};
use gprimitives::H256;
use parity_scale_codec::{Decode, Encode};

#[derive(Debug, Clone, Default, Encode, Decode, serde::Serialize)]
pub struct BlockHeader {
    pub height: u32,
    pub timestamp: u64,
    pub parent_hash: H256,
}

#[derive(Debug, Clone, Default, Encode, Decode)]
pub struct CodeUploadInfo {
    pub origin: ActorId,
    pub tx_hash: H256,
}

pub trait BlockMetaStorage: Send + Sync {
    fn block_header(&self, block_hash: H256) -> Option<BlockHeader>;
    fn set_block_header(&self, block_hash: H256, header: BlockHeader);

    fn block_end_state_is_valid(&self, block_hash: H256) -> Option<bool>;
    fn set_block_end_state_is_valid(&self, block_hash: H256, is_valid: bool);

    fn block_is_empty(&self, block_hash: H256) -> Option<bool>;
    fn set_block_is_empty(&self, block_hash: H256, is_empty: bool);

    fn block_commitment_queue(&self, block_hash: H256) -> Option<VecDeque<H256>>;
    fn set_block_commitment_queue(&self, block_hash: H256, queue: VecDeque<H256>);

    fn block_prev_commitment(&self, block_hash: H256) -> Option<H256>;
    fn set_block_prev_commitment(&self, block_hash: H256, prev_commitment: H256);

    fn block_start_program_states(&self, block_hash: H256) -> Option<BTreeMap<ActorId, H256>>;
    fn set_block_start_program_states(&self, block_hash: H256, map: BTreeMap<ActorId, H256>);

    fn block_end_program_states(&self, block_hash: H256) -> Option<BTreeMap<ActorId, H256>>;
    fn set_block_end_program_states(&self, block_hash: H256, map: BTreeMap<ActorId, H256>);

    fn block_events(&self, block_hash: H256) -> Option<Vec<BlockEvent>>;
    fn set_block_events(&self, block_hash: H256, events: Vec<BlockEvent>);

    fn block_outcome(&self, block_hash: H256) -> Option<Vec<StateTransition>>;
    fn set_block_outcome(&self, block_hash: H256, outcome: Vec<StateTransition>);

    fn latest_valid_block_height(&self) -> Option<u32>;
    fn set_latest_valid_block_height(&self, block_height: u32);
}

pub trait CodesStorage: Send + Sync {
    fn original_code(&self, code_id: CodeId) -> Option<Vec<u8>>;
    fn set_original_code(&self, code: &[u8]) -> CodeId;

    fn program_code_id(&self, program_id: ProgramId) -> Option<CodeId>;
    fn set_program_code_id(&self, program_id: ProgramId, code_id: CodeId);

    fn instrumented_code(&self, runtime_id: u32, code_id: CodeId) -> Option<InstrumentedCode>;
    fn set_instrumented_code(&self, runtime_id: u32, code_id: CodeId, code: InstrumentedCode);

    fn code_blob_tx(&self, code_id: CodeId) -> Option<H256>;
    fn set_code_blob_tx(&self, code_id: CodeId, blob_tx_hash: H256);

    fn code_valid(&self, code_id: CodeId) -> Option<bool>;
    fn set_code_valid(&self, code_id: CodeId, valid: bool);
}
