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

use crate::{
    overlay::{CASOverlay, KVOverlay},
    CASDatabase, KVDatabase,
};
use ethexe_common::{
    db::{BlockHeader, BlockMetaStorage, CodesStorage, Schedule},
    router::StateTransition,
    BlockRequestEvent,
};
use ethexe_runtime_common::state::{
    Allocations, DispatchStash, HashOf, Mailbox, MemoryPages, MessageQueue, ProgramState, Storage,
    Waitlist,
};
use gear_core::{
    code::InstrumentedCode,
    ids::{ActorId, CodeId, ProgramId},
    memory::PageBuf,
    message::Payload,
};
use gprimitives::H256;
use parity_scale_codec::{Decode, Encode};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

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
    CodeValid = 10,
    BlockStartSchedule = 11,
    BlockEndSchedule = 12,
}

impl KeyPrefix {
    fn prefix(self) -> [u8; 32] {
        H256::from_low_u64_be(self as u64).0
    }

    fn one(self, key: impl AsRef<[u8]>) -> Vec<u8> {
        [self.prefix().as_ref(), key.as_ref()].concat()
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

    fn block_events(&self, block_hash: H256) -> Option<Vec<BlockRequestEvent>> {
        self.kv
            .get(&KeyPrefix::BlockEvents.two(self.router_address, block_hash))
            .map(|data| {
                Vec::<BlockRequestEvent>::decode(&mut data.as_slice())
                    .expect("Failed to decode data into `Vec<BlockEvent>`")
            })
    }

    fn set_block_events(&self, block_hash: H256, events: Vec<BlockRequestEvent>) {
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

    fn latest_valid_block(&self) -> Option<(H256, BlockHeader)> {
        self.kv
            .get(&KeyPrefix::LatestValidBlock.one(self.router_address))
            .map(|data| {
                <(H256, BlockHeader)>::decode(&mut data.as_slice())
                    .expect("Failed to decode data into `(H256, BlockHeader)`")
            })
    }

    fn set_latest_valid_block(&self, block_hash: H256, header: BlockHeader) {
        self.kv.put(
            &KeyPrefix::LatestValidBlock.one(self.router_address),
            (block_hash, header).encode(),
        );
    }

    fn block_start_schedule(&self, block_hash: H256) -> Option<Schedule> {
        self.kv
            .get(&KeyPrefix::BlockStartSchedule.two(self.router_address, block_hash))
            .map(|data| {
                Schedule::decode(&mut data.as_slice())
                    .expect("Failed to decode data into `BTreeMap`")
            })
    }

    fn set_block_start_schedule(&self, block_hash: H256, map: Schedule) {
        log::trace!(target: LOG_TARGET, "For block {block_hash} set block start schedule: {map:?}");
        self.kv.put(
            &KeyPrefix::BlockStartSchedule.two(self.router_address, block_hash),
            map.encode(),
        );
    }

    fn block_end_schedule(&self, block_hash: H256) -> Option<Schedule> {
        self.kv
            .get(&KeyPrefix::BlockEndSchedule.two(self.router_address, block_hash))
            .map(|data| {
                Schedule::decode(&mut data.as_slice())
                    .expect("Failed to decode data into `BTreeMap`")
            })
    }

    fn set_block_end_schedule(&self, block_hash: H256, map: Schedule) {
        log::trace!(target: LOG_TARGET, "For block {block_hash} set block end schedule: {map:?}");
        self.kv.put(
            &KeyPrefix::BlockEndSchedule.two(self.router_address, block_hash),
            map.encode(),
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

    fn program_ids(&self) -> BTreeSet<ProgramId> {
        let key_prefix = KeyPrefix::ProgramToCodeId.prefix();

        self.kv
            .iter_prefix(&key_prefix)
            .map(|#[allow(unused_variables)] (key, code_id)| {
                let (split_key_prefix, program_id) = key.split_at(key_prefix.len());
                debug_assert_eq!(split_key_prefix, key_prefix);
                let program_id =
                    ProgramId::try_from(program_id).expect("Failed to decode key into `ProgramId`");

                #[cfg(debug_assertions)]
                CodeId::try_from(code_id.as_slice()).expect("Failed to decode data into `CodeId`");

                program_id
            })
            .collect()
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

    fn code_valid(&self, code_id: CodeId) -> Option<bool> {
        self.kv.get(&KeyPrefix::CodeValid.one(code_id)).map(|data| {
            bool::decode(&mut data.as_slice()).expect("Failed to decode data into `bool`")
        })
    }

    fn set_code_valid(&self, code_id: CodeId, approved: bool) {
        self.kv
            .put(&KeyPrefix::CodeValid.one(code_id), approved.encode());
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

    /// # Safety
    /// Not ready for using in prod. Intended to be for rpc calls only.
    pub unsafe fn overlaid(self) -> Self {
        Self {
            cas: Box::new(CASOverlay::new(self.cas)),
            kv: Box::new(KVOverlay::new(self.kv)),
            router_address: self.router_address,
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

    fn read_queue(&self, hash: HashOf<MessageQueue>) -> Option<MessageQueue> {
        self.cas.read(&hash.hash()).map(|data| {
            MessageQueue::decode(&mut &data[..]).expect("Failed to decode data into `MessageQueue`")
        })
    }

    fn write_queue(&self, queue: MessageQueue) -> HashOf<MessageQueue> {
        unsafe { HashOf::new(self.cas.write(&queue.encode())) }
    }

    fn read_waitlist(&self, hash: HashOf<Waitlist>) -> Option<Waitlist> {
        self.cas.read(&hash.hash()).map(|data| {
            Waitlist::decode(&mut data.as_slice()).expect("Failed to decode data into `Waitlist`")
        })
    }

    fn write_waitlist(&self, waitlist: Waitlist) -> HashOf<Waitlist> {
        unsafe { HashOf::new(self.cas.write(&waitlist.encode())) }
    }

    fn read_stash(&self, hash: HashOf<DispatchStash>) -> Option<DispatchStash> {
        self.cas.read(&hash.hash()).map(|data| {
            DispatchStash::decode(&mut data.as_slice())
                .expect("Failed to decode data into `DispatchStash`")
        })
    }

    fn write_stash(&self, stash: DispatchStash) -> HashOf<DispatchStash> {
        unsafe { HashOf::new(self.cas.write(&stash.encode())) }
    }

    fn read_mailbox(&self, hash: HashOf<Mailbox>) -> Option<Mailbox> {
        self.cas.read(&hash.hash()).map(|data| {
            Mailbox::decode(&mut data.as_slice()).expect("Failed to decode data into `Mailbox`")
        })
    }

    fn write_mailbox(&self, mailbox: Mailbox) -> HashOf<Mailbox> {
        unsafe { HashOf::new(self.cas.write(&mailbox.encode())) }
    }

    fn read_pages(&self, hash: HashOf<MemoryPages>) -> Option<MemoryPages> {
        self.cas.read(&hash.hash()).map(|data| {
            MemoryPages::decode(&mut &data[..]).expect("Failed to decode data into `MemoryPages`")
        })
    }

    fn write_pages(&self, pages: MemoryPages) -> HashOf<MemoryPages> {
        unsafe { HashOf::new(self.cas.write(&pages.encode())) }
    }

    fn read_allocations(&self, hash: HashOf<Allocations>) -> Option<Allocations> {
        self.cas.read(&hash.hash()).map(|data| {
            Allocations::decode(&mut &data[..]).expect("Failed to decode data into `Allocations`")
        })
    }

    fn write_allocations(&self, allocations: Allocations) -> HashOf<Allocations> {
        unsafe { HashOf::new(self.cas.write(&allocations.encode())) }
    }

    fn read_payload(&self, hash: HashOf<Payload>) -> Option<Payload> {
        self.cas
            .read(&hash.hash())
            .map(|data| Payload::try_from(data).expect("Failed to decode data into `Payload`"))
    }

    fn write_payload(&self, payload: Payload) -> HashOf<Payload> {
        unsafe { HashOf::new(self.cas.write(payload.inner())) }
    }

    fn read_page_data(&self, hash: HashOf<PageBuf>) -> Option<PageBuf> {
        self.cas.read(&hash.hash()).map(|data| {
            PageBuf::decode(&mut data.as_slice()).expect("Failed to decode data into `PageBuf`")
        })
    }

    fn write_page_data(&self, data: PageBuf) -> HashOf<PageBuf> {
        unsafe { HashOf::new(self.cas.write(&data)) }
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
