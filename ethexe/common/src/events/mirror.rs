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

// TODO: consider to sort events in same way as in IMirror.sol

#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, Hash)]
pub struct OwnedBalanceTopUpRequestedEvent {
    pub value: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, Hash)]
pub struct ExecutableBalanceTopUpRequestedEvent {
    pub value: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, Hash)]
pub struct MessageEvent {
    pub id: MessageId,
    pub destination: ActorId,
    pub payload: Vec<u8>,
    pub value: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, Hash)]
pub struct MessageCallFailedEvent {
    pub id: MessageId,
    pub destination: ActorId,
    pub value: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, Hash)]
pub struct MessageQueueingRequestedEvent {
    pub id: MessageId,
    pub source: ActorId,
    pub payload: Vec<u8>,
    pub value: u128,
    pub call_reply: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, Hash)]
pub struct ReplyEvent {
    pub payload: Vec<u8>,
    pub value: u128,
    pub reply_to: MessageId,
    pub reply_code: ReplyCode,
}

#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, Hash)]
pub struct ReplyCallFailedEvent {
    pub value: u128,
    pub reply_to: MessageId,
    pub reply_code: ReplyCode,
}

#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, Hash)]
pub struct ReplyQueueingRequestedEvent {
    pub replied_to: MessageId,
    pub source: ActorId,
    pub payload: Vec<u8>,
    pub value: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, Hash)]
pub struct StateChangedEvent {
    pub state_hash: H256,
}

#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, Hash)]
pub struct ValueClaimedEvent {
    pub claimed_id: MessageId,
    pub value: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, Hash)]
pub struct ValueClaimingRequestedEvent {
    pub claimed_id: MessageId,
    pub source: ActorId,
}

#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, Hash)]
pub enum Event {
    OwnedBalanceTopUpRequested(OwnedBalanceTopUpRequestedEvent),
    ExecutableBalanceTopUpRequested(ExecutableBalanceTopUpRequestedEvent),
    Message(MessageEvent),
    MessageCallFailed(MessageCallFailedEvent),
    MessageQueueingRequested(MessageQueueingRequestedEvent),
    Reply(ReplyEvent),
    ReplyCallFailed(ReplyCallFailedEvent),
    ReplyQueueingRequested(ReplyQueueingRequestedEvent),
    StateChanged(StateChangedEvent),
    ValueClaimed(ValueClaimedEvent),
    ValueClaimingRequested(ValueClaimingRequestedEvent),
}

impl Event {
    pub fn to_request(self) -> Option<RequestEvent> {
        Some(match self {
            Self::OwnedBalanceTopUpRequested(event) => {
                RequestEvent::OwnedBalanceTopUpRequested { value: event.value }
            }
            Self::ExecutableBalanceTopUpRequested(event) => {
                RequestEvent::ExecutableBalanceTopUpRequested { value: event.value }
            }
            Self::MessageQueueingRequested(event) => RequestEvent::MessageQueueingRequested {
                id: event.id,
                source: event.source,
                payload: event.payload,
                value: event.value,
                call_reply: event.call_reply,
            },
            Self::ReplyQueueingRequested(event) => RequestEvent::ReplyQueueingRequested {
                replied_to: event.replied_to,
                source: event.source,
                payload: event.payload,
                value: event.value,
            },
            Self::ValueClaimingRequested(event) => RequestEvent::ValueClaimingRequested {
                claimed_id: event.claimed_id,
                source: event.source,
            },
            Self::StateChanged(_)
            | Self::ValueClaimed(_)
            | Self::Message(_)
            | Self::MessageCallFailed(_)
            | Self::Reply(_)
            | Self::ReplyCallFailed(_) => return None,
        })
    }
}

// TODO: consider to refactor in the same way

#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub enum RequestEvent {
    OwnedBalanceTopUpRequested {
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
