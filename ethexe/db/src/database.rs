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
    CASDatabase, KVDatabase, MemDb,
    overlay::{CASOverlay, KVOverlay},
};
use anyhow::{Result, bail};
use ethexe_common::{
    BlockHeader, BlockMeta, CodeBlobInfo, Digest, ProgramStates, Schedule,
    db::{
        BlockMetaStorageRead, BlockMetaStorageWrite, CodesStorageRead, CodesStorageWrite,
        OnChainStorageRead, OnChainStorageWrite,
    },
    events::BlockEvent,
    gear::StateTransition,
    tx_pool::{OffchainTransaction, SignedOffchainTransaction},
};
use ethexe_runtime_common::state::{
    Allocations, DispatchStash, HashOf, Mailbox, MemoryPages, MemoryPagesRegion, MessageQueue,
    ProgramState, Storage, UserMailbox, Waitlist,
};
use gear_core::{
    buffer::Payload,
    code::{CodeMetadata, InstrumentedCode},
    ids::{ActorId, CodeId},
    memory::PageBuf,
};
use gprimitives::H256;
use parity_scale_codec::{Decode, Encode};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[repr(u64)]
enum Key {
    BlockSmallData(H256) = 0,
    BlockEvents(H256) = 1,
    BlockProgramStates(H256) = 2,
    BlockOutcome(H256) = 3,
    BlockSchedule(H256) = 4,

    ProgramToCodeId(ActorId) = 5,
    InstrumentedCode(u32, CodeId) = 6,
    CodeMetadata(CodeId) = 7,
    CodeUploadInfo(CodeId) = 8,
    CodeValid(CodeId) = 9,

    SignedTransaction(H256) = 10,

    LatestComputedBlock = 11,
    LatestSyncedBlockHeight = 12,
}

#[derive(Debug, Encode, Decode)]
enum BlockOutcome {
    Transitions(Vec<StateTransition>),
    /// The actual outcome is not available but it must be considered non-empty.
    ForcedNonEmpty,
}

impl Key {
    fn prefix(&self) -> [u8; 32] {
        // SAFETY: Because `Key` is marked as `#[repr(u64)]` it's actual layout
        // is `#[repr(C)]` and it's first field is a `u64` discriminant. We can read
        // it safely.
        let discriminant = unsafe { <*const _>::from(self).cast::<u64>().read() };
        H256::from_low_u64_be(discriminant).into()
    }

    fn to_bytes(&self) -> Vec<u8> {
        let prefix = self.prefix();
        match self {
            Self::BlockSmallData(hash)
            | Self::BlockEvents(hash)
            | Self::BlockProgramStates(hash)
            | Self::BlockOutcome(hash)
            | Self::BlockSchedule(hash)
            | Self::SignedTransaction(hash) => [prefix.as_ref(), hash.as_ref()].concat(),

            Self::ProgramToCodeId(program_id) => [prefix.as_ref(), program_id.as_ref()].concat(),

            Self::CodeMetadata(code_id)
            | Self::CodeUploadInfo(code_id)
            | Self::CodeValid(code_id) => [prefix.as_ref(), code_id.as_ref()].concat(),

            Self::InstrumentedCode(runtime_id, code_id) => [
                prefix.as_ref(),
                runtime_id.to_le_bytes().as_ref(),
                code_id.as_ref(),
            ]
            .concat(),
            Self::LatestComputedBlock | Self::LatestSyncedBlockHeight => prefix.as_ref().to_vec(),
        }
    }
}

pub struct Database {
    cas: Box<dyn CASDatabase>,
    kv: Box<dyn KVDatabase>,
}

impl Clone for Database {
    fn clone(&self) -> Self {
        Self {
            cas: self.cas.clone_boxed(),
            kv: self.kv.clone_boxed(),
        }
    }
}

impl Database {
    pub fn new(cas: Box<dyn CASDatabase>, kv: Box<dyn KVDatabase>) -> Self {
        Self { cas, kv }
    }

    pub fn from_one<DB: CASDatabase + KVDatabase>(db: &DB) -> Self {
        Self {
            cas: CASDatabase::clone_boxed(db),
            kv: KVDatabase::clone_boxed(db),
        }
    }

    pub fn memory() -> Self {
        let mem = MemDb::default();
        Self::from_one(&mem)
    }

    /// # Safety
    /// Not ready for using in prod. Intended to be for rpc calls only.
    pub unsafe fn overlaid(self) -> Self {
        Self {
            cas: Box::new(CASOverlay::new(self.cas)),
            kv: Box::new(KVOverlay::new(self.kv)),
        }
    }

    pub fn read_by_hash(&self, hash: H256) -> Option<Vec<u8>> {
        self.cas.read(hash)
    }

    pub fn contains_hash(&self, hash: H256) -> bool {
        self.cas.contains(hash)
    }

    pub fn write_hash(&self, data: &[u8]) -> H256 {
        self.cas.write(data)
    }

    pub fn get_offchain_transaction(&self, tx_hash: H256) -> Option<SignedOffchainTransaction> {
        self.kv
            .get(&Key::SignedTransaction(tx_hash).to_bytes())
            .map(|data| {
                Decode::decode(&mut data.as_slice())
                    .expect("failed to data into `SignedTransaction`")
            })
    }

    pub fn set_offchain_transaction(&self, tx: SignedOffchainTransaction) {
        let tx_hash = tx.tx_hash();
        self.kv
            .put(&Key::SignedTransaction(tx_hash).to_bytes(), tx.encode());
    }

    // TODO #4559: test this method
    pub fn check_within_recent_blocks(&self, reference_block_hash: H256) -> Result<bool> {
        let Some((latest_computed_block_hash, latest_computed_block_header)) =
            self.latest_computed_block()
        else {
            bail!("No latest valid block found");
        };
        let Some(reference_block_header) = self.block_header(reference_block_hash) else {
            bail!("No reference block found");
        };

        // If reference block is far away from the latest valid block, it's not in the window.
        let Some(actual_window) = latest_computed_block_header
            .height
            .checked_sub(reference_block_header.height)
        else {
            bail!(
                "Can't calculate actual window: reference block hash doesn't suit actual blocks state"
            );
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

            let Some(block_header) = self.block_header(block_hash) else {
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
            .get(&Key::BlockSmallData(block_hash).to_bytes())
            .map(|data| {
                BlockSmallData::decode(&mut data.as_slice())
                    .expect("Failed to decode data into `BlockSmallMetaInfo`")
            })
    }

    fn set_block_small_data(&self, block_hash: H256, meta: BlockSmallData) {
        self.kv
            .put(&Key::BlockSmallData(block_hash).to_bytes(), meta.encode());
    }

    fn block_outcome_inner(&self, block_hash: H256) -> Option<BlockOutcome> {
        self.kv
            .get(&Key::BlockOutcome(block_hash).to_bytes())
            .map(|data| {
                BlockOutcome::decode(&mut data.as_slice())
                    .expect("Failed to decode data into `Vec<StateTransition>`")
            })
    }

    /// # Safety
    ///
    /// If the block is actually empty but forced to be not, then database invariants are violated.
    pub unsafe fn set_non_empty_block_outcome(&self, block_hash: H256) {
        log::trace!("For block {block_hash} set non-empty outcome");
        self.kv.put(
            &Key::BlockOutcome(block_hash).to_bytes(),
            BlockOutcome::ForcedNonEmpty.encode(),
        );
    }
}

#[derive(Debug, Clone, Default, Encode, Decode, PartialEq, Eq)]
struct BlockSmallData {
    block_header: Option<BlockHeader>,
    meta: BlockMeta,
    prev_not_empty_block: Option<H256>,
    last_committed_batch: Option<Digest>,
    commitment_queue: Option<VecDeque<H256>>,
    codes_queue: Option<VecDeque<CodeId>>,
}

impl BlockMetaStorageRead for Database {
    fn block_meta(&self, block_hash: H256) -> BlockMeta {
        self.with_small_data(block_hash, |data| data.meta)
            .unwrap_or_default()
    }

    fn block_commitment_queue(&self, block_hash: H256) -> Option<VecDeque<H256>> {
        self.with_small_data(block_hash, |data| data.commitment_queue)?
    }

    fn block_codes_queue(&self, block_hash: H256) -> Option<VecDeque<CodeId>> {
        self.with_small_data(block_hash, |data| data.codes_queue)?
    }

    fn previous_not_empty_block(&self, block_hash: H256) -> Option<H256> {
        self.with_small_data(block_hash, |data| data.prev_not_empty_block)?
    }

    fn last_committed_batch(&self, block_hash: H256) -> Option<Digest> {
        self.with_small_data(block_hash, |data| data.last_committed_batch)?
    }

    fn block_program_states(&self, block_hash: H256) -> Option<ProgramStates> {
        self.kv
            .get(&Key::BlockProgramStates(block_hash).to_bytes())
            .map(|data| {
                BTreeMap::decode(&mut data.as_slice())
                    .expect("Failed to decode data into `BTreeMap`")
            })
    }

    fn block_outcome(&self, block_hash: H256) -> Option<Vec<StateTransition>> {
        self.block_outcome_inner(block_hash)
            .map(|outcome| match outcome {
                BlockOutcome::Transitions(transitions) => transitions,
                BlockOutcome::ForcedNonEmpty => {
                    panic!("`block_outcome()` called on forced non-empty block {block_hash}")
                }
            })
    }

    fn block_outcome_is_empty(&self, block_hash: H256) -> Option<bool> {
        self.block_outcome_inner(block_hash)
            .map(|outcome| match outcome {
                BlockOutcome::Transitions(transitions) => transitions.is_empty(),
                BlockOutcome::ForcedNonEmpty => false,
            })
    }

    fn block_schedule(&self, block_hash: H256) -> Option<Schedule> {
        self.kv
            .get(&Key::BlockSchedule(block_hash).to_bytes())
            .map(|data| {
                Schedule::decode(&mut data.as_slice())
                    .expect("Failed to decode data into `BTreeMap`")
            })
    }

    fn latest_computed_block(&self) -> Option<(H256, BlockHeader)> {
        self.kv
            .get(&Key::LatestComputedBlock.to_bytes())
            .map(|data| {
                <(H256, BlockHeader)>::decode(&mut data.as_slice())
                    .expect("Failed to decode data into `(H256, BlockHeader)`")
            })
    }
}

impl BlockMetaStorageWrite for Database {
    fn mutate_block_meta<F>(&self, block_hash: H256, f: F)
    where
        F: FnOnce(&mut BlockMeta),
    {
        log::trace!("For block {block_hash} mutate meta");
        self.mutate_small_data(block_hash, |data| {
            f(&mut data.meta);
        });
    }

    fn set_block_commitment_queue(&self, block_hash: H256, queue: VecDeque<H256>) {
        log::trace!("For block {block_hash} set commitment queue: {queue:?}");
        self.mutate_small_data(block_hash, |data| data.commitment_queue = Some(queue));
    }

    fn set_block_codes_queue(&self, block_hash: H256, queue: VecDeque<CodeId>) {
        log::trace!("For block {block_hash} set codes queue: {queue:?}");
        self.mutate_small_data(block_hash, |data| data.codes_queue = Some(queue));
    }

    fn set_previous_not_empty_block(&self, block_hash: H256, prev_not_empty_block_hash: H256) {
        log::trace!("For block {block_hash} set prev commitment: {prev_not_empty_block_hash}");
        self.mutate_small_data(block_hash, |data| {
            data.prev_not_empty_block = Some(prev_not_empty_block_hash)
        });
    }

    fn set_last_committed_batch(&self, block_hash: H256, batch: Digest) {
        log::trace!("For block {block_hash} set last committed batch: {batch:?}");
        self.mutate_small_data(block_hash, |data| data.last_committed_batch = Some(batch));
    }

    fn set_block_program_states(&self, block_hash: H256, map: ProgramStates) {
        log::trace!("For block {block_hash} set program states: {map:?}");
        self.kv.put(
            &Key::BlockProgramStates(block_hash).to_bytes(),
            map.encode(),
        );
    }

    fn set_block_outcome(&self, block_hash: H256, outcome: Vec<StateTransition>) {
        log::trace!("For block {block_hash} set outcome: {outcome:?}");
        self.kv.put(
            &Key::BlockOutcome(block_hash).to_bytes(),
            BlockOutcome::Transitions(outcome).encode(),
        );
    }

    fn set_block_schedule(&self, block_hash: H256, map: Schedule) {
        log::trace!("For block {block_hash} set schedule: {map:?}");
        self.kv
            .put(&Key::BlockSchedule(block_hash).to_bytes(), map.encode());
    }

    fn set_latest_computed_block(&self, block_hash: H256, header: BlockHeader) {
        log::trace!("Set latest computed block: {block_hash} {header:?}");
        self.kv.put(
            &Key::LatestComputedBlock.to_bytes(),
            (block_hash, header).encode(),
        );
    }
}

impl CodesStorageRead for Database {
    fn original_code_exists(&self, code_id: CodeId) -> bool {
        self.kv.contains(code_id.as_ref())
    }

    fn original_code(&self, code_id: CodeId) -> Option<Vec<u8>> {
        self.cas.read(code_id.into())
    }

    fn program_code_id(&self, program_id: ActorId) -> Option<CodeId> {
        self.kv
            .get(&Key::ProgramToCodeId(program_id).to_bytes())
            .map(|data| {
                CodeId::try_from(data.as_slice()).expect("Failed to decode data into `CodeId`")
            })
    }

    fn instrumented_code_exists(&self, runtime_id: u32, code_id: CodeId) -> bool {
        self.kv
            .contains(&Key::InstrumentedCode(runtime_id, code_id).to_bytes())
    }

    fn instrumented_code(&self, runtime_id: u32, code_id: CodeId) -> Option<InstrumentedCode> {
        self.kv
            .get(&Key::InstrumentedCode(runtime_id, code_id).to_bytes())
            .map(|data| {
                Decode::decode(&mut data.as_slice())
                    .expect("Failed to decode data into `InstrumentedCode`")
            })
    }

    fn code_metadata(&self, code_id: CodeId) -> Option<CodeMetadata> {
        self.kv
            .get(&Key::CodeMetadata(code_id).to_bytes())
            .map(|data| {
                CodeMetadata::decode(&mut data.as_slice())
                    .expect("Failed to decode data into `CodeMetadata`")
            })
    }

    fn code_valid(&self, code_id: CodeId) -> Option<bool> {
        self.kv
            .get(&Key::CodeValid(code_id).to_bytes())
            .map(|data| {
                bool::decode(&mut data.as_slice()).expect("Failed to decode data into `bool`")
            })
    }
}

impl CodesStorageWrite for Database {
    fn set_original_code(&self, code: &[u8]) -> CodeId {
        self.cas.write(code).into()
    }

    fn set_program_code_id(&self, program_id: ActorId, code_id: CodeId) {
        self.kv.put(
            &Key::ProgramToCodeId(program_id).to_bytes(),
            code_id.into_bytes().to_vec(),
        );
    }

    fn set_instrumented_code(&self, runtime_id: u32, code_id: CodeId, code: InstrumentedCode) {
        self.kv.put(
            &Key::InstrumentedCode(runtime_id, code_id).to_bytes(),
            code.encode(),
        );
    }

    fn set_code_metadata(&self, code_id: CodeId, code_metadata: CodeMetadata) {
        self.kv.put(
            &Key::CodeMetadata(code_id).to_bytes(),
            code_metadata.encode(),
        );
    }

    fn set_code_valid(&self, code_id: CodeId, valid: bool) {
        self.kv
            .put(&Key::CodeValid(code_id).to_bytes(), valid.encode());
    }

    fn valid_codes(&self) -> BTreeSet<CodeId> {
        let key_prefix = Key::CodeValid(Default::default()).prefix();
        self.kv
            .iter_prefix(&key_prefix)
            .map(|(key, valid)| {
                let (split_key_prefix, code_id) = key.split_at(key_prefix.len());
                debug_assert_eq!(split_key_prefix, key_prefix);
                let code_id =
                    CodeId::try_from(code_id).expect("Failed to decode key into `CodeId`");

                let valid =
                    bool::decode(&mut valid.as_slice()).expect("Failed to decode data into `bool`");

                (code_id, valid)
            })
            .filter_map(|(code_id, valid)| valid.then_some(code_id))
            .collect()
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

    fn read_user_mailbox(&self, hash: HashOf<UserMailbox>) -> Option<UserMailbox> {
        self.cas.read(hash.hash()).map(|data| {
            UserMailbox::decode(&mut data.as_slice())
                .expect("Failed to decode data into `UserMailbox`")
        })
    }

    fn write_user_mailbox(&self, use_mailbox: UserMailbox) -> HashOf<UserMailbox> {
        unsafe { HashOf::new(self.cas.write(&use_mailbox.encode())) }
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

impl OnChainStorageRead for Database {
    fn block_header(&self, block_hash: H256) -> Option<BlockHeader> {
        self.with_small_data(block_hash, |data| data.block_header)?
    }

    fn block_events(&self, block_hash: H256) -> Option<Vec<BlockEvent>> {
        self.kv
            .get(&Key::BlockEvents(block_hash).to_bytes())
            .map(|data| {
                Vec::<BlockEvent>::decode(&mut data.as_slice())
                    .expect("Failed to decode data into `Vec<BlockEvent>`")
            })
    }

    fn code_blob_info(&self, code_id: CodeId) -> Option<CodeBlobInfo> {
        self.kv
            .get(&Key::CodeUploadInfo(code_id).to_bytes())
            .map(|data| {
                Decode::decode(&mut data.as_slice())
                    .expect("Failed to decode data into `CodeBlobInfo`")
            })
    }

    fn latest_synced_block_height(&self) -> Option<u32> {
        self.kv
            .get(&Key::LatestSyncedBlockHeight.to_bytes())
            .map(|data| {
                u32::decode(&mut data.as_slice()).expect("Failed to decode data into `u32`")
            })
    }
}

impl OnChainStorageWrite for Database {
    fn set_block_header(&self, block_hash: H256, header: BlockHeader) {
        self.mutate_small_data(block_hash, |data| data.block_header = Some(header));
    }

    fn set_block_events(&self, block_hash: H256, events: &[BlockEvent]) {
        self.kv
            .put(&Key::BlockEvents(block_hash).to_bytes(), events.encode());
    }

    fn set_code_blob_info(&self, code_id: CodeId, code_info: CodeBlobInfo) {
        self.kv
            .put(&Key::CodeUploadInfo(code_id).to_bytes(), code_info.encode());
    }

    fn set_latest_synced_block_height(&self, height: u32) {
        self.kv
            .put(&Key::LatestSyncedBlockHeight.to_bytes(), height.encode());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ethexe_common::{
        ecdsa::PrivateKey, events::RouterEvent, tx_pool::RawOffchainTransaction::SendMessage,
    };
    use gear_core::code::{InstantiatedSectionSizes, InstrumentationStatus};

    #[test]
    fn test_offchain_transaction() {
        let db = Database::memory();

        let private_key = PrivateKey::from([1; 32]);
        let tx = SignedOffchainTransaction::create(
            private_key,
            OffchainTransaction {
                raw: SendMessage {
                    program_id: H256::random().into(),
                    payload: H256::random().0.to_vec(),
                },
                reference_block: H256::random(),
            },
        )
        .unwrap();
        let tx_hash = tx.tx_hash();
        db.set_offchain_transaction(tx.clone());
        assert_eq!(db.get_offchain_transaction(tx_hash), Some(tx));
    }

    #[test]
    fn check_within_recent_blocks_scenarios() {
        const WINDOW_SIZE: u32 = OffchainTransaction::BLOCK_HASHES_WINDOW_SIZE;
        const BASE_HEIGHT: u32 = 100;

        // --- Success: Latest Block ---
        {
            println!("Scenario: Success - Latest Block");
            let db = Database::memory();
            let block_hash = H256::random();
            let block_header = BlockHeader {
                height: BASE_HEIGHT,
                ..Default::default()
            };
            db.set_block_header(block_hash, block_header.clone());
            db.set_latest_computed_block(block_hash, block_header);
            assert!(db.check_within_recent_blocks(block_hash).unwrap());
        }

        // --- Success: Within Window ---
        {
            println!("Scenario: Success - Within Window");
            let db = Database::memory();
            let mut current_hash = H256::random();
            let mut current_header = BlockHeader {
                height: BASE_HEIGHT + WINDOW_SIZE,
                ..Default::default()
            };
            db.set_latest_computed_block(current_hash, current_header.clone());

            let mut history = vec![(current_hash, current_header.clone())];

            // Build history within the window
            for i in 0..WINDOW_SIZE {
                let parent_hash = H256::random();
                current_header.parent_hash = parent_hash;
                db.set_block_header(current_hash, current_header.clone());
                history.push((current_hash, current_header.clone()));

                current_hash = parent_hash;
                current_header = BlockHeader {
                    height: BASE_HEIGHT + WINDOW_SIZE - 1 - i,
                    ..Default::default()
                };
            }
            // Oldest in window
            db.set_block_header(current_hash, current_header.clone());
            history.push((current_hash, current_header.clone()));

            // Check block near the end of the window
            let reference_block_hash_mid = history[WINDOW_SIZE as usize - 5].0;
            assert!(
                db.check_within_recent_blocks(reference_block_hash_mid)
                    .unwrap()
            );

            // Check block at the edge of the window
            // Block at BASE_HEIGHT
            let reference_block_hash_edge = history[WINDOW_SIZE as usize].0;
            assert!(
                db.check_within_recent_blocks(reference_block_hash_edge)
                    .unwrap()
            );
        }

        // --- Fail: Outside Window ---
        {
            println!("Scenario: Fail - Outside Window");
            let db = Database::memory();
            let mut current_hash = H256::random();
            // One block beyond the window
            let mut current_header = BlockHeader {
                height: BASE_HEIGHT + WINDOW_SIZE + 1,
                parent_hash: H256::random(),
                ..Default::default()
            };
            db.set_latest_computed_block(current_hash, current_header.clone());

            let mut reference_block_hash = H256::zero();

            // Build history
            for i in 0..(WINDOW_SIZE + 1) {
                let parent_hash = H256::random();
                current_header.parent_hash = parent_hash;
                db.set_block_header(current_hash, current_header.clone());

                // This is the block just outside the window (height BASE_HEIGHT)
                if i == WINDOW_SIZE {
                    reference_block_hash = current_hash;
                }

                current_hash = parent_hash;
                current_header = BlockHeader {
                    height: BASE_HEIGHT + WINDOW_SIZE - i,
                    parent_hash: H256::random(),
                    ..Default::default()
                };
            }
            // Oldest block
            db.set_block_header(current_hash, current_header);

            assert!(!db.check_within_recent_blocks(reference_block_hash).unwrap());
        }

        // --- Fail: Reorg ---
        {
            println!("Scenario: Fail - Reorg");
            let db = Database::memory();
            let mut current_hash = H256::random();
            let mut current_header = BlockHeader {
                height: BASE_HEIGHT + WINDOW_SIZE,
                parent_hash: H256::random(),
                ..Default::default()
            };
            db.set_latest_computed_block(current_hash, current_header.clone());

            // Build canonical chain history
            for i in 0..WINDOW_SIZE {
                let parent_hash = H256::random();
                current_header.parent_hash = parent_hash;
                db.set_block_header(current_hash, current_header.clone());

                current_hash = parent_hash;
                current_header = BlockHeader {
                    height: BASE_HEIGHT + WINDOW_SIZE - 1 - i,
                    parent_hash: H256::random(),
                    ..Default::default()
                };
            }
            // Oldest canonical block
            db.set_block_header(current_hash, current_header.clone());

            // Create a fork (reference block not on the canonical chain)
            let fork_block_hash = H256::random();
            // Within height window
            // Different parent
            let fork_block_header = BlockHeader {
                height: BASE_HEIGHT + 1,
                parent_hash: H256::random(),
                ..Default::default()
            };
            db.set_block_header(fork_block_hash, fork_block_header);

            assert!(!db.check_within_recent_blocks(fork_block_hash).unwrap());
        }

        // --- Error: No Latest Block ---
        {
            println!("Scenario: Error - No Latest Block");
            let db = Database::memory();
            let reference_block_hash = H256::random();
            let result = db.check_within_recent_blocks(reference_block_hash);
            assert!(result.is_err());
            assert!(
                result
                    .unwrap_err()
                    .to_string()
                    .contains("No latest valid block found")
            );
        }

        // --- Error: No Reference Block ---
        {
            println!("Scenario: Error - No Reference Block");
            let db = Database::memory();
            let latest_hash = H256::random();
            let latest_header = BlockHeader {
                height: BASE_HEIGHT,
                ..Default::default()
            };
            db.set_latest_computed_block(latest_hash, latest_header.clone());
            // Need the latest header itself
            db.set_block_header(latest_hash, latest_header);

            // This block doesn't exist
            let reference_block_hash = H256::random();
            let result = db.check_within_recent_blocks(reference_block_hash);
            assert!(result.is_err());
            assert!(
                result
                    .unwrap_err()
                    .to_string()
                    .contains("No reference block found")
            );
        }

        // --- Error: Missing History ---
        {
            println!("Scenario: Error - Missing History");
            let db = Database::memory();
            let latest_hash = H256::random();
            let missing_parent_hash = H256::random();
            // This parent won't be in the DB
            let latest_header = BlockHeader {
                height: BASE_HEIGHT + WINDOW_SIZE,
                parent_hash: missing_parent_hash,
                ..Default::default()
            };
            db.set_latest_computed_block(latest_hash, latest_header.clone());
            // Add latest block header
            db.set_block_header(latest_hash, latest_header);

            let reference_block_hash = H256::random();
            // Within height range
            let reference_header = BlockHeader {
                height: BASE_HEIGHT,
                parent_hash: H256::random(),
                ..Default::default()
            };
            // Add reference block header
            db.set_block_header(reference_block_hash, reference_header);

            let result = db.check_within_recent_blocks(reference_block_hash);
            assert!(result.is_err());
            assert!(
                result
                    .unwrap_err()
                    .to_string()
                    .contains("not found in the window")
            );
        }
    }

    #[test]
    fn test_block_program_states() {
        let db = Database::memory();

        let block_hash = H256::random();
        let program_states = BTreeMap::new();
        db.set_block_program_states(block_hash, program_states.clone());
        assert_eq!(db.block_program_states(block_hash), Some(program_states));
    }

    #[test]
    fn test_block_outcome() {
        let db = Database::memory();

        let block_hash = H256::random();
        let block_outcome = vec![StateTransition::default()];
        db.set_block_outcome(block_hash, block_outcome.clone());
        assert_eq!(db.block_outcome(block_hash), Some(block_outcome));
    }

    #[test]
    fn test_block_schedule() {
        let db = Database::memory();

        let block_hash = H256::random();
        let schedule = Schedule::default();
        db.set_block_schedule(block_hash, schedule.clone());
        assert_eq!(db.block_schedule(block_hash), Some(schedule));
    }

    #[test]
    fn test_latest_computed_block() {
        let db = Database::memory();

        let block_hash = H256::random();
        let block_header = BlockHeader::default();
        db.set_latest_computed_block(block_hash, block_header.clone());
        assert_eq!(db.latest_computed_block(), Some((block_hash, block_header)));
    }

    #[test]
    fn test_block_events() {
        let db = Database::memory();

        let block_hash = H256::random();
        let events = vec![BlockEvent::Router(RouterEvent::StorageSlotChanged)];
        db.set_block_events(block_hash, &events);
        assert_eq!(db.block_events(block_hash), Some(events));
    }

    #[test]
    fn test_code_blob_info() {
        let db = Database::memory();

        let code_id = CodeId::default();
        let code_info = CodeBlobInfo::default();
        db.set_code_blob_info(code_id, code_info.clone());
        assert_eq!(db.code_blob_info(code_id), Some(code_info));
    }

    #[test]
    fn test_block_is_synced() {
        let db = Database::memory();

        let block_hash = H256::random();
        db.mutate_block_meta(block_hash, |meta| meta.synced = true);
        assert!(db.block_meta(block_hash).synced);
    }

    #[test]
    fn test_latest_synced_block_height() {
        let db = Database::memory();

        let height = 42;
        db.set_latest_synced_block_height(height);
        assert_eq!(db.latest_synced_block_height(), Some(height));
    }

    #[test]
    fn test_original_code() {
        let db = Database::memory();

        let code = vec![1, 2, 3];
        let code_id = db.set_original_code(&code);
        assert_eq!(db.original_code(code_id), Some(code));
    }

    #[test]
    fn test_program_code_id() {
        let db = Database::memory();

        let program_id = ActorId::default();
        let code_id = CodeId::default();
        db.set_program_code_id(program_id, code_id);
        assert_eq!(db.program_code_id(program_id), Some(code_id));
    }

    #[test]
    fn test_instrumented_code() {
        let db = Database::memory();

        let runtime_id = 1;
        let code_id = CodeId::default();
        let section_sizes = InstantiatedSectionSizes::new(0, 0, 0, 0, 0, 0);
        let instrumented_code = InstrumentedCode::new(vec![1, 2, 3, 4], section_sizes);
        db.set_instrumented_code(runtime_id, code_id, instrumented_code.clone());
        assert_eq!(
            db.instrumented_code(runtime_id, code_id)
                .as_ref()
                .map(|c| c.bytes()),
            Some(instrumented_code.bytes())
        );
    }

    #[test]
    fn test_code_metadata() {
        let db = Database::memory();

        let code_id = CodeId::default();
        let code_metadata = CodeMetadata::new(
            1,
            Default::default(),
            0.into(),
            None,
            InstrumentationStatus::Instrumented {
                version: 3,
                code_len: 2,
            },
        );
        db.set_code_metadata(code_id, code_metadata.clone());
        assert_eq!(
            db.code_metadata(code_id)
                .as_ref()
                .map(|m| m.original_code_len()),
            Some(code_metadata.original_code_len())
        );
        assert_eq!(
            db.code_metadata(code_id)
                .as_ref()
                .map(|m| m.instrumented_code_len()),
            Some(code_metadata.instrumented_code_len())
        );
        assert_eq!(
            db.code_metadata(code_id)
                .as_ref()
                .map(|m| m.instrumentation_status()),
            Some(code_metadata.instrumentation_status())
        );
        assert_eq!(
            db.code_metadata(code_id)
                .as_ref()
                .map(|m| m.instruction_weights_version()),
            Some(code_metadata.instruction_weights_version())
        );
    }

    #[test]
    fn test_code_valid() {
        let db = Database::memory();

        let code_id = CodeId::default();
        db.set_code_valid(code_id, true);
        assert_eq!(db.code_valid(code_id), Some(true));
    }

    #[test]
    fn test_block_header() {
        let db = Database::memory();

        let block_hash = H256::random();
        let block_header = BlockHeader::default();
        db.set_block_header(block_hash, block_header.clone());
        assert_eq!(db.block_header(block_hash), Some(block_header));
    }

    #[test]
    fn test_state() {
        let db = Database::memory();

        let state = ProgramState::zero();
        let hash = db.write_state(state.clone());
        assert_eq!(db.read_state(hash), Some(state));
    }

    #[test]
    fn test_queue() {
        let db = Database::memory();

        let queue = MessageQueue::default();
        let hash = db.write_queue(queue.clone());
        assert_eq!(db.read_queue(hash), Some(queue));
    }

    #[test]
    fn test_waitlist() {
        let db = Database::memory();

        let waitlist = Waitlist::default();
        let hash = db.write_waitlist(waitlist.clone());
        assert_eq!(db.read_waitlist(hash), Some(waitlist));
    }

    #[test]
    fn test_stash() {
        let db = Database::memory();

        let stash = DispatchStash::default();
        let hash = db.write_stash(stash.clone());
        assert_eq!(db.read_stash(hash), Some(stash));
    }

    #[test]
    fn test_mailbox() {
        let db = Database::memory();

        let mailbox = Mailbox::default();
        let hash = db.write_mailbox(mailbox.clone());
        assert_eq!(db.read_mailbox(hash), Some(mailbox));
    }

    #[test]
    fn test_pages() {
        let db = Database::memory();

        let pages = MemoryPages::default();
        let hash = db.write_pages(pages.clone());
        assert_eq!(db.read_pages(hash), Some(pages));
    }

    #[test]
    fn test_pages_region() {
        let db = Database::memory();

        let pages_region = MemoryPagesRegion::default();
        let hash = db.write_pages_region(pages_region.clone());
        assert_eq!(db.read_pages_region(hash), Some(pages_region));
    }

    #[test]
    fn test_allocations() {
        let db = Database::memory();

        let allocations = Allocations::default();
        let hash = db.write_allocations(allocations.clone());
        assert_eq!(db.read_allocations(hash), Some(allocations));
    }

    #[test]
    fn test_payload() {
        let db = Database::memory();

        let payload: Payload = vec![1, 2, 3].try_into().unwrap();
        let hash = db.write_payload(payload.clone());
        assert_eq!(db.read_payload(hash), Some(payload));
    }

    #[test]
    fn test_page_data() {
        let db = Database::memory();

        let mut page_data = PageBuf::new_zeroed();
        page_data[42] = 42;
        let hash = db.write_page_data(page_data.clone());
        assert_eq!(db.read_page_data(hash), Some(page_data));
    }
}
