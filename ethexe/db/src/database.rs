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
    CASDatabase, KVDatabase,
    overlay::{CASOverlay, KVOverlay},
};
use anyhow::Result;
use delegate::delegate;
use ethexe_common::{
    Announce, BlockHeader, CodeBlobInfo, HashOf, ProgramStates, Schedule, ValidatorsVec,
    db::{
        AnnounceMeta, AnnounceStorageRO, AnnounceStorageRW, BlockMeta, BlockMetaStorageRO,
        BlockMetaStorageRW, CodesStorageRO, CodesStorageRW, ConfigStorageRO, DBConfig, DBGlobals,
        GlobalsStorageRO, GlobalsStorageRW, HashStorageRO, InjectedStorageRO, InjectedStorageRW,
        OnChainStorageRO, OnChainStorageRW,
    },
    events::BlockEvent,
    gear::StateTransition,
    injected::{InjectedTransaction, SignedInjectedTransaction},
};
use ethexe_runtime_common::state::{
    Allocations, DispatchStash, Mailbox, MemoryPages, MemoryPagesRegion, MessageQueue,
    ProgramState, Storage, UserMailbox, Waitlist,
};
use gear_core::{
    buffer::Payload,
    code::{CodeMetadata, InstrumentedCode},
    ids::{ActorId, CodeId, prelude::CodeIdExt as _},
    memory::PageBuf,
};
use gprimitives::H256;
use parity_scale_codec::{Decode, Encode};
use std::{
    collections::BTreeSet,
    sync::{Arc, RwLock, RwLockReadGuard},
};

pub const VERSION: u32 = 1;

#[repr(u64)]
enum Key {
    // TODO (kuzmindev): use `HashOf<T>` here
    BlockSmallData(H256) = 0,
    BlockEvents(H256) = 1,

    ValidatorSet(u64) = 2,

    AnnounceProgramStates(HashOf<Announce>) = 3,
    AnnounceOutcome(HashOf<Announce>) = 4,
    AnnounceSchedule(HashOf<Announce>) = 5,
    AnnounceMeta(HashOf<Announce>) = 6,

    ProgramToCodeId(ActorId) = 7,
    InstrumentedCode(u32, CodeId) = 8,
    CodeMetadata(CodeId) = 9,
    CodeUploadInfo(CodeId) = 10,
    CodeValid(CodeId) = 11,

    InjectedTransaction(HashOf<InjectedTransaction>) = 12,

    // TODO kuzmindev: make keys prefixes consistent. We don't change it to avoid corrupting existing key layout.
    Globals = 14,
    Config = 15,

    // TODO kuzmindev: temporal solution - must move into block meta or something else.
    LatestEraValidatorsCommitted(H256),
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
        // Pre-allocate enough space for the largest possible key.
        let mut bytes = Vec::with_capacity(2 * size_of::<H256>() + size_of::<u32>());
        bytes.extend(self.prefix());

        match self {
            Self::BlockSmallData(hash)
            | Self::BlockEvents(hash)
            | Self::LatestEraValidatorsCommitted(hash) => bytes.extend(hash.as_ref()),

            Self::ValidatorSet(era_index) => {
                bytes.extend(era_index.to_le_bytes());
            }

            Self::AnnounceProgramStates(hash)
            | Self::AnnounceOutcome(hash)
            | Self::AnnounceSchedule(hash)
            | Self::AnnounceMeta(hash) => bytes.extend(hash.as_ref()),

            Self::InjectedTransaction(hash) => bytes.extend(hash.as_ref()),

            Self::ProgramToCodeId(program_id) => bytes.extend(program_id.as_ref()),

            Self::CodeMetadata(code_id)
            | Self::CodeUploadInfo(code_id)
            | Self::CodeValid(code_id) => bytes.extend(code_id.as_ref()),

            Self::InstrumentedCode(runtime_id, code_id) => {
                bytes.extend(runtime_id.to_le_bytes());
                bytes.extend(code_id.as_ref());
            }
            Self::Globals | Self::Config => {
                // append additional zero bytes to avoid intersection with CAS
                bytes.extend([0; 8])
            }
        };

        debug_assert!(
            bytes.len() > size_of::<H256>(),
            "Key must be longer than H256, to avoid collision with CAS keys"
        );
        debug_assert!(
            bytes.len() <= 2 * size_of::<H256>() + size_of::<u32>(),
            "Key must not be longer than maximum possible length"
        );

        bytes
    }
}

impl dyn KVDatabase {
    pub fn config(&self) -> Result<Option<DBConfig>> {
        self.get(&Key::Config.to_bytes())
            .map(|data| DBConfig::decode(&mut data.as_ref()).map_err(Into::into))
            .transpose()
    }

    pub fn globals(&self) -> Result<DBGlobals> {
        DBGlobals::decode(
            &mut self
                .get(&Key::Globals.to_bytes())
                .expect("Database globals not found")
                .as_ref(),
        )
        .map_err(Into::into)
    }

    pub fn set_config(&self, config: DBConfig) {
        self.put(&Key::Config.to_bytes(), config.encode());
    }

    pub fn set_globals(&self, globals: DBGlobals) {
        self.put(&Key::Globals.to_bytes(), globals.encode());
    }
}

pub struct DatabaseRef<'a, 'b> {
    pub cas: &'a dyn CASDatabase,
    pub kv: &'b dyn KVDatabase,
}

impl<'a, 'b> DatabaseRef<'a, 'b> {
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

    fn with_small_data<R>(
        &self,
        block_hash: H256,
        f: impl FnOnce(BlockSmallData) -> R,
    ) -> Option<R> {
        self.block_small_data(block_hash).map(f)
    }

    /// Mutates `BlockSmallData` for the given block hash.
    ///
    /// If data wasn't found, it will be created with default values and then mutated.
    fn mutate_small_data(&self, block_hash: H256, f: impl FnOnce(&mut BlockSmallData)) {
        let mut data = self.block_small_data(block_hash).unwrap_or_default();
        f(&mut data);
        self.set_block_small_data(block_hash, data);
    }

    pub fn config(&self) -> Result<Option<DBConfig>> {
        self.kv
            .get(&Key::Config.to_bytes())
            .map(|data| DBConfig::decode(&mut data.as_ref()).map_err(Into::into))
            .transpose()
    }

    pub fn globals(&self) -> Result<DBGlobals> {
        DBGlobals::decode(
            &mut self
                .kv
                .get(&Key::Globals.to_bytes())
                .expect("Database globals not found")
                .as_ref(),
        )
        .map_err(Into::into)
    }

    pub fn set_config(&self, config: DBConfig) {
        self.kv.put(&Key::Config.to_bytes(), config.encode());
    }

    pub fn set_globals(&self, globals: DBGlobals) {
        self.kv.put(&Key::Globals.to_bytes(), globals.encode());
    }
}

impl<'a, 'b> AnnounceStorageRO for DatabaseRef<'a, 'b> {
    fn announce(&self, hash: HashOf<Announce>) -> Option<Announce> {
        self.cas.read(hash.inner()).map(|data| {
            Announce::decode(&mut &data[..]).expect("Failed to decode data into `Announce`")
        })
    }

    fn announce_program_states(&self, announce_hash: HashOf<Announce>) -> Option<ProgramStates> {
        self.kv
            .get(&Key::AnnounceProgramStates(announce_hash).to_bytes())
            .map(|data| {
                ProgramStates::decode(&mut data.as_slice())
                    .expect("Failed to decode data into `ProgramStates`")
            })
    }

    fn announce_outcome(&self, announce_hash: HashOf<Announce>) -> Option<Vec<StateTransition>> {
        self.kv
            .get(&Key::AnnounceOutcome(announce_hash).to_bytes())
            .map(|data| {
                Vec::<StateTransition>::decode(&mut data.as_slice())
                    .expect("Failed to decode data into `Vec<StateTransition>`")
            })
    }

    fn announce_schedule(&self, announce_hash: HashOf<Announce>) -> Option<Schedule> {
        self.kv
            .get(&Key::AnnounceSchedule(announce_hash).to_bytes())
            .map(|data| {
                Schedule::decode(&mut data.as_slice())
                    .expect("Failed to decode data into `Schedule`")
            })
    }

    fn announce_meta(&self, announce_hash: HashOf<Announce>) -> AnnounceMeta {
        self.kv
            .get(&Key::AnnounceMeta(announce_hash).to_bytes())
            .map(|data| {
                AnnounceMeta::decode(&mut data.as_slice())
                    .expect("Failed to decode data into `AnnounceMeta`")
            })
            .unwrap_or_default()
    }
}

impl<'a, 'b> AnnounceStorageRW for DatabaseRef<'a, 'b> {
    fn set_announce(&self, announce: Announce) -> HashOf<Announce> {
        tracing::trace!(announce_hash = %announce.to_hash(), announce = ?announce, "Set announce");
        unsafe { HashOf::new(self.cas.write(&announce.encode())) }
    }

    fn set_announce_program_states(
        &self,
        announce_hash: HashOf<Announce>,
        program_states: ProgramStates,
    ) {
        tracing::trace!(announce_hash = %announce_hash, "Set announce program states");
        self.kv.put(
            &Key::AnnounceProgramStates(announce_hash).to_bytes(),
            program_states.encode(),
        );
    }

    fn set_announce_outcome(&self, announce_hash: HashOf<Announce>, outcome: Vec<StateTransition>) {
        tracing::trace!(announce_hash = %announce_hash, "Set announce outcome");
        self.kv.put(
            &Key::AnnounceOutcome(announce_hash).to_bytes(),
            outcome.encode(),
        );
    }

    fn set_announce_schedule(&self, announce_hash: HashOf<Announce>, schedule: Schedule) {
        tracing::trace!(announce_hash = %announce_hash, "Set announce schedule");
        self.kv.put(
            &Key::AnnounceSchedule(announce_hash).to_bytes(),
            schedule.encode(),
        );
    }

    fn mutate_announce_meta(
        &self,
        announce_hash: HashOf<Announce>,
        f: impl FnOnce(&mut AnnounceMeta),
    ) {
        tracing::trace!(announce_hash = %announce_hash, "Mutate announce meta");
        let mut meta = self.announce_meta(announce_hash);
        f(&mut meta);
        self.kv
            .put(&Key::AnnounceMeta(announce_hash).to_bytes(), meta.encode());
    }
}

impl<'a, 'b> OnChainStorageRO for DatabaseRef<'a, 'b> {
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

    fn block_synced(&self, block_hash: H256) -> bool {
        self.with_small_data(block_hash, |data| data.block_is_synced)
            .unwrap_or_default()
    }

    fn validators(&self, era_index: u64) -> Option<ValidatorsVec> {
        self.kv
            .get(&Key::ValidatorSet(era_index).to_bytes())
            .map(|data| {
                Decode::decode(&mut data.as_slice())
                    .expect("Failed to decode data into `ValidatorsVec`")
            })
    }

    fn block_validators_committed_for_era(&self, block_hash: H256) -> Option<u64> {
        self.kv
            .get(&Key::LatestEraValidatorsCommitted(block_hash).to_bytes())
            .map(|data| {
                Decode::decode(&mut data.as_slice())
                    .expect("Failed to decode data into `u64` (era_index)")
            })
    }
}

impl<'a, 'b> OnChainStorageRW for DatabaseRef<'a, 'b> {
    fn set_block_header(&self, block_hash: H256, header: BlockHeader) {
        tracing::trace!("Set block header for {block_hash}");
        self.mutate_small_data(block_hash, |data| data.block_header = Some(header));
    }

    fn set_block_events(&self, block_hash: H256, events: &[BlockEvent]) {
        tracing::trace!("Set block events for {block_hash}");
        self.kv
            .put(&Key::BlockEvents(block_hash).to_bytes(), events.encode());
    }

    fn set_code_blob_info(&self, code_id: CodeId, code_info: CodeBlobInfo) {
        tracing::trace!("Set code upload info for {code_id}");
        self.kv
            .put(&Key::CodeUploadInfo(code_id).to_bytes(), code_info.encode());
    }

    fn set_block_synced(&self, block_hash: H256) {
        tracing::trace!("For block {block_hash} set synced");
        self.mutate_small_data(block_hash, |data| {
            data.block_is_synced = true;
        });
    }

    fn set_validators(&self, era_index: u64, validator_set: ValidatorsVec) {
        self.kv.put(
            &Key::ValidatorSet(era_index).to_bytes(),
            validator_set.encode(),
        );
    }

    fn set_block_validators_committed_for_era(&self, block_hash: H256, era_index: u64) {
        self.kv.put(
            &Key::LatestEraValidatorsCommitted(block_hash).to_bytes(),
            era_index.encode(),
        );
    }
}

impl<'a, 'b> BlockMetaStorageRO for DatabaseRef<'a, 'b> {
    fn block_meta(&self, block_hash: H256) -> BlockMeta {
        self.with_small_data(block_hash, |data| data.meta)
            .unwrap_or_default()
    }
}

impl<'a, 'b> BlockMetaStorageRW for DatabaseRef<'a, 'b> {
    fn mutate_block_meta(&self, block_hash: H256, f: impl FnOnce(&mut BlockMeta)) {
        tracing::trace!("For block {block_hash} mutate meta");
        self.mutate_small_data(block_hash, |data| {
            f(&mut data.meta);
        });
    }
}

#[derive(derive_more::Debug)]
#[debug("Database(CAS + KV)")]
pub struct Database {
    cas: Box<dyn CASDatabase>,
    kv: Box<dyn KVDatabase>,
    globals: Arc<RwLock<DBGlobals>>,
    config: Arc<RwLock<DBConfig>>,
}

impl Clone for Database {
    fn clone(&self) -> Self {
        Self {
            cas: self.cas.clone_boxed(),
            kv: self.kv.clone_boxed(),
            config: self.config.clone(),
            globals: self.globals.clone(),
        }
    }
}

impl Database {
    pub fn new(cas: &dyn CASDatabase, kv: &dyn KVDatabase) -> Result<Self> {
        let config = DBConfig::decode(
            &mut (kv
                .get(&Key::Config.to_bytes())
                .ok_or_else(|| anyhow::anyhow!("Database config not found"))?)
            .as_ref(),
        )?;

        if config.version != VERSION {
            return Err(anyhow::anyhow!(
                "Database version mismatch: expected {}, found {}",
                VERSION,
                config.version
            ));
        }

        let globals = DBGlobals::decode(
            &mut (kv
                .get(&Key::Globals.to_bytes())
                .ok_or_else(|| anyhow::anyhow!("Database globals not found"))?)
            .as_ref(),
        )?;

        let db = Self {
            cas: cas.clone_boxed(),
            kv: kv.clone_boxed(),
            globals: Arc::new(RwLock::new(globals)),
            config: Arc::new(RwLock::new(config)),
        };

        Ok(db)
    }

    fn as_ref(&self) -> DatabaseRef<'_, '_> {
        DatabaseRef {
            cas: self.cas.as_ref(),
            kv: self.kv.as_ref(),
        }
    }

    pub fn from_one<DB: CASDatabase + KVDatabase>(db: &DB) -> Result<Self> {
        Self::new(db, db)
    }

    #[cfg(feature = "mock")]
    #[track_caller]
    pub fn memory() -> Self {
        use crate::MemDb;
        use ethexe_common::{Address, ProtocolTimelines, SimpleBlockData};

        let mem_db = MemDb::default();

        // set default config and globals
        let config = DBConfig {
            version: VERSION,
            chain_id: 0,
            router_address: Address([0; 20]),
            timelines: ProtocolTimelines::default(),
            genesis_block_hash: H256::zero(),
            genesis_announce_hash: HashOf::zero(),
        };

        let globals = DBGlobals {
            start_block_hash: H256::zero(),
            start_announce_hash: HashOf::zero(),
            latest_synced_block: SimpleBlockData::default(),
            latest_prepared_block_hash: H256::zero(),
            latest_computed_announce_hash: HashOf::zero(),
        };

        mem_db.put(&Key::Config.to_bytes(), config.encode());
        mem_db.put(&Key::Globals.to_bytes(), globals.encode());

        Self::from_one(&mem_db).unwrap()
    }

    /// # Safety
    /// Not ready for using in prod. Intended to be for rpc calls only.
    pub unsafe fn overlaid(self) -> Self {
        Self {
            cas: Box::new(CASOverlay::new(self.cas)),
            kv: Box::new(KVOverlay::new(self.kv)),
            config: self.config,
            globals: self.globals,
        }
    }

    pub fn cas(&self) -> &dyn CASDatabase {
        self.cas.as_ref()
    }

    fn with_small_data<R>(
        &self,
        block_hash: H256,
        f: impl FnOnce(BlockSmallData) -> R,
    ) -> Option<R> {
        self.block_small_data(block_hash).map(f)
    }

    /// Mutates `BlockSmallData` for the given block hash.
    ///
    /// If data wasn't found, it will be created with default values and then mutated.
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
}

impl HashStorageRO for Database {
    fn read_by_hash(&self, hash: H256) -> Option<Vec<u8>> {
        self.cas.read(hash)
    }
}

#[derive(Debug, Clone, Default, Encode, Decode, PartialEq, Eq)]
struct BlockSmallData {
    block_header: Option<BlockHeader>,
    block_is_synced: bool,
    meta: BlockMeta,
}

impl BlockMetaStorageRO for Database {
    fn block_meta(&self, block_hash: H256) -> BlockMeta {
        self.with_small_data(block_hash, |data| data.meta)
            .unwrap_or_default()
    }
}

impl BlockMetaStorageRW for Database {
    fn mutate_block_meta(&self, block_hash: H256, f: impl FnOnce(&mut BlockMeta)) {
        tracing::trace!("For block {block_hash} mutate meta");
        self.mutate_small_data(block_hash, |data| {
            f(&mut data.meta);
        });
    }
}

impl CodesStorageRO for Database {
    fn original_code_exists(&self, code_id: CodeId) -> bool {
        self.cas.contains(code_id.into())
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

impl CodesStorageRW for Database {
    fn set_original_code(&self, code: &[u8]) -> CodeId {
        tracing::trace!(code_id = %CodeId::generate(code), code_len = %code.len(), "Set original code");
        self.cas.write(code).into()
    }

    fn set_program_code_id(&self, program_id: ActorId, code_id: CodeId) {
        tracing::trace!(
            program_id = ?program_id,
            code_id = ?code_id,
            "Set program to code id mapping"
        );
        self.kv.put(
            &Key::ProgramToCodeId(program_id).to_bytes(),
            code_id.into_bytes().to_vec(),
        );
    }

    fn set_instrumented_code(&self, runtime_id: u32, code_id: CodeId, code: InstrumentedCode) {
        tracing::trace!(
            code_id = ?code_id,
            runtime_id = %runtime_id,
            "Set instrumented code"
        );
        self.kv.put(
            &Key::InstrumentedCode(runtime_id, code_id).to_bytes(),
            code.encode(),
        );
    }

    fn set_code_metadata(&self, code_id: CodeId, code_metadata: CodeMetadata) {
        tracing::trace!(code_id = ?code_id, "Set code metadata");
        self.kv.put(
            &Key::CodeMetadata(code_id).to_bytes(),
            code_metadata.encode(),
        );
    }

    fn set_code_valid(&self, code_id: CodeId, valid: bool) {
        tracing::trace!(code_id = ?code_id, valid = %valid, "Set code status");
        self.kv
            .put(&Key::CodeValid(code_id).to_bytes(), valid.encode());
    }
}

impl<'a> Storage for dyn CASDatabase + 'a {
    fn program_state(&self, hash: H256) -> Option<ProgramState> {
        if hash.is_zero() {
            return Some(ProgramState::zero());
        }

        let data = self.read(hash)?;

        let state = ProgramState::decode(&mut &data[..])
            .expect("Failed to decode data into `ProgramState`");

        Some(state)
    }

    fn write_program_state(&self, state: ProgramState) -> H256 {
        if state.is_zero() {
            return H256::zero();
        }

        self.write(&state.encode())
    }

    fn message_queue(&self, hash: HashOf<MessageQueue>) -> Option<MessageQueue> {
        self.read(hash.inner()).map(|data| {
            MessageQueue::decode(&mut &data[..]).expect("Failed to decode data into `MessageQueue`")
        })
    }

    fn write_message_queue(&self, queue: MessageQueue) -> HashOf<MessageQueue> {
        unsafe { HashOf::new(self.write(&queue.encode())) }
    }

    fn waitlist(&self, hash: HashOf<Waitlist>) -> Option<Waitlist> {
        self.read(hash.inner()).map(|data| {
            Waitlist::decode(&mut data.as_slice()).expect("Failed to decode data into `Waitlist`")
        })
    }

    fn write_waitlist(&self, waitlist: Waitlist) -> HashOf<Waitlist> {
        unsafe { HashOf::new(self.write(&waitlist.encode())) }
    }

    fn dispatch_stash(&self, hash: HashOf<DispatchStash>) -> Option<DispatchStash> {
        self.read(hash.inner()).map(|data| {
            DispatchStash::decode(&mut data.as_slice())
                .expect("Failed to decode data into `DispatchStash`")
        })
    }

    fn write_dispatch_stash(&self, stash: DispatchStash) -> HashOf<DispatchStash> {
        unsafe { HashOf::new(self.write(&stash.encode())) }
    }

    fn mailbox(&self, hash: HashOf<Mailbox>) -> Option<Mailbox> {
        self.read(hash.inner()).map(|data| {
            Mailbox::decode(&mut data.as_slice()).expect("Failed to decode data into `Mailbox`")
        })
    }

    fn write_mailbox(&self, mailbox: Mailbox) -> HashOf<Mailbox> {
        unsafe { HashOf::new(self.write(&mailbox.encode())) }
    }

    fn user_mailbox(&self, hash: HashOf<UserMailbox>) -> Option<UserMailbox> {
        self.read(hash.inner()).map(|data| {
            UserMailbox::decode(&mut data.as_slice())
                .expect("Failed to decode data into `UserMailbox`")
        })
    }

    fn write_user_mailbox(&self, use_mailbox: UserMailbox) -> HashOf<UserMailbox> {
        unsafe { HashOf::new(self.write(&use_mailbox.encode())) }
    }

    fn memory_pages(&self, hash: HashOf<MemoryPages>) -> Option<MemoryPages> {
        self.read(hash.inner()).map(|data| {
            MemoryPages::decode(&mut &data[..]).expect("Failed to decode data into `MemoryPages`")
        })
    }

    fn memory_pages_region(&self, hash: HashOf<MemoryPagesRegion>) -> Option<MemoryPagesRegion> {
        self.read(hash.inner()).map(|data| {
            MemoryPagesRegion::decode(&mut &data[..])
                .expect("Failed to decode data into `MemoryPagesRegion`")
        })
    }

    fn write_memory_pages(&self, pages: MemoryPages) -> HashOf<MemoryPages> {
        unsafe { HashOf::new(self.write(&pages.encode())) }
    }

    fn write_memory_pages_region(
        &self,
        pages_region: MemoryPagesRegion,
    ) -> HashOf<MemoryPagesRegion> {
        unsafe { HashOf::new(self.write(&pages_region.encode())) }
    }

    fn allocations(&self, hash: HashOf<Allocations>) -> Option<Allocations> {
        self.read(hash.inner()).map(|data| {
            Allocations::decode(&mut &data[..]).expect("Failed to decode data into `Allocations`")
        })
    }

    fn write_allocations(&self, allocations: Allocations) -> HashOf<Allocations> {
        unsafe { HashOf::new(self.write(&allocations.encode())) }
    }

    fn payload(&self, hash: HashOf<Payload>) -> Option<Payload> {
        self.read(hash.inner())
            .map(|data| Payload::try_from(data).expect("Failed to decode data into `Payload`"))
    }

    fn write_payload(&self, payload: Payload) -> HashOf<Payload> {
        unsafe { HashOf::new(self.write(&payload)) }
    }

    fn page_data(&self, hash: HashOf<PageBuf>) -> Option<PageBuf> {
        self.read(hash.inner()).map(|data| {
            PageBuf::decode(&mut data.as_slice()).expect("Failed to decode data into `PageBuf`")
        })
    }

    fn write_page_data(&self, data: PageBuf) -> HashOf<PageBuf> {
        unsafe { HashOf::new(self.write(&data)) }
    }
}

// Delegate Storage implementation to inner CASDatabase (mostly for testing purposes)
impl Storage for Database {
    delegate::delegate! {
        to self.cas {
            fn program_state(&self, hash: H256) -> Option<ProgramState>;
            fn write_program_state(&self, state: ProgramState) -> H256;
            fn message_queue(&self, hash: HashOf<MessageQueue>) -> Option<MessageQueue>;
            fn write_message_queue(&self, queue: MessageQueue) -> HashOf<MessageQueue>;
            fn waitlist(&self, hash: HashOf<Waitlist>) -> Option<Waitlist>;
            fn write_waitlist(&self, waitlist: Waitlist) -> HashOf<Waitlist>;
            fn dispatch_stash(&self, hash: HashOf<DispatchStash>) -> Option<DispatchStash>;
            fn write_dispatch_stash(&self, stash: DispatchStash) -> HashOf<DispatchStash>;
            fn mailbox(&self, hash: HashOf<Mailbox>) -> Option<Mailbox>;
            fn write_mailbox(&self, mailbox: Mailbox) -> HashOf<Mailbox>;
            fn user_mailbox(&self, hash: HashOf<UserMailbox>) -> Option<UserMailbox>;
            fn write_user_mailbox(&self, use_mailbox: UserMailbox) -> HashOf<UserMailbox>;
            fn memory_pages(&self, hash: HashOf<MemoryPages>) -> Option<MemoryPages>;
            fn memory_pages_region(&self, hash: HashOf<MemoryPagesRegion>) -> Option<MemoryPagesRegion>;
            fn write_memory_pages(&self, pages: MemoryPages) -> HashOf<MemoryPages>;
            fn write_memory_pages_region(&self, pages_region: MemoryPagesRegion) -> HashOf<MemoryPagesRegion>;
            fn allocations(&self, hash: HashOf<Allocations>) -> Option<Allocations>;
            fn write_allocations(&self, allocations: Allocations) -> HashOf<Allocations>;
            fn payload(&self, hash: HashOf<Payload>) -> Option<Payload>;
            fn write_payload(&self, payload: Payload) -> HashOf<Payload>;
            fn page_data(&self, hash: HashOf<PageBuf>) -> Option<PageBuf>;
            fn write_page_data(&self, data: PageBuf) -> HashOf<PageBuf>;
        }
    }
}

impl OnChainStorageRO for Database {
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

    fn block_synced(&self, block_hash: H256) -> bool {
        self.with_small_data(block_hash, |data| data.block_is_synced)
            .unwrap_or_default()
    }

    fn validators(&self, era_index: u64) -> Option<ValidatorsVec> {
        self.kv
            .get(&Key::ValidatorSet(era_index).to_bytes())
            .map(|data| {
                Decode::decode(&mut data.as_slice())
                    .expect("Failed to decode data into `ValidatorsVec`")
            })
    }

    fn block_validators_committed_for_era(&self, block_hash: H256) -> Option<u64> {
        self.kv
            .get(&Key::LatestEraValidatorsCommitted(block_hash).to_bytes())
            .map(|data| {
                Decode::decode(&mut data.as_slice())
                    .expect("Failed to decode data into `u64` (era_index)")
            })
    }
}

impl OnChainStorageRW for Database {
    fn set_block_header(&self, block_hash: H256, header: BlockHeader) {
        tracing::trace!("Set block header for {block_hash}");
        self.mutate_small_data(block_hash, |data| data.block_header = Some(header));
    }

    fn set_block_events(&self, block_hash: H256, events: &[BlockEvent]) {
        tracing::trace!("Set block events for {block_hash}");
        self.kv
            .put(&Key::BlockEvents(block_hash).to_bytes(), events.encode());
    }

    fn set_code_blob_info(&self, code_id: CodeId, code_info: CodeBlobInfo) {
        tracing::trace!("Set code upload info for {code_id}");
        self.kv
            .put(&Key::CodeUploadInfo(code_id).to_bytes(), code_info.encode());
    }

    fn set_block_synced(&self, block_hash: H256) {
        tracing::trace!("For block {block_hash} set synced");
        self.mutate_small_data(block_hash, |data| {
            data.block_is_synced = true;
        });
    }

    fn set_validators(&self, era_index: u64, validator_set: ValidatorsVec) {
        self.kv.put(
            &Key::ValidatorSet(era_index).to_bytes(),
            validator_set.encode(),
        );
    }

    fn set_block_validators_committed_for_era(&self, block_hash: H256, era_index: u64) {
        self.kv.put(
            &Key::LatestEraValidatorsCommitted(block_hash).to_bytes(),
            era_index.encode(),
        );
    }
}

impl InjectedStorageRO for Database {
    fn injected_transaction(
        &self,
        hash: HashOf<InjectedTransaction>,
    ) -> Option<SignedInjectedTransaction> {
        self.kv
            .get(&Key::InjectedTransaction(hash).to_bytes())
            .map(|data| {
                SignedInjectedTransaction::decode(&mut data.as_slice())
                    .expect("Failed to decode data into `SignedInjectedTransaction`")
            })
    }
}

impl InjectedStorageRW for Database {
    fn set_injected_transaction(&self, tx: SignedInjectedTransaction) {
        let tx_hash = tx.data().to_hash();

        tracing::trace!(injected_tx_hash = ?tx_hash, "Set injected transaction");
        self.kv
            .put(&Key::InjectedTransaction(tx_hash).to_bytes(), tx.encode());
    }
}

impl AnnounceStorageRO for Database {
    delegate!(to self.as_ref() {
        fn announce(&self, hash: HashOf<Announce>) -> Option<Announce>;
        fn announce_program_states(&self, announce_hash: HashOf<Announce>) -> Option<ProgramStates>;
        fn announce_outcome(&self, announce_hash: HashOf<Announce>) -> Option<Vec<StateTransition>>;
        fn announce_schedule(&self, announce_hash: HashOf<Announce>) -> Option<Schedule>;
        fn announce_meta(&self, announce_hash: HashOf<Announce>) -> AnnounceMeta;
    });
}

impl AnnounceStorageRW for Database {
    delegate!(to self.as_ref() {
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
    });
}

impl GlobalsStorageRO for Database {
    fn globals(&self) -> RwLockReadGuard<'_, DBGlobals> {
        self.globals
            .read()
            .expect("Failed to lock globals for reading")
    }
}

impl GlobalsStorageRW for Database {
    fn globals_mutate<R>(&self, mut f: impl FnMut(&mut DBGlobals) -> R) -> R {
        let mut globals = self
            .globals
            .write()
            .expect("Failed to lock globals for writing");
        let res = f(&mut globals);
        self.kv.put(&Key::Globals.to_bytes(), globals.encode());
        res
    }
}

impl ConfigStorageRO for Database {
    fn config(&self) -> RwLockReadGuard<'_, DBConfig> {
        self.config
            .read()
            .expect("Failed to lock config for reading")
    }
}

#[cfg(feature = "mock")]
mod mock {
    use super::*;
    use ethexe_common::db::{SetConfig, SetGlobals};

    impl SetConfig for Database {
        fn set_config(&self, config: DBConfig) {
            self.config
                .write()
                .expect("Failed to lock config for writing")
                .clone_from(&config);
            self.kv.put(&Key::Config.to_bytes(), config.encode());
        }
    }

    impl SetGlobals for Database {
        fn set_globals(&self, globals: DBGlobals) {
            self.globals
                .write()
                .expect("Failed to lock globals for writing")
                .clone_from(&globals);
            self.kv.put(&Key::Globals.to_bytes(), globals.encode());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ethexe_common::{
        ecdsa::PrivateKey,
        events::{RouterEvent, router::StorageSlotChangedEvent},
    };
    use gear_core::code::{InstantiatedSectionSizes, InstrumentationStatus};

    #[test]
    fn test_injected_transaction() {
        let db = Database::memory();

        let private_key = PrivateKey::from_seed([1; 32]).expect("valid seed");
        let tx = SignedInjectedTransaction::create(
            private_key,
            InjectedTransaction {
                destination: ActorId::zero(),
                payload: vec![].into(),
                value: 0,
                reference_block: H256::random(),
                salt: vec![].into(),
            },
        )
        .unwrap();
        let tx_hash = tx.data().to_hash();
        db.set_injected_transaction(tx.clone());
        assert_eq!(db.injected_transaction(tx_hash), Some(tx));
    }

    #[test]
    fn test_announce() {
        let db = Database::memory();

        let announce = Announce {
            block_hash: H256::random(),
            parent: HashOf::random(),
            gas_allowance: Some(1000),
            injected_transactions: vec![],
        };
        let announce_hash = db.set_announce(announce.clone());
        assert_eq!(announce_hash, announce.to_hash());
        assert_eq!(db.announce(announce_hash), Some(announce));
    }

    #[test]
    fn test_announce_program_states() {
        let db = Database::memory();

        let announce_hash = HashOf::random();
        let program_states = ProgramStates::default();
        db.set_announce_program_states(announce_hash, program_states.clone());
        assert_eq!(
            db.announce_program_states(announce_hash),
            Some(program_states)
        );
    }

    #[test]
    fn test_announce_outcome() {
        let db = Database::memory();

        let announce_hash = HashOf::random();
        let block_outcome = vec![StateTransition::default()];
        db.set_announce_outcome(announce_hash, block_outcome.clone());
        assert_eq!(db.announce_outcome(announce_hash), Some(block_outcome));
    }

    #[test]
    fn test_announce_schedule() {
        let db = Database::memory();

        let announce_hash = HashOf::random();
        let schedule = Schedule::default();
        db.set_announce_schedule(announce_hash, schedule.clone());
        assert_eq!(db.announce_schedule(announce_hash), Some(schedule));
    }

    #[test]
    fn test_block_events() {
        let db = Database::memory();

        let block_hash = H256::random();
        let events = vec![BlockEvent::Router(RouterEvent::StorageSlotChanged(
            StorageSlotChangedEvent {
                slot: H256::random(),
            },
        ))];
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
        assert!(!db.block_synced(block_hash));
        db.set_block_synced(block_hash);
        assert!(db.block_synced(block_hash));
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
        db.set_block_header(block_hash, block_header);
        assert_eq!(db.block_header(block_hash), Some(block_header));
    }

    #[test]
    fn test_state() {
        let db = Database::memory();

        let state = ProgramState::zero();
        let hash = db.write_program_state(state);
        assert_eq!(db.program_state(hash), Some(state));
    }

    #[test]
    fn test_queue() {
        let db = Database::memory();

        let queue = MessageQueue::default();
        let hash = db.write_message_queue(queue.clone());
        assert_eq!(db.message_queue(hash), Some(queue));
    }

    #[test]
    fn test_waitlist() {
        let db = Database::memory();

        let waitlist = Waitlist::default();
        let hash = db.write_waitlist(waitlist.clone());
        assert_eq!(db.waitlist(hash), Some(waitlist));
    }

    #[test]
    fn test_stash() {
        let db = Database::memory();

        let stash = DispatchStash::default();
        let hash = db.write_dispatch_stash(stash.clone());
        assert_eq!(db.dispatch_stash(hash), Some(stash));
    }

    #[test]
    fn test_mailbox() {
        let db = Database::memory();

        let mailbox = Mailbox::default();
        let hash = db.write_mailbox(mailbox.clone());
        assert_eq!(db.mailbox(hash), Some(mailbox));
    }

    #[test]
    fn test_pages() {
        let db = Database::memory();

        let pages = MemoryPages::default();
        let hash = db.write_memory_pages(pages.clone());
        assert_eq!(db.memory_pages(hash), Some(pages));
    }

    #[test]
    fn test_pages_region() {
        let db = Database::memory();

        let pages_region = MemoryPagesRegion::default();
        let hash = db.write_memory_pages_region(pages_region.clone());
        assert_eq!(db.memory_pages_region(hash), Some(pages_region));
    }

    #[test]
    fn test_allocations() {
        let db = Database::memory();

        let allocations = Allocations::default();
        let hash = db.write_allocations(allocations.clone());
        assert_eq!(db.allocations(hash), Some(allocations));
    }

    #[test]
    fn test_payload() {
        let db = Database::memory();

        let payload: Payload = vec![1, 2, 3].try_into().unwrap();
        let hash = db.write_payload(payload.clone());
        assert_eq!(db.payload(hash), Some(payload));
    }

    #[test]
    fn test_page_data() {
        let db = Database::memory();

        let mut page_data = PageBuf::new_zeroed();
        page_data[42] = 42;
        let hash = db.write_page_data(page_data.clone());
        assert_eq!(db.page_data(hash), Some(page_data));
    }
}
