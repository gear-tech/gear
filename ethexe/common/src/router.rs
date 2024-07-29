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
use parity_scale_codec::{Decode, Encode};

/* Storage related structures */

#[derive(Clone, Debug, Default, Encode, Decode)]
pub enum CodeState {
    #[default]
    Unknown,
    ValidationRequested,
    Validated,
}

/* Commitment related structures */

#[derive(Clone, Debug, Default, Encode, Decode)]
pub struct CodeCommitment {
    pub id: CodeId,
    pub valid: bool,
}

#[derive(Clone, Debug, Default, Encode, Decode)]
pub struct BlockCommitment {
    pub block_hash: H256,
    pub prev_commitment_hash: H256,
    pub pred_block_hash: H256,
    pub transitions: Vec<StateTransition>,
}

#[derive(Clone, Debug, Default, Encode, Decode)]
pub struct StateTransition {
    pub actor_id: ActorId,
    pub prev_state_hash: H256,
    pub new_state_hash: H256,
    pub value_to_receive: u128,
    pub value_claims: Vec<ValueClaim>,
    pub messages: Vec<OutgoingMessage>,
}

#[derive(Clone, Debug, Default, Encode, Decode)]
pub struct ValueClaim {
    pub message_id: MessageId,
    pub destination: ActorId,
    pub value: u128,
}

#[derive(Clone, Debug, Default, Encode, Decode)]
pub struct OutgoingMessage {
    pub id: MessageId,
    pub destination: ActorId,
    pub payload: Vec<u8>,
    pub value: u128,
    pub reply_details: ReplyDetails,
}

#[derive(Clone, Debug, Default, Encode, Decode)]
pub struct ReplyDetails {
    pub to: MessageId,
    pub code: [u8; 4],
}

/* Events section */

#[derive(Clone, Debug, Encode, Decode)]
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
    CodeValidationRequested {
        code_id: CodeId,
        /// This field is replaced with tx hash in case of zero.
        blob_tx_hash: H256,
    },
    ProgramCreated {
        actor_id: ActorId,
        code_id: CodeId,
    },
    StorageSlotChanged,
    ValidatorsSetChanged,
    ValuePerWeightChanged {
        value_per_weight: u128,
    },
}
