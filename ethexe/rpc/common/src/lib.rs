// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Shared Vara.eth RPC types.

mod filter;
pub use filter::{
    FilterSet, PromiseEnvelope, PromiseSubscriptionFilter, ReplyCodeFilter, ValueOrArray,
};

use ethexe_common::gear::Message;
use ethexe_runtime_common::state::{DispatchStash, Mailbox, MessageQueue, Program, Waitlist};
use gear_core::rpc::ReplyInfo;
use gprimitives::H256;
use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgramBestState {
    pub mb_hash: H256,
    pub new_state_hash: H256,
    pub messages: Vec<Message>,
}
