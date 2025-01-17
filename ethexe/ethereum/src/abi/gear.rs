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

use crate::abi::{utils::*, Gear};
use ethexe_common::gear::*;
use gear_core::message::ReplyDetails;

//                          //
// From Rust types to alloy //
//                          //

impl From<BlockCommitment> for Gear::BlockCommitment {
    fn from(value: BlockCommitment) -> Self {
        Self {
            hash: h256_to_bytes32(value.hash),
            timestamp: u64_to_uint48_lossy(value.timestamp),
            previousCommittedBlock: h256_to_bytes32(value.previous_committed_block),
            predecessorBlock: h256_to_bytes32(value.predecessor_block),
            transitions: value.transitions.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<CodeCommitment> for Gear::CodeCommitment {
    fn from(value: CodeCommitment) -> Self {
        Self {
            id: code_id_to_bytes32(value.id),
            valid: value.valid,
        }
    }
}

impl From<ValidatorsCommitment> for Gear::ValidatorsCommitment {
    fn from(value: ValidatorsCommitment) -> Self {
        Self {
            validators: value
                .validators
                .into_iter()
                .map(actor_id_to_address_lossy)
                .collect(),
            eraIndex: Uint256::from(value.era_index),
        }
    }
}

impl From<Message> for Gear::Message {
    fn from(value: Message) -> Self {
        Self {
            id: message_id_to_bytes32(value.id),
            destination: actor_id_to_address_lossy(value.destination),
            payload: value.payload.into(),
            value: value.value,
            replyDetails: value.reply_details.into(),
        }
    }
}

impl From<Option<ReplyDetails>> for Gear::ReplyDetails {
    fn from(value: Option<ReplyDetails>) -> Self {
        value.unwrap_or_default().into()
    }
}

impl From<ReplyDetails> for Gear::ReplyDetails {
    fn from(value: ReplyDetails) -> Self {
        let (to, code) = value.into_parts();

        Self {
            to: message_id_to_bytes32(to),
            code: code.to_bytes().into(),
        }
    }
}

impl From<StateTransition> for Gear::StateTransition {
    fn from(value: StateTransition) -> Self {
        Self {
            actorId: actor_id_to_address_lossy(value.actor_id),
            newStateHash: h256_to_bytes32(value.new_state_hash),
            inheritor: actor_id_to_address_lossy(value.inheritor),
            valueToReceive: value.value_to_receive,
            valueClaims: value.value_claims.into_iter().map(Into::into).collect(),
            messages: value.messages.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<ValueClaim> for Gear::ValueClaim {
    fn from(value: ValueClaim) -> Self {
        Self {
            messageId: message_id_to_bytes32(value.message_id),
            destination: actor_id_to_address_lossy(value.destination),
            value: value.value,
        }
    }
}
