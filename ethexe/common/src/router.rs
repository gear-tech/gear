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
use gear_core::message::{ReplyDetails, StoredMessage};
use gprimitives::{ActorId, CodeId, MessageId, H256};
use parity_scale_codec::{Decode, Encode};

/* Storage related structures */

#[derive(Clone, Debug, Default, Encode, Decode, PartialEq, Eq)]
pub enum CodeState {
    #[default]
    Unknown,
    ValidationRequested,
    Validated,
}

/* Commitment related structures */

#[derive(Clone, Debug, Default, Encode, Decode, PartialEq, Eq)]
pub struct CodeCommitment {
    pub id: CodeId,
    pub valid: bool,
}

#[derive(Clone, Debug, Default, Encode, Decode, PartialEq, Eq)]
pub struct BlockCommitment {
    pub block_hash: H256,
    pub prev_commitment_hash: H256,
    pub pred_block_hash: H256,
    pub transitions: Vec<StateTransition>,
}

#[derive(Clone, Debug, Default, Encode, Decode, PartialEq, Eq)]
pub struct StateTransition {
    pub actor_id: ActorId,
    pub new_state_hash: H256,
    pub inheritor: ActorId,
    pub value_to_receive: u128,
    pub value_claims: Vec<ValueClaim>,
    pub messages: Vec<OutgoingMessage>,
}

#[derive(Clone, Debug, Default, Encode, Decode, PartialEq, Eq)]
pub struct ValueClaim {
    pub message_id: MessageId,
    pub destination: ActorId,
    pub value: u128,
}

#[derive(Clone, Debug, Default, Encode, Decode, PartialEq, Eq)]
pub struct OutgoingMessage {
    pub id: MessageId,
    pub destination: ActorId,
    pub payload: Vec<u8>,
    pub value: u128,
    pub reply_details: Option<ReplyDetails>,
}

impl From<StoredMessage> for OutgoingMessage {
    fn from(value: StoredMessage) -> Self {
        let (id, _source, destination, payload, value, details) = value.into_parts();
        Self {
            id,
            destination,
            payload: payload.into_vec(),
            value,
            reply_details: details.and_then(|v| v.to_reply_details()),
        }
    }
}

/* Events section */

#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq)]
pub enum Event {
    BaseWeightChanged {
        base_weight: u64,
    },
    BlockCommitted {
        block_hash: H256,
    },
    CodeGotValidated {
        id: CodeId,
        valid: bool,
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

impl Event {
    pub fn as_request(self) -> Option<RequestEvent> {
        Some(match self {
            Self::BaseWeightChanged { base_weight } => {
                RequestEvent::BaseWeightChanged { base_weight }
            }
            Self::CodeValidationRequested {
                code_id,
                blob_tx_hash,
            } => RequestEvent::CodeValidationRequested {
                code_id,
                blob_tx_hash,
            },
            Self::ProgramCreated { actor_id, code_id } => {
                RequestEvent::ProgramCreated { actor_id, code_id }
            }
            Self::StorageSlotChanged => RequestEvent::StorageSlotChanged,
            Self::ValidatorsSetChanged => RequestEvent::ValidatorsSetChanged,
            Self::ValuePerWeightChanged { value_per_weight } => {
                RequestEvent::ValuePerWeightChanged { value_per_weight }
            }
            Self::BlockCommitted { .. } | Self::CodeGotValidated { .. } => return None,
        })
    }
}

#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq)]
pub enum RequestEvent {
    BaseWeightChanged {
        base_weight: u64,
    },
    CodeValidationRequested {
        code_id: CodeId,
        // TODO (breathx): replace with `code: Vec<u8>`
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
