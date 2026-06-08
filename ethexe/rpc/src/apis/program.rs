// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

#[cfg(feature = "server")]
use crate::{errors, utils};
use ethexe_common::gear::Message;
#[cfg(feature = "server")]
use ethexe_common::{
    HashOf,
    db::{CodesStorageRO, MbStorageRO},
};
#[cfg(feature = "server")]
use ethexe_db::Database;
#[cfg(feature = "server")]
use ethexe_processor::{ExecutableDataForReply, OverlaidProcessor};
use ethexe_runtime_common::state::{
    DispatchStash, Mailbox, MemoryPages, MessageQueue, Program, ProgramState, Waitlist,
};
#[cfg(feature = "server")]
use ethexe_runtime_common::state::{QueryableStorage, Storage};
use gear_core::rpc::ReplyInfo;
use gprimitives::{H160, H256};
#[cfg(feature = "server")]
use jsonrpsee::core::async_trait;
use jsonrpsee::proc_macros::rpc;
#[cfg(feature = "server")]
use parity_scale_codec::Encode;
use serde::{Deserialize, Serialize};
use sp_core::Bytes;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FullProgramState {
    pub program: Program,
    pub canonical_queue: Option<MessageQueue>,
    pub injected_queue: Option<MessageQueue>,
    pub waitlist: Option<Waitlist>,
    pub stash: Option<DispatchStash>,
    pub mailbox: Option<Mailbox>,
    pub balance: u128,
    pub executable_balance: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalculateReplyForHandleResult {
    pub reply: ReplyInfo,
    pub messages: Vec<Message>,
}

#[cfg_attr(all(feature = "server", feature = "client"), rpc(server, client))]
#[cfg_attr(all(feature = "server", not(feature = "client")), rpc(server))]
#[cfg_attr(all(not(feature = "server"), feature = "client"), rpc(client))]
pub trait Program {
    #[method(name = "program_calculateReplyForHandle")]
    async fn calculate_reply_for_handle(
        &self,
        at: Option<H256>,
        source: H160,
        program_id: H160,
        payload: Bytes,
        value: u128,
    ) -> jsonrpsee::core::RpcResult<CalculateReplyForHandleResult>;

    #[method(name = "program_ids")]
    async fn ids(&self) -> jsonrpsee::core::RpcResult<Vec<H160>>;

    #[method(name = "program_codeId")]
    async fn code_id(&self, program_id: H160) -> jsonrpsee::core::RpcResult<H256>;

    #[method(name = "program_readState")]
    async fn read_state(&self, hash: H256) -> jsonrpsee::core::RpcResult<ProgramState>;

    #[method(name = "program_readQueue")]
    async fn read_queue(&self, hash: H256) -> jsonrpsee::core::RpcResult<MessageQueue>;

    #[method(name = "program_readWaitlist")]
    async fn read_waitlist(&self, hash: H256) -> jsonrpsee::core::RpcResult<Waitlist>;

    #[method(name = "program_readStash")]
    async fn read_stash(&self, hash: H256) -> jsonrpsee::core::RpcResult<DispatchStash>;

    #[method(name = "program_readMailbox")]
    async fn read_mailbox(&self, hash: H256) -> jsonrpsee::core::RpcResult<Mailbox>;

    #[method(name = "program_readFullState")]
    async fn read_full_state(&self, hash: H256) -> jsonrpsee::core::RpcResult<FullProgramState>;

    #[method(name = "program_readPages")]
    async fn read_pages(&self, hash: H256) -> jsonrpsee::core::RpcResult<MemoryPages>;

    #[method(name = "program_readPageData")]
    async fn read_page_data(&self, hash: H256) -> jsonrpsee::core::RpcResult<Bytes>;
}

#[cfg(feature = "server")]
pub struct ProgramApi {
    db: Database,
    processor: OverlaidProcessor,
    gas_allowance: u64,
}

#[cfg(feature = "server")]
impl ProgramApi {
    pub fn new(db: Database, processor: OverlaidProcessor, gas_allowance: u64) -> Self {
        Self {
            db,
            processor,
            gas_allowance,
        }
    }

    fn read_queue(&self, hash: H256) -> Option<MessageQueue> {
        self.db.message_queue(unsafe { HashOf::new(hash) })
    }

    fn read_waitlist(&self, hash: H256) -> Option<Waitlist> {
        self.db.waitlist(unsafe { HashOf::new(hash) })
    }

    fn read_stash(&self, hash: H256) -> Option<DispatchStash> {
        self.db.dispatch_stash(unsafe { HashOf::new(hash) })
    }

    fn read_mailbox(&self, hash: H256) -> Option<Mailbox> {
        self.db.mailbox(unsafe { HashOf::new(hash) })
    }
}

#[cfg(feature = "server")]
#[async_trait]
impl ProgramServer for ProgramApi {
    async fn calculate_reply_for_handle(
        &self,
        _at: Option<H256>,
        source: H160,
        program_id: H160,
        payload: Bytes,
        value: u128,
    ) -> jsonrpsee::core::RpcResult<CalculateReplyForHandleResult> {
        let mb_hash = utils::latest_computed_mb(&self.db)?;
        let block = utils::block_at_or_latest_synced(&self.db, None)?;

        let executable = ExecutableDataForReply {
            height: block.header.height,
            timestamp: block.header.timestamp,
            program_states: self
                .db
                .mb_program_states(mb_hash)
                .ok_or_else(|| errors::db("Failed to get program states"))?,
            source: source.into(),
            program_id: program_id.into(),
            payload: payload.to_vec(),
            value,
            gas_allowance: self.gas_allowance,
        };

        // TODO (breathx): spawn in a new thread and catch panics. (?) Generally catch runtime panics (?).
        // TODO (breathx): optimize here instantiation if matches actual runtime.

        self.processor
            .clone()
            .execute_for_reply(executable)
            .await
            .map(|outcome| CalculateReplyForHandleResult {
                reply: outcome.reply,
                messages: outcome.messages,
            })
            .map_err(errors::runtime)
    }

    async fn ids(&self) -> jsonrpsee::core::RpcResult<Vec<H160>> {
        let mb_hash = utils::latest_computed_mb(&self.db)?;

        Ok(self
            .db
            .mb_program_states(mb_hash)
            .ok_or_else(|| errors::db("Failed to get program states"))?
            .into_keys()
            .map(|id| id.try_into().unwrap())
            .collect())
    }

    async fn code_id(&self, program_id: H160) -> jsonrpsee::core::RpcResult<H256> {
        self.db
            .program_code_id(program_id.into())
            .ok_or_else(|| errors::db("Failed to get code id"))
            .map(|code_id| code_id.into())
    }

    async fn read_state(&self, hash: H256) -> jsonrpsee::core::RpcResult<ProgramState> {
        self.db
            .program_state(hash)
            .ok_or_else(|| errors::db("Failed to read state by hash"))
    }

    async fn read_queue(&self, hash: H256) -> jsonrpsee::core::RpcResult<MessageQueue> {
        self.read_queue(hash)
            .ok_or_else(|| errors::db("Failed to read queue by hash"))
    }

    async fn read_waitlist(&self, hash: H256) -> jsonrpsee::core::RpcResult<Waitlist> {
        self.read_waitlist(hash)
            .ok_or_else(|| errors::db("Failed to read waitlist by hash"))
    }

    async fn read_stash(&self, hash: H256) -> jsonrpsee::core::RpcResult<DispatchStash> {
        self.read_stash(hash)
            .ok_or_else(|| errors::db("Failed to read stash by hash"))
    }

    async fn read_mailbox(&self, hash: H256) -> jsonrpsee::core::RpcResult<Mailbox> {
        self.read_mailbox(hash)
            .ok_or_else(|| errors::db("Failed to read mailbox by hash"))
    }

    async fn read_full_state(&self, hash: H256) -> jsonrpsee::core::RpcResult<FullProgramState> {
        let Some(ProgramState {
            program,
            canonical_queue,
            injected_queue,
            waitlist_hash,
            stash_hash,
            mailbox_hash,
            balance,
            executable_balance,
        }) = self.db.program_state(hash)
        else {
            return Err(errors::db("Failed to read state by hash"));
        };

        let canonical_queue = canonical_queue.query(&self.db).ok();
        let injected_queue = injected_queue.query(&self.db).ok();
        let waitlist = self.db.query(&waitlist_hash).ok();
        let stash = self.db.query(&stash_hash).ok();
        let mailbox = self.db.query(&mailbox_hash).ok();

        Ok(FullProgramState {
            program,
            canonical_queue,
            injected_queue,
            waitlist,
            stash,
            mailbox,
            balance,
            executable_balance,
        })
    }

    async fn read_pages(&self, hash: H256) -> jsonrpsee::core::RpcResult<MemoryPages> {
        self.db
            .memory_pages(unsafe { HashOf::new(hash) })
            .ok_or_else(|| errors::db("Failed to read pages by hash"))
    }

    async fn read_page_data(&self, hash: H256) -> jsonrpsee::core::RpcResult<Bytes> {
        self.db
            .page_data(unsafe { HashOf::new(hash) })
            .map(|buf| buf.encode().into())
            .ok_or_else(|| errors::db("Failed to read page data by hash"))
    }
}
