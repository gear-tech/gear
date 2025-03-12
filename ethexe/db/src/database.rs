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

//! Database for ethexe.

use crate::{
    overlay::{CASOverlay, KVOverlay},
    CASDatabase, KVDatabase,
};
use anyhow::{bail, Result};
use ethexe_common::{
    db::{BlockHeader, BlockMetaStorage, CodeInfo, CodesStorage, OnChainStorage, Schedule},
    events::BlockEvent,
    gear::StateTransition,
    tx_pool::{OffchainTransaction, SignedOffchainTransaction},
};
use ethexe_runtime_common::state::{
    Allocations, DispatchStash, HashOf, Mailbox, MemoryPages, MemoryPagesRegion, MessageQueue,
    ProgramState, Storage, Waitlist,
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

// TODO (gsobol): because router address is a part of almost all keys, consider to use different db for each router.
// TODO (gsobol): make separate structures for each key prefix. Each structure should have own method for key generation.
#[repr(u64)]
enum KeyPrefix {
    /// BlockSmallData key prefix uses two keys: router address and block hash.
    BlockSmallData = 0,
    /// BlockEvents key prefix uses two keys: router address and block hash.
    BlockEvents = 1,

    /// BlockProgramStates key prefix uses two keys: router address and block hash.
    BlockProgramStates = 2,
    /// BlockOutcome key prefix uses two keys: router address and block hash.
    BlockOutcome = 3,
    /// BlockSchedule key prefix uses two keys: router address and block hash.
    BlockSchedule = 4,

    /// ProgramToCodeId key prefix uses two keys: router address and program id.
    ProgramToCodeId = 5,
    /// InstrumentedCode key prefix uses three keys: router address, runtime id and code id.
    InstrumentedCode = 6,
    /// CodeUploadInfo key prefix uses two keys: router address and code id.
    CodeUploadInfo = 7,
    /// CodeValid key prefix uses two keys: router address and code id.
    CodeValid = 8,

    /// SignedTransaction key prefix uses one key: transaction hash.
    SignedTransaction = 9,

    /// LatestComputedBlock key prefix uses one key: router address.
    LatestComputedBlock = 10,
    /// LatestSyncedBlockHeight key prefix uses one key: router address.
    LatestSyncedBlockHeight = 11,
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
            kv: self.kv.clone_boxed(),
            router_address: self.router_address,
        }
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
            kv: KVDatabase::clone_boxed(db),
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

    pub fn read_by_hash(&self, hash: H256) -> Option<Vec<u8>> {
        self.cas.read(hash)
    }

    pub fn write(&self, data: &[u8]) -> H256 {
        self.cas.write(data)
    }

    pub fn get_offchain_transaction(&self, tx_hash: H256) -> Option<SignedOffchainTransaction> {
        self.kv
            .get(&KeyPrefix::SignedTransaction.one(tx_hash))
            .map(|data| {
                Decode::decode(&mut data.as_slice())
                    .expect("failed to data into `SignedTransaction`")
            })
    }

    pub fn set_offchain_transaction(&self, tx: SignedOffchainTransaction) {
        let tx_hash = tx.tx_hash();
        self.kv
            .put(&KeyPrefix::SignedTransaction.one(tx_hash), tx.encode());
    }

    // TODO (gsobol): test this method
    pub fn check_within_recent_blocks(&self, reference_block_hash: H256) -> Result<bool> {
        let Some((latest_computed_block_hash, latest_computed_block_header)) =
            self.latest_computed_block()
        else {
            bail!("No latest valid block found");
        };
        let Some(reference_block_header) = OnChainStorage::block_header(self, reference_block_hash)
        else {
            bail!("No reference block found");
        };

        // If reference block is far away from the latest valid block, it's not in the window.
        let Some(actual_window) = latest_computed_block_header
            .height
            .checked_sub(reference_block_header.height)
        else {
            bail!("Can't calculate actual window: reference block hash doesn't suit actual blocks state");
        };

        if actual_window > OffchainTransaction::BLOCK_HASHES_WINDOW_SIZE {
            return Ok(false);
        }

        // Check against reorgs.
        let mut block_hash = latest_computed_block_hash;
        for _ in 0..OffchainTransaction::BLOCK_HASHES_WINDOW_SIZE {
            if block_hash == reference_block_hash {
                return Ok(true);
            }

            let Some(block_header) = OnChainStorage::block_header(self, block_hash) else {
                bail!(
                    "Block with {block_hash} hash not found in the window. Possibly reorg happened"
                );
            };
            block_hash = block_header.parent_hash;
        }

        Ok(false)
    }

    fn with_small_data<R>(
        &self,
        block_hash: H256,
        f: impl FnOnce(BlockSmallData) -> R,
    ) -> Option<R> {
        self.block_small_data(block_hash).map(f)
    }

    fn mutate_small_data(&self, block_hash: H256, f: impl FnOnce(&mut BlockSmallData)) {
        let mut data = self.block_small_data(block_hash).unwrap_or_default();
        f(&mut data);
        self.set_block_small_data(block_hash, data);
    }

    fn block_small_data(&self, block_hash: H256) -> Option<BlockSmallData> {
        self.kv
            .get(&KeyPrefix::BlockSmallData.two(self.router_address, block_hash))
            .map(|data| {
                BlockSmallData::decode(&mut data.as_slice())
                    .expect("Failed to decode data into `BlockSmallMetaInfo`")
            })
    }

    fn set_block_small_data(&self, block_hash: H256, meta: BlockSmallData) {
        self.kv.put(
            &KeyPrefix::BlockSmallData.two(self.router_address, block_hash),
            meta.encode(),
        );
    }
}

#[cfg_attr(test, derive(serde::Serialize))]
#[derive(Debug, Clone, Default, Encode, Decode, PartialEq, Eq)]
struct BlockSmallData {
    block_header: Option<BlockHeader>,
    block_synced: bool,
    block_computed: bool,
    prev_not_empty_block: Option<H256>,
    commitment_queue: Option<VecDeque<H256>>,
}

impl BlockMetaStorage for Database {
    fn block_computed(&self, block_hash: H256) -> bool {
        self.with_small_data(block_hash, |data| data.block_computed)
            .unwrap_or(false)
    }

    fn set_block_computed(&self, block_hash: H256) {
        log::trace!("For block {block_hash} set block computed");
        self.mutate_small_data(block_hash, |data| data.block_computed = true);
    }

    fn block_commitment_queue(&self, block_hash: H256) -> Option<VecDeque<H256>> {
        self.with_small_data(block_hash, |data| data.commitment_queue)?
    }

    fn set_block_commitment_queue(&self, block_hash: H256, queue: VecDeque<H256>) {
        log::trace!("For block {block_hash} set commitment queue: {queue:?}");
        self.mutate_small_data(block_hash, |data| data.commitment_queue = Some(queue));
    }

    fn previous_not_empty_block(&self, block_hash: H256) -> Option<H256> {
        self.with_small_data(block_hash, |data| data.prev_not_empty_block)?
    }

    fn set_previous_not_empty_block(&self, block_hash: H256, prev_not_empty_block_hash: H256) {
        log::trace!("For block {block_hash} set prev commitment: {prev_not_empty_block_hash}");
        self.mutate_small_data(block_hash, |data| {
            data.prev_not_empty_block = Some(prev_not_empty_block_hash)
        });
    }

    fn block_program_states(&self, block_hash: H256) -> Option<BTreeMap<ActorId, H256>> {
        self.kv
            .get(&KeyPrefix::BlockProgramStates.two(self.router_address, block_hash))
            .map(|data| {
                BTreeMap::decode(&mut data.as_slice())
                    .expect("Failed to decode data into `BTreeMap`")
            })
    }

    fn set_block_program_states(&self, block_hash: H256, map: BTreeMap<ActorId, H256>) {
        log::trace!("For block {block_hash} set program states: {map:?}");
        self.kv.put(
            &KeyPrefix::BlockProgramStates.two(self.router_address, block_hash),
            map.encode(),
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
        log::trace!("For block {block_hash} set outcome: {outcome:?}");
        self.kv.put(
            &KeyPrefix::BlockOutcome.two(self.router_address, block_hash),
            outcome.encode(),
        );
    }

    fn block_schedule(&self, block_hash: H256) -> Option<Schedule> {
        self.kv
            .get(&KeyPrefix::BlockSchedule.two(self.router_address, block_hash))
            .map(|data| {
                Schedule::decode(&mut data.as_slice())
                    .expect("Failed to decode data into `BTreeMap`")
            })
    }

    fn set_block_schedule(&self, block_hash: H256, map: Schedule) {
        self.kv.put(
            &KeyPrefix::BlockSchedule.two(self.router_address, block_hash),
            map.encode(),
        );
    }

    fn latest_computed_block(&self) -> Option<(H256, BlockHeader)> {
        self.kv
            .get(&KeyPrefix::LatestComputedBlock.one(self.router_address))
            .map(|data| {
                <(H256, BlockHeader)>::decode(&mut data.as_slice())
                    .expect("Failed to decode data into `(H256, BlockHeader)`")
            })
    }

    fn set_latest_computed_block(&self, block_hash: H256, header: BlockHeader) {
        log::trace!("Set latest computed block: {block_hash} {header:?}");
        self.kv.put(
            &KeyPrefix::LatestComputedBlock.one(self.router_address),
            (block_hash, header).encode(),
        );
    }
}

impl CodesStorage for Database {
    fn original_code_exists(&self, code_id: CodeId) -> bool {
        self.cas.read(code_id.into()).is_some()
    }

    fn original_code(&self, code_id: CodeId) -> Option<Vec<u8>> {
        self.cas.read(code_id.into())
    }

    fn set_original_code(&self, code: &[u8]) -> CodeId {
        self.cas.write(code).into()
    }

    fn program_code_id(&self, program_id: ProgramId) -> Option<CodeId> {
        self.kv
            .get(&KeyPrefix::ProgramToCodeId.two(self.router_address, program_id))
            .map(|data| {
                CodeId::try_from(data.as_slice()).expect("Failed to decode data into `CodeId`")
            })
    }

    fn set_program_code_id(&self, program_id: ProgramId, code_id: CodeId) {
        self.kv.put(
            &KeyPrefix::ProgramToCodeId.two(self.router_address, program_id),
            code_id.into_bytes().to_vec(),
        );
    }

    // TODO (gsobol): consider to move to another place
    // TODO (gsobol): test this method
    fn program_ids(&self) -> BTreeSet<ProgramId> {
        let key_prefix = KeyPrefix::ProgramToCodeId.one(self.router_address);

        self.kv
            .iter_prefix(&key_prefix)
            .map(|(key, code_id)| {
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

    fn code_valid(&self, code_id: CodeId) -> Option<bool> {
        self.kv
            .get(&KeyPrefix::CodeValid.two(self.router_address, code_id))
            .map(|data| {
                bool::decode(&mut data.as_slice()).expect("Failed to decode data into `bool`")
            })
    }

    fn set_code_valid(&self, code_id: CodeId, valid: bool) {
        self.kv.put(
            &KeyPrefix::CodeValid.two(self.router_address, code_id),
            valid.encode(),
        );
    }
}

// TODO: consider to change decode panics to Results.
impl Storage for Database {
    fn read_state(&self, hash: H256) -> Option<ProgramState> {
        if hash.is_zero() {
            return Some(ProgramState::zero());
        }

        let data = self.cas.read(hash)?;

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
        self.cas.read(hash.hash()).map(|data| {
            MessageQueue::decode(&mut &data[..]).expect("Failed to decode data into `MessageQueue`")
        })
    }

    fn write_queue(&self, queue: MessageQueue) -> HashOf<MessageQueue> {
        unsafe { HashOf::new(self.cas.write(&queue.encode())) }
    }

    fn read_waitlist(&self, hash: HashOf<Waitlist>) -> Option<Waitlist> {
        self.cas.read(hash.hash()).map(|data| {
            Waitlist::decode(&mut data.as_slice()).expect("Failed to decode data into `Waitlist`")
        })
    }

    fn write_waitlist(&self, waitlist: Waitlist) -> HashOf<Waitlist> {
        unsafe { HashOf::new(self.cas.write(&waitlist.encode())) }
    }

    fn read_stash(&self, hash: HashOf<DispatchStash>) -> Option<DispatchStash> {
        self.cas.read(hash.hash()).map(|data| {
            DispatchStash::decode(&mut data.as_slice())
                .expect("Failed to decode data into `DispatchStash`")
        })
    }

    fn write_stash(&self, stash: DispatchStash) -> HashOf<DispatchStash> {
        unsafe { HashOf::new(self.cas.write(&stash.encode())) }
    }

    fn read_mailbox(&self, hash: HashOf<Mailbox>) -> Option<Mailbox> {
        self.cas.read(hash.hash()).map(|data| {
            Mailbox::decode(&mut data.as_slice()).expect("Failed to decode data into `Mailbox`")
        })
    }

    fn write_mailbox(&self, mailbox: Mailbox) -> HashOf<Mailbox> {
        unsafe { HashOf::new(self.cas.write(&mailbox.encode())) }
    }

    fn read_pages(&self, hash: HashOf<MemoryPages>) -> Option<MemoryPages> {
        self.cas.read(hash.hash()).map(|data| {
            MemoryPages::decode(&mut &data[..]).expect("Failed to decode data into `MemoryPages`")
        })
    }

    fn read_pages_region(&self, hash: HashOf<MemoryPagesRegion>) -> Option<MemoryPagesRegion> {
        self.cas.read(hash.hash()).map(|data| {
            MemoryPagesRegion::decode(&mut &data[..])
                .expect("Failed to decode data into `MemoryPagesRegion`")
        })
    }

    fn write_pages(&self, pages: MemoryPages) -> HashOf<MemoryPages> {
        unsafe { HashOf::new(self.cas.write(&pages.encode())) }
    }

    fn write_pages_region(&self, pages_region: MemoryPagesRegion) -> HashOf<MemoryPagesRegion> {
        unsafe { HashOf::new(self.cas.write(&pages_region.encode())) }
    }

    fn read_allocations(&self, hash: HashOf<Allocations>) -> Option<Allocations> {
        self.cas.read(hash.hash()).map(|data| {
            Allocations::decode(&mut &data[..]).expect("Failed to decode data into `Allocations`")
        })
    }

    fn write_allocations(&self, allocations: Allocations) -> HashOf<Allocations> {
        unsafe { HashOf::new(self.cas.write(&allocations.encode())) }
    }

    fn read_payload(&self, hash: HashOf<Payload>) -> Option<Payload> {
        self.cas
            .read(hash.hash())
            .map(|data| Payload::try_from(data).expect("Failed to decode data into `Payload`"))
    }

    fn write_payload(&self, payload: Payload) -> HashOf<Payload> {
        unsafe { HashOf::new(self.cas.write(payload.inner())) }
    }

    fn read_page_data(&self, hash: HashOf<PageBuf>) -> Option<PageBuf> {
        self.cas.read(hash.hash()).map(|data| {
            PageBuf::decode(&mut data.as_slice()).expect("Failed to decode data into `PageBuf`")
        })
    }

    fn write_page_data(&self, data: PageBuf) -> HashOf<PageBuf> {
        unsafe { HashOf::new(self.cas.write(&data)) }
    }
}

impl OnChainStorage for Database {
    fn block_header(&self, block_hash: H256) -> Option<BlockHeader> {
        self.with_small_data(block_hash, |data| data.block_header)?
    }

    fn set_block_header(&self, block_hash: H256, header: BlockHeader) {
        self.mutate_small_data(block_hash, |data| data.block_header = Some(header));
    }

    fn block_events(&self, block_hash: H256) -> Option<Vec<BlockEvent>> {
        self.kv
            .get(&KeyPrefix::BlockEvents.two(self.router_address, block_hash))
            .map(|data| {
                Vec::<BlockEvent>::decode(&mut data.as_slice())
                    .expect("Failed to decode data into `Vec<BlockEvent>`")
            })
    }

    fn set_block_events(&self, block_hash: H256, events: &[BlockEvent]) {
        self.kv.put(
            &KeyPrefix::BlockEvents.two(self.router_address, block_hash),
            events.encode(),
        );
    }

    fn code_blob_info(&self, code_id: CodeId) -> Option<CodeInfo> {
        self.kv
            .get(&KeyPrefix::CodeUploadInfo.two(self.router_address, code_id))
            .map(|data| {
                Decode::decode(&mut data.as_slice()).expect("Failed to decode data into `CodeInfo`")
            })
    }

    fn set_code_blob_info(&self, code_id: CodeId, code_info: CodeInfo) {
        self.kv.put(
            &KeyPrefix::CodeUploadInfo.two(self.router_address, code_id),
            code_info.encode(),
        );
    }

    fn block_is_synced(&self, block_hash: H256) -> bool {
        self.with_small_data(block_hash, |data| data.block_synced)
            .unwrap_or(false)
    }

    fn set_block_is_synced(&self, block_hash: H256) {
        self.mutate_small_data(block_hash, |data| data.block_synced = true);
    }

    fn latest_synced_block_height(&self) -> Option<u32> {
        self.kv
            .get(&KeyPrefix::LatestSyncedBlockHeight.one(self.router_address))
            .map(|data| {
                u32::decode(&mut data.as_slice()).expect("Failed to decode data into `u32`")
            })
    }

    fn set_latest_synced_block_height(&self, height: u32) {
        self.kv.put(
            &KeyPrefix::LatestSyncedBlockHeight.one(self.router_address),
            height.encode(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MemDb;
    use ethexe_common::{events::RouterEvent, tx_pool::RawOffchainTransaction::SendMessage};
    use gear_core::code::InstantiatedSectionSizes;

    #[test]
    fn test_offchain_transaction() {
        let db = Database::new(
            Box::new(MemDb::default()),
            Box::new(MemDb::default()),
            [0; 20],
        );

        let tx = SignedOffchainTransaction {
            signature: Default::default(),
            transaction: OffchainTransaction {
                raw: SendMessage {
                    program_id: H256::random().into(),
                    payload: H256::random().as_bytes().to_vec(),
                },
                reference_block: H256::random(),
            },
        };
        let tx_hash = tx.tx_hash();
        db.set_offchain_transaction(tx.clone());
        assert_eq!(db.get_offchain_transaction(tx_hash), Some(tx));
    }

    #[test]
    fn test_check_within_recent_blocks() {
        let db = Database::new(
            Box::new(MemDb::default()),
            Box::new(MemDb::default()),
            [0; 20],
        );

        let block_hash = H256::random();
        let block_header = BlockHeader::default();
        db.set_block_header(block_hash, block_header.clone());
        db.set_latest_computed_block(block_hash, block_header);

        assert!(db.check_within_recent_blocks(block_hash).unwrap());
    }

    #[test]
    fn test_block_program_states() {
        let db = Database::new(
            Box::new(MemDb::default()),
            Box::new(MemDb::default()),
            [0; 20],
        );

        let block_hash = H256::random();
        let program_states = BTreeMap::new();
        db.set_block_program_states(block_hash, program_states.clone());
        assert_eq!(db.block_program_states(block_hash), Some(program_states));
    }

    #[test]
    fn test_block_outcome() {
        let db = Database::new(
            Box::new(MemDb::default()),
            Box::new(MemDb::default()),
            [0; 20],
        );

        let block_hash = H256::random();
        let block_outcome = vec![StateTransition::default()];
        db.set_block_outcome(block_hash, block_outcome.clone());
        assert_eq!(db.block_outcome(block_hash), Some(block_outcome));
    }

    #[test]
    fn test_block_schedule() {
        let db = Database::new(
            Box::new(MemDb::default()),
            Box::new(MemDb::default()),
            [0; 20],
        );

        let block_hash = H256::random();
        let schedule = Schedule::default();
        db.set_block_schedule(block_hash, schedule.clone());
        assert_eq!(db.block_schedule(block_hash), Some(schedule));
    }

    #[test]
    fn test_latest_computed_block() {
        let db = Database::new(
            Box::new(MemDb::default()),
            Box::new(MemDb::default()),
            [0; 20],
        );

        let block_hash = H256::random();
        let block_header = BlockHeader::default();
        db.set_latest_computed_block(block_hash, block_header.clone());
        assert_eq!(db.latest_computed_block(), Some((block_hash, block_header)));
    }

    #[test]
    fn test_block_events() {
        let db = Database::new(
            Box::new(MemDb::default()),
            Box::new(MemDb::default()),
            [0; 20],
        );

        let block_hash = H256::random();
        let events = vec![BlockEvent::Router(RouterEvent::StorageSlotChanged)];
        db.set_block_events(block_hash, &events);
        assert_eq!(db.block_events(block_hash), Some(events));
    }

    #[test]
    fn test_code_blob_info() {
        let db = Database::new(
            Box::new(MemDb::default()),
            Box::new(MemDb::default()),
            [0; 20],
        );

        let code_id = CodeId::default();
        let code_info = CodeInfo::default();
        db.set_code_blob_info(code_id, code_info.clone());
        assert_eq!(db.code_blob_info(code_id), Some(code_info));
    }

    #[test]
    fn test_block_is_synced() {
        let db = Database::new(
            Box::new(MemDb::default()),
            Box::new(MemDb::default()),
            [0; 20],
        );

        let block_hash = H256::random();
        db.set_block_is_synced(block_hash);
        assert!(db.block_is_synced(block_hash));
    }

    #[test]
    fn test_latest_synced_block_height() {
        let db = Database::new(
            Box::new(MemDb::default()),
            Box::new(MemDb::default()),
            [0; 20],
        );

        let height = 42;
        db.set_latest_synced_block_height(height);
        assert_eq!(db.latest_synced_block_height(), Some(height));
    }

    #[test]
    fn test_original_code() {
        let db = Database::new(
            Box::new(MemDb::default()),
            Box::new(MemDb::default()),
            [0; 20],
        );

        let code = vec![1, 2, 3];
        let code_id = db.set_original_code(&code);
        assert_eq!(db.original_code(code_id), Some(code));
    }

    #[test]
    fn test_program_code_id() {
        let db = Database::new(
            Box::new(MemDb::default()),
            Box::new(MemDb::default()),
            [0; 20],
        );

        let program_id = ProgramId::default();
        let code_id = CodeId::default();
        db.set_program_code_id(program_id, code_id);
        assert_eq!(db.program_code_id(program_id), Some(code_id));
    }

    #[test]
    fn test_instrumented_code() {
        let db = Database::new(
            Box::new(MemDb::default()),
            Box::new(MemDb::default()),
            [0; 20],
        );

        let runtime_id = 1;
        let code_id = CodeId::default();
        let instrumented_code = unsafe {
            InstrumentedCode::new_unchecked(
                vec![1, 2, 3, 4],
                2,
                Default::default(),
                0.into(),
                None,
                InstantiatedSectionSizes::EMPTY,
                1,
            )
        };
        db.set_instrumented_code(runtime_id, code_id, instrumented_code.clone());
        assert_eq!(
            db.instrumented_code(runtime_id, code_id)
                .as_ref()
                .map(|c| c.code()),
            Some(instrumented_code.code())
        );
    }

    #[test]
    fn test_code_valid() {
        let db = Database::new(
            Box::new(MemDb::default()),
            Box::new(MemDb::default()),
            [0; 20],
        );

        let code_id = CodeId::default();
        db.set_code_valid(code_id, true);
        assert_eq!(db.code_valid(code_id), Some(true));
    }

    #[test]
    fn test_block_header() {
        let db = Database::new(
            Box::new(MemDb::default()),
            Box::new(MemDb::default()),
            [0; 20],
        );

        let block_hash = H256::random();
        let block_header = BlockHeader::default();
        db.set_block_header(block_hash, block_header.clone());
        assert_eq!(db.block_header(block_hash), Some(block_header));
    }

    #[test]
    fn test_state() {
        let db = Database::new(
            Box::new(MemDb::default()),
            Box::new(MemDb::default()),
            [0; 20],
        );

        let state = ProgramState::zero();
        let hash = db.write_state(state.clone());
        assert_eq!(db.read_state(hash), Some(state));
    }

    #[test]
    fn test_queue() {
        let db = Database::new(
            Box::new(MemDb::default()),
            Box::new(MemDb::default()),
            [0; 20],
        );

        let queue = MessageQueue::default();
        let hash = db.write_queue(queue.clone());
        assert_eq!(db.read_queue(hash), Some(queue));
    }

    #[test]
    fn test_waitlist() {
        let db = Database::new(
            Box::new(MemDb::default()),
            Box::new(MemDb::default()),
            [0; 20],
        );

        let waitlist = Waitlist::default();
        let hash = db.write_waitlist(waitlist.clone());
        assert_eq!(db.read_waitlist(hash), Some(waitlist));
    }

    #[test]
    fn test_stash() {
        let db = Database::new(
            Box::new(MemDb::default()),
            Box::new(MemDb::default()),
            [0; 20],
        );

        let stash = DispatchStash::default();
        let hash = db.write_stash(stash.clone());
        assert_eq!(db.read_stash(hash), Some(stash));
    }

    #[test]
    fn test_mailbox() {
        let db = Database::new(
            Box::new(MemDb::default()),
            Box::new(MemDb::default()),
            [0; 20],
        );

        let mailbox = Mailbox::default();
        let hash = db.write_mailbox(mailbox.clone());
        assert_eq!(db.read_mailbox(hash), Some(mailbox));
    }

    #[test]
    fn test_pages() {
        let db = Database::new(
            Box::new(MemDb::default()),
            Box::new(MemDb::default()),
            [0; 20],
        );

        let pages = MemoryPages::default();
        let hash = db.write_pages(pages.clone());
        assert_eq!(db.read_pages(hash), Some(pages));
    }

    #[test]
    fn test_pages_region() {
        let db = Database::new(
            Box::new(MemDb::default()),
            Box::new(MemDb::default()),
            [0; 20],
        );

        let pages_region = MemoryPagesRegion::default();
        let hash = db.write_pages_region(pages_region.clone());
        assert_eq!(db.read_pages_region(hash), Some(pages_region));
    }

    #[test]
    fn test_allocations() {
        let db = Database::new(
            Box::new(MemDb::default()),
            Box::new(MemDb::default()),
            [0; 20],
        );

        let allocations = Allocations::default();
        let hash = db.write_allocations(allocations.clone());
        assert_eq!(db.read_allocations(hash), Some(allocations));
    }

    #[test]
    fn test_payload() {
        let db = Database::new(
            Box::new(MemDb::default()),
            Box::new(MemDb::default()),
            [0; 20],
        );

        let payload: Payload = vec![1, 2, 3].try_into().unwrap();
        let hash = db.write_payload(payload.clone());
        assert_eq!(db.read_payload(hash), Some(payload));
    }

    #[test]
    fn test_page_data() {
        let db = Database::new(
            Box::new(MemDb::default()),
            Box::new(MemDb::default()),
            [0; 20],
        );

        let mut page_data = PageBuf::new_zeroed();
        page_data[42] = 42;
        let hash = db.write_page_data(page_data.clone());
        assert_eq!(db.read_page_data(hash), Some(page_data));
    }
}
