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

use alloc::vec::Vec;
use gprimitives::{ActorId, CodeId, MessageId, H256};

/* Storage related structures */

pub enum CodeState {
    Unknown,
    ValidationRequested,
    Validated,
}

/* Commitment related structures */

pub struct CodeCommitment {
    pub code_id: CodeId,
    // TODO (breathx): rename into `validated`?
    pub approved: bool,
}

pub struct BlockCommitment {
    pub block_hash: H256,
    // TODO (breathx): rename removed "allowed"?
    pub allowed_prev_commitment_hash: H256,
    pub allowed_pred_block_hash: H256,
    pub transitions: Vec<StateTransition>,
}

pub struct StateTransition {
    pub actor_id: ActorId,
    // TODO (breathx): rename into `pre_state_hash`?
    pub old_state_hash: H256,
    pub new_state_hash: H256,
    // TODO (breathx): rename into `messages`?
    pub outgoing_messages: Vec<OutgoingMessage>,
}

pub struct OutgoingMessage {
    // TODO (breathx): rename into `id`?
    pub message_id: MessageId,
    pub destination: ActorId,
    pub payload: Vec<u8>,
    pub value: u128,
    pub reply_details: ReplyDetails,
}

pub struct ReplyDetails {
    pub reply_to: MessageId,
    pub reply_code: [u8; 4],
}

/* Events section */

pub enum Event {
    BaseWeightChanged {
        base_weight: u64,
    },
    BlockCommitted {
        block_hash: H256,
    },
    CodeFailedValidation {
        code_id: CodeId,
    },
    CodeGotValidated {
        code_id: CodeId,
    },
    // TODO (breathx): remove origin from here.
    CodeValidationRequested {
        origin: ActorId,
        code_id: CodeId,
        /// This field is replaced with tx hash in case of zero.
        blob_tx_hash: H256,
    },
    // TODO (breathx): remove origin from here.
    ProgramCreated {
        origin: ActorId,
        actor_id: ActorId,
        code_id: CodeId,
    },
    StorageSlotChanged,
    ValidatorsSetChanged,
    ValuePerWeightChanged {
        value_per_weight: u128,
    },
}
