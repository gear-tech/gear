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

use alloc::vec::Vec;
use gear_core::message::ReplyCode;
use gprimitives::{ActorId, H256, MessageId};
use parity_scale_codec::{Decode, Encode};

#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, Hash)]
pub enum Event {
    ReducibleBalanceTopUpRequested {
        value: u128,
    },
    ExecutableBalanceTopUpRequested {
        value: u128,
    },
    Message {
        id: MessageId,
        destination: ActorId,
        payload: Vec<u8>,
        value: u128,
    },
    MessageCallFailed {
        id: MessageId,
        destination: ActorId,
        value: u128,
    },
    MessageQueueingRequested {
        id: MessageId,
        source: ActorId,
        payload: Vec<u8>,
        value: u128,
        call_reply: bool,
    },
    Reply {
        payload: Vec<u8>,
        value: u128,
        reply_to: MessageId,
        reply_code: ReplyCode,
    },
    ReplyCallFailed {
        value: u128,
        reply_to: MessageId,
        reply_code: ReplyCode,
    },
    ReplyQueueingRequested {
        replied_to: MessageId,
        source: ActorId,
        payload: Vec<u8>,
        value: u128,
    },
    StateChanged {
        state_hash: H256,
    },
    ValueClaimed {
        claimed_id: MessageId,
        value: u128,
    },
    ValueClaimingRequested {
        claimed_id: MessageId,
        source: ActorId,
    },
}

impl Event {
    pub fn to_request(self) -> Option<RequestEvent> {
        Some(match self {
            Self::ReducibleBalanceTopUpRequested { value } => {
                RequestEvent::ReducibleBalanceTopUpRequested { value }
            }
            Self::ExecutableBalanceTopUpRequested { value } => {
                RequestEvent::ExecutableBalanceTopUpRequested { value }
            }
            Self::MessageQueueingRequested {
                id,
                source,
                payload,
                value,
                call_reply,
            } => RequestEvent::MessageQueueingRequested {
                id,
                source,
                payload,
                value,
                call_reply,
            },
            Self::ReplyQueueingRequested {
                replied_to,
                source,
                payload,
                value,
            } => RequestEvent::ReplyQueueingRequested {
                replied_to,
                source,
                payload,
                value,
            },
            Self::ValueClaimingRequested { claimed_id, source } => {
                RequestEvent::ValueClaimingRequested { claimed_id, source }
            }
            Self::StateChanged { .. }
            | Self::ValueClaimed { .. }
            | Self::Message { .. }
            | Self::MessageCallFailed { .. }
            | Self::Reply { .. }
            | Self::ReplyCallFailed { .. } => return None,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub enum RequestEvent {
    ReducibleBalanceTopUpRequested {
        value: u128,
    },
    ExecutableBalanceTopUpRequested {
        value: u128,
    },
    MessageQueueingRequested {
        id: MessageId,
        source: ActorId,
        payload: Vec<u8>,
        value: u128,
        call_reply: bool,
    },
    ReplyQueueingRequested {
        replied_to: MessageId,
        source: ActorId,
        payload: Vec<u8>,
        value: u128,
    },
    ValueClaimingRequested {
        claimed_id: MessageId,
        source: ActorId,
    },
}
