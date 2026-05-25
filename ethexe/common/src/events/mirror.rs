// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use alloc::vec::Vec;
use gear_core::message::ReplyCode;
use gprimitives::{ActorId, H256, MessageId};
use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;

// TODO: consider to sort events in same way as in IMirror.sol

#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, TypeInfo, Hash)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct OwnedBalanceTopUpRequestedEvent {
    pub value: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, TypeInfo, Hash)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct ExecutableBalanceTopUpRequestedEvent {
    pub value: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, TypeInfo, Hash)]
pub struct MessageEvent {
    pub id: MessageId,
    pub destination: ActorId,
    pub payload: Vec<u8>,
    pub value: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, TypeInfo, Hash)]
pub struct MessageCallFailedEvent {
    pub id: MessageId,
    pub destination: ActorId,
    pub value: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, TypeInfo, Hash)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct MessageQueueingRequestedEvent {
    pub id: MessageId,
    pub source: ActorId,
    pub payload: Vec<u8>,
    pub value: u128,
    pub call_reply: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, TypeInfo, Hash)]
pub struct ReplyEvent {
    pub payload: Vec<u8>,
    pub value: u128,
    pub reply_to: MessageId,
    pub reply_code: ReplyCode,
}

#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, TypeInfo, Hash)]
pub struct ReplyCallFailedEvent {
    pub value: u128,
    pub reply_to: MessageId,
    pub reply_code: ReplyCode,
}

#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, TypeInfo, Hash)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct ReplyQueueingRequestedEvent {
    pub replied_to: MessageId,
    pub source: ActorId,
    pub payload: Vec<u8>,
    pub value: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, TypeInfo, Hash)]
pub struct StateChangedEvent {
    pub state_hash: H256,
}

#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, TypeInfo, Hash)]
pub struct ValueClaimedEvent {
    pub claimed_id: MessageId,
    pub value: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, TypeInfo, Hash)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct ValueClaimingRequestedEvent {
    pub claimed_id: MessageId,
    pub source: ActorId,
}

#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, TypeInfo, Hash)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct TransferLockedValueToInheritorFailedEvent {
    pub inheritor: ActorId,
    pub value: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, TypeInfo, Hash)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct ReplyTransferFailedEvent {
    pub destination: ActorId,
    pub value: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, TypeInfo, Hash)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct ValueClaimFailedEvent {
    pub claimed_id: MessageId,
    pub value: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, TypeInfo, Hash)]
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
    TransferLockedValueToInheritorFailed(TransferLockedValueToInheritorFailedEvent),
    ReplyTransferFailed(ReplyTransferFailedEvent),
    ValueClaimFailed(ValueClaimFailedEvent),
}

impl Event {
    pub fn to_request(self) -> Option<RequestEvent> {
        Some(match self {
            Self::OwnedBalanceTopUpRequested(event) => {
                RequestEvent::OwnedBalanceTopUpRequested(event)
            }
            Self::ExecutableBalanceTopUpRequested(event) => {
                RequestEvent::ExecutableBalanceTopUpRequested(event)
            }
            Self::MessageQueueingRequested(event) => RequestEvent::MessageQueueingRequested(event),
            Self::ReplyQueueingRequested(event) => RequestEvent::ReplyQueueingRequested(event),
            Self::ValueClaimingRequested(event) => RequestEvent::ValueClaimingRequested(event),
            Self::StateChanged(_)
            | Self::ValueClaimed(_)
            | Self::Message(_)
            | Self::MessageCallFailed(_)
            | Self::Reply(_)
            | Self::ReplyCallFailed(_)
            | Self::TransferLockedValueToInheritorFailed(_)
            | Self::ReplyTransferFailed(_)
            | Self::ValueClaimFailed(_) => return None,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub enum RequestEvent {
    OwnedBalanceTopUpRequested(OwnedBalanceTopUpRequestedEvent),
    ExecutableBalanceTopUpRequested(ExecutableBalanceTopUpRequestedEvent),
    MessageQueueingRequested(MessageQueueingRequestedEvent),
    ReplyQueueingRequested(ReplyQueueingRequestedEvent),
    ValueClaimingRequested(ValueClaimingRequestedEvent),
}
