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

//! ethexe common types and traits.

#![no_std]

extern crate alloc;

pub mod db;
pub mod events;

use alloc::vec::Vec;
use gear_core::{
    ids::{ActorId, CodeId},
    message::{Payload, ReplyDetails},
};
use gprimitives::{MessageId, H256};
use parity_scale_codec::{Decode, Encode};

pub use gear_core;
pub use gprimitives;

#[derive(Debug, Clone, Default, Encode, Decode)]
pub struct CodeCommitment {
    pub code_id: CodeId,
    pub approved: bool,
}

#[derive(Debug, Clone, Default, Encode, Decode, PartialEq, Eq)]
pub struct StateTransition {
    pub actor_id: ActorId,
    pub old_state_hash: H256,
    pub new_state_hash: H256,
    pub outgoing_messages: Vec<OutgoingMessage>,
}

#[derive(Debug, Clone, Default, Encode, Decode, PartialEq, Eq)]
pub struct OutgoingMessage {
    pub message_id: MessageId,
    pub destination: ActorId,
    pub payload: Payload,
    pub value: u128,
    pub reply_details: Option<ReplyDetails>,
}

#[derive(Debug, Clone, Default, Encode, Decode)]
pub struct BlockCommitment {
    pub block_hash: H256,
    pub allowed_pred_block_hash: H256,
    pub allowed_prev_commitment_hash: H256,
    pub transitions: Vec<StateTransition>,
}

#[derive(Debug, Clone, Default)]
pub struct Commitments {
    pub codes: Vec<CodeCommitment>,
    pub blocks: Vec<BlockCommitment>,
}

impl From<(Vec<CodeCommitment>, Vec<BlockCommitment>)> for Commitments {
    fn from((codes, blocks): (Vec<CodeCommitment>, Vec<BlockCommitment>)) -> Self {
        Self { codes, blocks }
    }
}
