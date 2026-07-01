// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::types::{CalculateReplyForHandleResult, FullProgramState, ProgramBestState};
use ethexe_runtime_common::state::{
    DispatchStash, Mailbox, MemoryPages, MessageQueue, ProgramState, UserMailbox, Waitlist,
};
use gprimitives::{H160, H256};
use jsonrpsee::proc_macros::rpc;
use sp_core::Bytes;

#[rpc(client)]
pub trait Program {
    #[method(name = "program_calculateReplyForHandle")]
    async fn calculate_reply_for_handle(
        &self,
        at: Option<H256>,
        source: H160,
        program_id: H160,
        payload: Bytes,
        value: u128,
        top_up: Option<u128>,
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

    #[method(name = "program_readUserMailbox")]
    async fn read_user_mailbox(&self, hash: H256) -> jsonrpsee::core::RpcResult<UserMailbox>;

    #[method(name = "program_readFullState")]
    async fn read_full_state(&self, hash: H256) -> jsonrpsee::core::RpcResult<FullProgramState>;

    #[method(name = "program_readPages")]
    async fn read_pages(&self, hash: H256) -> jsonrpsee::core::RpcResult<MemoryPages>;

    #[method(name = "program_readPageData")]
    async fn read_page_data(&self, hash: H256) -> jsonrpsee::core::RpcResult<Bytes>;

    /// Subscribes to the program's best state, emitted on every newly computed MB.
    #[subscription(
        name = "program_subscribeBestState",
        unsubscribe = "program_unsubscribeBestState",
        item = ProgramBestState
    )]
    async fn subscribe_best_state(&self, program_id: H160) -> jsonrpsee::core::SubscriptionResult;
}
