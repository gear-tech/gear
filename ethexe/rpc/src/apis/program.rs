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

use crate::{errors, utils};
use ethexe_common::db::{AnnounceStorageRO, CodesStorageRO};
use ethexe_db::Database;
use ethexe_processor::Processor;
use ethexe_runtime_common::state::{
    DispatchStash, HashOf, Mailbox, MemoryPages, MessageQueue, Program, ProgramState, Storage,
    Waitlist,
};
use gear_core::rpc::ReplyInfo;
use gprimitives::{H160, H256};
use jsonrpsee::{
    core::{RpcResult, async_trait},
    proc_macros::rpc,
};
use parity_scale_codec::Encode;
use serde::{Deserialize, Serialize};
use sp_core::Bytes;

#[derive(Clone, Serialize, Deserialize)]
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

#[rpc(server)]
pub trait Program {
    #[method(name = "program_calculateReplyForHandle")]
    async fn calculate_reply_for_handle(
        &self,
        at: Option<H256>,
        source: H160,
        program_id: H160,
        payload: Bytes,
        value: u128,
    ) -> RpcResult<ReplyInfo>;

    #[method(name = "program_ids")]
    async fn ids(&self) -> RpcResult<Vec<H160>>;

    #[method(name = "program_codeId")]
    async fn code_id(&self, program_id: H160) -> RpcResult<H256>;

    #[method(name = "program_readState")]
    async fn read_state(&self, hash: H256) -> RpcResult<ProgramState>;

    #[method(name = "program_readQueue")]
    async fn read_queue(&self, hash: H256) -> RpcResult<MessageQueue>;

    #[method(name = "program_readWaitlist")]
    async fn read_waitlist(&self, hash: H256) -> RpcResult<Waitlist>;

    #[method(name = "program_readStash")]
    async fn read_stash(&self, hash: H256) -> RpcResult<DispatchStash>;

    #[method(name = "program_readMailbox")]
    async fn read_mailbox(&self, hash: H256) -> RpcResult<Mailbox>;

    #[method(name = "program_readFullState")]
    async fn read_full_state(&self, hash: H256) -> RpcResult<FullProgramState>;

    #[method(name = "program_readPages")]
    async fn read_pages(&self, hash: H256) -> RpcResult<MemoryPages>;

    #[method(name = "program_readPageData")]
    async fn read_page_data(&self, hash: H256) -> RpcResult<Bytes>;
}

pub struct ProgramApi {
    db: Database,
}

impl ProgramApi {
    pub fn new(db: Database) -> Self {
        Self { db }
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

#[async_trait]
impl ProgramServer for ProgramApi {
    async fn calculate_reply_for_handle(
        &self,
        at: Option<H256>,
        source: H160,
        program_id: H160,
        payload: Bytes,
        value: u128,
    ) -> RpcResult<ReplyInfo> {
        let block_hash = utils::block_header_at_or_latest(&self.db, at)?.hash;

        // TODO (breathx): spawn in a new thread and catch panics. (?) Generally catch runtime panics (?).
        // TODO (breathx): optimize here instantiation if matches actual runtime.
        let processor = Processor::new(self.db.clone()).map_err(|_| errors::internal())?;

        let mut overlaid_processor = processor.overlaid();

        overlaid_processor
            .execute_for_reply(
                block_hash,
                source.into(),
                program_id.into(),
                payload.0,
                value,
            )
            .await
            .map_err(errors::runtime)
    }

    async fn ids(&self) -> RpcResult<Vec<H160>> {
        let announce_hash = utils::announce_at_or_latest(&self.db, None)?;

        Ok(self
            .db
            .announce_program_states(announce_hash)
            .ok_or_else(|| errors::db("Failed to get program states"))?
            .into_keys()
            .map(|id| id.try_into().unwrap())
            .collect())
    }

    async fn code_id(&self, program_id: H160) -> RpcResult<H256> {
        self.db
            .program_code_id(program_id.into())
            .ok_or_else(|| errors::db("Failed to get code id"))
            .map(|code_id| code_id.into())
    }

    async fn read_state(&self, hash: H256) -> RpcResult<ProgramState> {
        self.db
            .program_state(hash)
            .ok_or_else(|| errors::db("Failed to read state by hash"))
    }

    async fn read_queue(&self, hash: H256) -> RpcResult<MessageQueue> {
        self.read_queue(hash)
            .ok_or_else(|| errors::db("Failed to read queue by hash"))
    }

    async fn read_waitlist(&self, hash: H256) -> RpcResult<Waitlist> {
        self.read_waitlist(hash)
            .ok_or_else(|| errors::db("Failed to read waitlist by hash"))
    }

    async fn read_stash(&self, hash: H256) -> RpcResult<DispatchStash> {
        self.read_stash(hash)
            .ok_or_else(|| errors::db("Failed to read stash by hash"))
    }

    async fn read_mailbox(&self, hash: H256) -> RpcResult<Mailbox> {
        self.read_mailbox(hash)
            .ok_or_else(|| errors::db("Failed to read mailbox by hash"))
    }

    async fn read_full_state(&self, hash: H256) -> RpcResult<FullProgramState> {
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
        let waitlist = waitlist_hash.query(&self.db).ok();
        let stash = stash_hash.query(&self.db).ok();
        let mailbox = mailbox_hash.query(&self.db).ok();

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

    async fn read_pages(&self, hash: H256) -> RpcResult<MemoryPages> {
        self.db
            .memory_pages(unsafe { HashOf::new(hash) })
            .ok_or_else(|| errors::db("Failed to read pages by hash"))
    }

    async fn read_page_data(&self, hash: H256) -> RpcResult<Bytes> {
        self.db
            .page_data(unsafe { HashOf::new(hash) })
            .map(|buf| buf.encode().into())
            .ok_or_else(|| errors::db("Failed to read page data by hash"))
    }
}
