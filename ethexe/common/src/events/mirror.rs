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
use gear_core::message::ReplyCode;
use gprimitives::{ActorId, H256, MessageId};
use parity_scale_codec::{Decode, Encode};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Encode, Decode)]
pub enum Event {
    ExecutableBalanceTopUpRequested {
        value: u128,
    },
    Message {
        id: MessageId,
        destination: ActorId,
        payload: Vec<u8>,
        value: u128,
    },
    MessageQueueingRequested {
        id: MessageId,
        source: ActorId,
        payload: Vec<u8>,
        value: u128,
    },
    Reply {
        payload: Vec<u8>,
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
    pub fn as_request(self) -> Option<RequestEvent> {
        Some(match self {
            Self::ExecutableBalanceTopUpRequested { value } => {
                RequestEvent::ExecutableBalanceTopUpRequested { value }
            }
            Self::MessageQueueingRequested {
                id,
                source,
                payload,
                value,
            } => RequestEvent::MessageQueueingRequested {
                id,
                source,
                payload,
                value,
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
            | Self::Reply { .. } => return None,
        })
    }
}

#[derive(Clone, Debug, Encode, Decode, Serialize, Deserialize)]
pub enum RequestEvent {
    ExecutableBalanceTopUpRequested {
        value: u128,
    },
    MessageQueueingRequested {
        id: MessageId,
        source: ActorId,
        payload: Vec<u8>,
        value: u128,
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
