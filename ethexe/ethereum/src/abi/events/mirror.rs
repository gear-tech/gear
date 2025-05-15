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

use crate::abi::{utils::*, IMirror};
use ethexe_common::events::MirrorEvent;
use gear_core_errors::ReplyCode;

impl From<IMirror::StateChanged> for MirrorEvent {
    fn from(value: IMirror::StateChanged) -> Self {
        Self::StateChanged {
            state_hash: bytes32_to_h256(value.stateHash),
        }
    }
}

impl From<IMirror::MessageQueueingRequested> for MirrorEvent {
    fn from(value: IMirror::MessageQueueingRequested) -> Self {
        Self::MessageQueueingRequested {
            id: bytes32_to_message_id(value.id),
            source: address_to_actor_id(value.source),
            payload: value.payload.into(),
            value: value.value,
            call_reply: value.callReply,
        }
    }
}
impl From<IMirror::ReplyQueueingRequested> for MirrorEvent {
    fn from(value: IMirror::ReplyQueueingRequested) -> Self {
        Self::ReplyQueueingRequested {
            replied_to: bytes32_to_message_id(value.repliedTo),
            source: address_to_actor_id(value.source),
            payload: value.payload.into(),
            value: value.value,
        }
    }
}
impl From<IMirror::ValueClaimingRequested> for MirrorEvent {
    fn from(value: IMirror::ValueClaimingRequested) -> Self {
        Self::ValueClaimingRequested {
            claimed_id: bytes32_to_message_id(value.claimedId),
            source: address_to_actor_id(value.source),
        }
    }
}

impl From<IMirror::ExecutableBalanceTopUpRequested> for MirrorEvent {
    fn from(value: IMirror::ExecutableBalanceTopUpRequested) -> Self {
        Self::ExecutableBalanceTopUpRequested { value: value.value }
    }
}

impl From<IMirror::Message> for MirrorEvent {
    fn from(value: IMirror::Message) -> Self {
        Self::Message {
            id: bytes32_to_message_id(value.id),
            destination: address_to_actor_id(value.destination),
            payload: value.payload.into(),
            value: value.value,
        }
    }
}

impl From<IMirror::MessageCallFailed> for MirrorEvent {
    fn from(value: IMirror::MessageCallFailed) -> Self {
        Self::MessageCallFailed {
            id: bytes32_to_message_id(value.id),
            destination: address_to_actor_id(value.destination),
            value: value.value,
        }
    }
}

impl From<IMirror::Reply> for MirrorEvent {
    fn from(value: IMirror::Reply) -> Self {
        Self::Reply {
            payload: value.payload.into(),
            value: value.value,
            reply_to: bytes32_to_message_id(value.replyTo),
            reply_code: ReplyCode::from_bytes(*value.replyCode),
        }
    }
}

impl From<IMirror::ReplyCallFailed> for MirrorEvent {
    fn from(value: IMirror::ReplyCallFailed) -> Self {
        Self::ReplyCallFailed {
            value: value.value,
            reply_to: bytes32_to_message_id(value.replyTo),
            reply_code: ReplyCode::from_bytes(*value.replyCode),
        }
    }
}

impl From<IMirror::ValueClaimed> for MirrorEvent {
    fn from(value: IMirror::ValueClaimed) -> Self {
        Self::ValueClaimed {
            claimed_id: bytes32_to_message_id(value.claimedId),
            value: value.value,
        }
    }
}
