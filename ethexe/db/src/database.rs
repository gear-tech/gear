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

//! Database for ethexe.

use std::collections::{BTreeMap, VecDeque};

use crate::{CASDatabase, KVDatabase};
use ethexe_common::{
    db::{BlockHeader, BlockMetaStorage, CodesStorage},
    router::StateTransition,
    BlockEventForHandling,
};
use ethexe_runtime_common::state::{
    Allocations, MemoryPages, MessageQueue, ProgramState, Storage, Waitlist,
};
use gear_core::{
    code::InstrumentedCode,
    ids::{ActorId, CodeId, ProgramId},
    memory::PageBuf,
    message::Payload,
};
use gprimitives::H256;
use parity_scale_codec::{Decode, Encode};

const LOG_TARGET: &str = "ethexe-db";

#[repr(u64)]
enum KeyPrefix {
    ProgramToCodeId = 0,
    InstrumentedCode = 1,
    BlockStartProgramStates = 2,
    BlockEndProgramStates = 3,
    BlockEvents = 4,
    BlockOutcome = 5,
    BlockSmallMeta = 6,
    CodeUpload = 7,
    LatestValidBlock = 8,
    BlockHeader = 9,
}

impl KeyPrefix {
    fn one(self, key: impl AsRef<[u8]>) -> Vec<u8> {
        [H256::from_low_u64_be(self as u64).as_bytes(), key.as_ref()].concat()
    }

    fn two(self, key1: impl AsRef<[u8]>, key2: impl AsRef<[u8]>) -> Vec<u8> {
        let key = [key1.as_ref(), key2.as_ref()].concat();
        self.one(key)
    }

    fn three(
        self,
        key1: impl AsRef<[u8]>,
        key2: impl AsRef<[u8]>,
        key3: impl AsRef<[u8]>,
    ) -> Vec<u8> {
        let key = [key1.as_ref(), key2.as_ref(), key3.as_ref()].concat();
        self.one(key)
    }
}

pub struct Database {
    cas: Box<dyn CASDatabase>,
    kv: Box<dyn KVDatabase>,
    router_address: [u8; 20],
}

impl Clone for Database {
    fn clone(&self) -> Self {
        Self {
            cas: self.cas.clone_boxed(),
            kv: self.kv.clone_boxed_kv(),
            router_address: self.router_address,
        }
    }
}

#[derive(Debug, Clone, Default, Encode, Decode, serde::Serialize)]
struct BlockSmallMetaInfo {
    block_end_state_is_valid: bool,
    is_empty: Option<bool>,
    prev_commitment: Option<H256>,
    commitment_queue: Option<VecDeque<H256>>,
}

impl BlockMetaStorage for Database {
    fn block_header(&self, block_hash: H256) -> Option<BlockHeader> {
        self.kv
            .get(&KeyPrefix::BlockHeader.one(block_hash))
            .map(|data| {
                BlockHeader::decode(&mut data.as_slice())
                    .expect("Failed to decode data into `BlockHeader`")
            })
    }

    fn set_block_header(&self, block_hash: H256, header: BlockHeader) {
        log::trace!(target: LOG_TARGET, "For block {block_hash} set header: {header:?}");
        self.kv
            .put(&KeyPrefix::BlockHeader.one(block_hash), header.encode());
    }

    fn block_end_state_is_valid(&self, block_hash: H256) -> Option<bool> {
        self.block_small_meta(block_hash)
            .map(|meta| meta.block_end_state_is_valid)
    }

    fn set_block_end_state_is_valid(&self, block_hash: H256, is_valid: bool) {
        log::trace!(target: LOG_TARGET, "For block {block_hash} set end state valid: {is_valid}");
        let meta = self.block_small_meta(block_hash).unwrap_or_default();
        self.set_block_small_meta(
            block_hash,
            BlockSmallMetaInfo {
                block_end_state_is_valid: is_valid,
                ..meta
            },
        );
    }

    fn block_is_empty(&self, block_hash: H256) -> Option<bool> {
        self.block_small_meta(block_hash)
            .and_then(|meta| meta.is_empty)
    }

    fn set_block_is_empty(&self, block_hash: H256, is_empty: bool) {
        log::trace!(target: LOG_TARGET, "For block {block_hash} set is empty: {is_empty}");
        let meta = self.block_small_meta(block_hash).unwrap_or_default();
        self.set_block_small_meta(
            block_hash,
            BlockSmallMetaInfo {
                is_empty: Some(is_empty),
                ..meta
            },
        );
    }

    fn block_commitment_queue(&self, block_hash: H256) -> Option<VecDeque<H256>> {
        self.block_small_meta(block_hash)
            .and_then(|meta| meta.commitment_queue)
    }

    fn set_block_commitment_queue(&self, block_hash: H256, queue: VecDeque<H256>) {
        log::trace!(target: LOG_TARGET, "For block {block_hash} set commitment queue: {queue:?}");
        let meta = self.block_small_meta(block_hash).unwrap_or_default();
        self.set_block_small_meta(
            block_hash,
            BlockSmallMetaInfo {
                commitment_queue: Some(queue),
                ..meta
            },
        );
    }

    fn block_prev_commitment(&self, block_hash: H256) -> Option<H256> {
        self.block_small_meta(block_hash)
            .and_then(|meta| meta.prev_commitment)
    }

    fn set_block_prev_commitment(&self, block_hash: H256, prev_commitment: H256) {
        log::trace!(target: LOG_TARGET, "For block {block_hash} set prev commitment: {prev_commitment}");
        let meta = self.block_small_meta(block_hash).unwrap_or_default();
        self.set_block_small_meta(
            block_hash,
            BlockSmallMetaInfo {
                prev_commitment: Some(prev_commitment),
                ..meta
            },
        );
    }

    fn block_start_program_states(&self, block_hash: H256) -> Option<BTreeMap<ActorId, H256>> {
        self.kv
            .get(&KeyPrefix::BlockStartProgramStates.two(self.router_address, block_hash))
            .map(|data| {
                BTreeMap::decode(&mut data.as_slice())
                    .expect("Failed to decode data into `BTreeMap`")
            })
    }

    fn set_block_start_program_states(&self, block_hash: H256, map: BTreeMap<ActorId, H256>) {
        log::trace!(target: LOG_TARGET, "For block {block_hash} set start program states: {map:?}");
        self.kv.put(
            &KeyPrefix::BlockStartProgramStates.two(self.router_address, block_hash),
            map.encode(),
        );
    }

    fn block_end_program_states(&self, block_hash: H256) -> Option<BTreeMap<ActorId, H256>> {
        self.kv
            .get(&KeyPrefix::BlockEndProgramStates.two(self.router_address, block_hash))
            .map(|data| {
                BTreeMap::decode(&mut data.as_slice())
                    .expect("Failed to decode data into `BTreeMap`")
            })
    }

    fn set_block_end_program_states(&self, block_hash: H256, map: BTreeMap<ActorId, H256>) {
        self.kv.put(
            &KeyPrefix::BlockEndProgramStates.two(self.router_address, block_hash),
            map.encode(),
        );
    }

    fn block_events(&self, block_hash: H256) -> Option<Vec<BlockEventForHandling>> {
        self.kv
            .get(&KeyPrefix::BlockEvents.two(self.router_address, block_hash))
            .map(|data| {
                Vec::<BlockEventForHandling>::decode(&mut data.as_slice())
                    .expect("Failed to decode data into `Vec<BlockEvent>`")
            })
    }

    fn set_block_events(&self, block_hash: H256, events: Vec<BlockEventForHandling>) {
        self.kv.put(
            &KeyPrefix::BlockEvents.two(self.router_address, block_hash),
            events.encode(),
        );
    }

    fn block_outcome(&self, block_hash: H256) -> Option<Vec<StateTransition>> {
        self.kv
            .get(&KeyPrefix::BlockOutcome.two(self.router_address, block_hash))
            .map(|data| {
                Vec::<StateTransition>::decode(&mut data.as_slice())
                    .expect("Failed to decode data into `Vec<StateTransition>`")
            })
    }

    fn set_block_outcome(&self, block_hash: H256, outcome: Vec<StateTransition>) {
        self.kv.put(
            &KeyPrefix::BlockOutcome.two(self.router_address, block_hash),
            outcome.encode(),
        );
    }

    fn latest_valid_block_height(&self) -> Option<u32> {
        self.kv
            .get(&KeyPrefix::LatestValidBlock.one(self.router_address))
            .map(|block_height| {
                u32::from_le_bytes(block_height.try_into().expect("must be correct; qed"))
            })
    }

    fn set_latest_valid_block_height(&self, block_height: u32) {
        self.kv.put(
            &KeyPrefix::LatestValidBlock.one(self.router_address),
            block_height.to_le_bytes().to_vec(),
        );
    }
}

impl CodesStorage for Database {
    fn original_code(&self, code_id: CodeId) -> Option<Vec<u8>> {
        let hash = H256::from(code_id.into_bytes());
        self.cas.read(&hash)
    }

    fn set_original_code(&self, code: &[u8]) -> CodeId {
        self.cas.write(code).into()
    }

    fn program_code_id(&self, program_id: ProgramId) -> Option<CodeId> {
        self.kv
            .get(&KeyPrefix::ProgramToCodeId.one(program_id))
            .map(|data| {
                CodeId::try_from(data.as_slice()).expect("Failed to decode data into `CodeId`")
            })
    }

    fn set_program_code_id(&self, program_id: ProgramId, code_id: CodeId) {
        self.kv.put(
            &KeyPrefix::ProgramToCodeId.one(program_id),
            code_id.into_bytes().to_vec(),
        );
    }

    fn instrumented_code(&self, runtime_id: u32, code_id: CodeId) -> Option<InstrumentedCode> {
        self.kv
            .get(&KeyPrefix::InstrumentedCode.three(
                self.router_address,
                runtime_id.to_le_bytes(),
                code_id,
            ))
            .map(|data| {
                InstrumentedCode::decode(&mut data.as_slice())
                    .expect("Failed to decode data into `InstrumentedCode`")
            })
    }

    fn set_instrumented_code(&self, runtime_id: u32, code_id: CodeId, code: InstrumentedCode) {
        self.kv.put(
            &KeyPrefix::InstrumentedCode.three(
                self.router_address,
                runtime_id.to_le_bytes(),
                code_id,
            ),
            code.encode(),
        );
    }

    fn code_blob_tx(&self, code_id: CodeId) -> Option<H256> {
        self.kv
            .get(&KeyPrefix::CodeUpload.one(code_id))
            .map(|data| {
                Decode::decode(&mut data.as_slice()).expect("Failed to decode data into `H256`")
            })
    }

    fn set_code_blob_tx(&self, code_id: CodeId, blob_tx_hash: H256) {
        self.kv
            .put(&KeyPrefix::CodeUpload.one(code_id), blob_tx_hash.encode());
    }
}

impl Database {
    pub fn new(
        cas: Box<dyn CASDatabase>,
        kv: Box<dyn KVDatabase>,
        router_address: [u8; 20],
    ) -> Self {
        Self {
            cas,
            kv,
            router_address,
        }
    }

    pub fn from_one<DB: CASDatabase + KVDatabase>(db: &DB, router_address: [u8; 20]) -> Self {
        Self {
            cas: CASDatabase::clone_boxed(db),
            kv: KVDatabase::clone_boxed_kv(db),
            router_address,
        }
    }

    // TODO: temporary solution for MVP runtime-interfaces db access.
    pub fn read_by_hash(&self, hash: H256) -> Option<Vec<u8>> {
        self.cas.read(&hash)
    }

    // TODO: temporary solution for MVP runtime-interfaces db access.
    pub fn write(&self, data: &[u8]) -> H256 {
        self.cas.write(data)
    }

    fn block_small_meta(&self, block_hash: H256) -> Option<BlockSmallMetaInfo> {
        self.kv
            .get(&KeyPrefix::BlockSmallMeta.two(self.router_address, block_hash))
            .map(|data| {
                BlockSmallMetaInfo::decode(&mut data.as_slice())
                    .expect("Failed to decode data into `BlockSmallMetaInfo`")
            })
    }

    fn set_block_small_meta(&self, block_hash: H256, meta: BlockSmallMetaInfo) {
        self.kv.put(
            &KeyPrefix::BlockSmallMeta.two(self.router_address, block_hash),
            meta.encode(),
        );
    }
}

// TODO: consider to change decode panics to Results.
impl Storage for Database {
    fn read_state(&self, hash: H256) -> Option<ProgramState> {
        if hash.is_zero() {
            return Some(ProgramState::zero());
        }

        let data = self.cas.read(&hash)?;

        let state = ProgramState::decode(&mut &data[..])
            .expect("Failed to decode data into `ProgramState`");

        Some(state)
    }

    fn write_state(&self, state: ProgramState) -> H256 {
        if state.is_zero() {
            return H256::zero();
        }

        self.cas.write(&state.encode())
    }

    fn read_queue(&self, hash: H256) -> Option<MessageQueue> {
        let data = self.cas.read(&hash)?;
        Some(
            MessageQueue::decode(&mut &data[..])
                .expect("Failed to decode data into `MessageQueue`"),
        )
    }

    fn write_queue(&self, queue: MessageQueue) -> H256 {
        self.cas.write(&queue.encode())
    }

    fn read_waitlist(&self, hash: H256) -> Option<Waitlist> {
        self.cas.read(&hash).map(|data| {
            Waitlist::decode(&mut data.as_slice()).expect("Failed to decode data into `Waitlist`")
        })
    }

    fn write_waitlist(&self, waitlist: Waitlist) -> H256 {
        self.cas.write(&waitlist.encode())
    }

    fn read_pages(&self, hash: H256) -> Option<MemoryPages> {
        let data = self.cas.read(&hash)?;
        Some(MemoryPages::decode(&mut &data[..]).expect("Failed to decode data into `MemoryPages`"))
    }

    fn write_pages(&self, pages: MemoryPages) -> H256 {
        self.cas.write(&pages.encode())
    }

    fn read_allocations(&self, hash: H256) -> Option<Allocations> {
        let data = self.cas.read(&hash)?;
        Some(Allocations::decode(&mut &data[..]).expect("Failed to decode data into `Allocations`"))
    }

    fn write_allocations(&self, allocations: Allocations) -> H256 {
        self.cas.write(&allocations.encode())
    }

    fn read_payload(&self, hash: H256) -> Option<Payload> {
        let data = self.cas.read(&hash)?;
        Some(Payload::try_from(data).expect("Failed to decode data into `Payload`"))
    }

    fn write_payload(&self, payload: Payload) -> H256 {
        self.cas.write(payload.inner())
    }

    fn read_page_data(&self, hash: H256) -> Option<PageBuf> {
        let data = self.cas.read(&hash)?;
        Some(PageBuf::decode(&mut data.as_slice()).expect("Failed to decode data into `PageBuf`"))
    }

    fn write_page_data(&self, data: PageBuf) -> H256 {
        self.cas.write(&data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_database() {
        let db = crate::MemDb::default();
        let database = crate::Database::from_one(&db, Default::default());

        let block_hash = H256::zero();
        // let parent_hash = H256::zero();
        let map: BTreeMap<ActorId, H256> = [(ActorId::zero(), H256::zero())].into();

        database.set_block_start_program_states(block_hash, map.clone());
        assert_eq!(database.block_start_program_states(block_hash), Some(map));

        // database.set_parent_hash(block_hash, parent_hash);
        // assert_eq!(database.parent_hash(block_hash), Some(parent_hash));
    }
}
