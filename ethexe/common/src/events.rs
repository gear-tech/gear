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

//! ethexe events.

use alloc::vec::Vec;
use gear_core::{
    ids::{ActorId, CodeId, MessageId},
    message::ReplyDetails,
};
use gprimitives::H256;
use parity_scale_codec::{Decode, Encode};

#[derive(Debug, Clone, Encode, Decode, derive_more::From)]
pub enum BlockEvent {
    CodeApproved(CodeApproved),
    CodeRejected(CodeRejected),
    CreateProgram(CreateProgram),
    UserMessageSent(UserMessageSent),
    UserReplySent(UserReplySent),
    UpdatedProgram(UpdatedProgram),
    SendMessage(SendMessage),
    SendReply(SendReply),
    ClaimValue(ClaimValue),
    BlockCommitted(BlockCommitted),

    /// Special event: [`UploadCode`] event detected in the block,
    /// but the code is not yet loaded from the chain.
    UploadCode(PendingUploadCode),
}

#[derive(Debug, Clone, Decode, Encode)]
pub struct UploadCode {
    pub origin: ActorId,
    pub code_id: CodeId,
    pub blob_tx: H256,
}

#[derive(Debug, Clone, Decode, Encode)]
pub struct CodeApproved {
    pub code_id: CodeId,
}

#[derive(Debug, Clone, Decode, Encode)]
pub struct CodeRejected {
    pub code_id: CodeId,
}

#[derive(Debug, Clone, Decode, Encode)]
pub struct CreateProgram {
    pub origin: ActorId,
    pub actor_id: ActorId,
    pub code_id: CodeId,
    pub init_payload: Vec<u8>,
    pub gas_limit: u64, // TODO (breathx): remove me
    pub value: u128,
}

#[derive(Debug, Clone, Encode, Decode)]
pub struct UpdatedProgram {
    pub actor_id: ActorId,
    pub old_state_hash: H256,
    pub new_state_hash: H256,
}

#[derive(Debug, Clone, Decode, Encode)]
pub struct UserMessageSent {
    pub destination: ActorId,
    pub payload: Vec<u8>,
    pub value: u128,
}

#[derive(Debug, Clone, Decode, Encode)]
pub struct UserReplySent {
    pub destination: ActorId,
    pub payload: Vec<u8>,
    pub value: u128,
    pub reply_details: ReplyDetails,
}

#[derive(Debug, Clone, Decode, Encode)]
pub struct SendMessage {
    pub origin: ActorId,
    pub destination: ActorId,
    pub payload: Vec<u8>,
    pub gas_limit: u64, // TODO (breathx): remove me
    pub value: u128,
}

#[derive(Debug, Clone, Decode, Encode)]
pub struct SendReply {
    pub origin: ActorId,
    pub reply_to_id: MessageId,
    pub payload: Vec<u8>,
    pub gas_limit: u64, // TODO (breathx): remove me
    pub value: u128,
}

#[derive(Debug, Clone, Decode, Encode)]
pub struct ClaimValue {
    pub origin: ActorId,
    pub message_id: MessageId,
}

#[derive(Debug, Clone, Decode, Encode)]
pub struct BlockCommitted {
    pub block_hash: H256,
}

#[derive(Debug, Clone, Encode, Decode)]
pub struct PendingUploadCode {
    pub origin: ActorId,
    pub code_id: CodeId,
    pub blob_tx: H256,
    pub tx_hash: H256,
}

impl PendingUploadCode {
    pub fn blob_tx(&self) -> H256 {
        if self.blob_tx.is_zero() {
            self.tx_hash
        } else {
            self.blob_tx
        }
    }
}
