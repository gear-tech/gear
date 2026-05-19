// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::abi::{IMirror, utils::*};
use ethexe_common::events::mirror::*;
use gear_core_errors::ReplyCode;

impl From<IMirror::StateChanged> for StateChangedEvent {
    fn from(value: IMirror::StateChanged) -> Self {
        Self {
            state_hash: bytes32_to_h256(value.stateHash),
        }
    }
}

impl From<IMirror::MessageQueueingRequested> for MessageQueueingRequestedEvent {
    fn from(value: IMirror::MessageQueueingRequested) -> Self {
        Self {
            id: bytes32_to_message_id(value.id),
            source: address_to_actor_id(value.source),
            payload: value.payload.into(),
            value: value.value,
            call_reply: value.callReply,
        }
    }
}
impl From<IMirror::ReplyQueueingRequested> for ReplyQueueingRequestedEvent {
    fn from(value: IMirror::ReplyQueueingRequested) -> Self {
        Self {
            replied_to: bytes32_to_message_id(value.repliedTo),
            source: address_to_actor_id(value.source),
            payload: value.payload.into(),
            value: value.value,
        }
    }
}
impl From<IMirror::ValueClaimingRequested> for ValueClaimingRequestedEvent {
    fn from(value: IMirror::ValueClaimingRequested) -> Self {
        Self {
            claimed_id: bytes32_to_message_id(value.claimedId),
            source: address_to_actor_id(value.source),
        }
    }
}

impl From<IMirror::OwnedBalanceTopUpRequested> for OwnedBalanceTopUpRequestedEvent {
    fn from(value: IMirror::OwnedBalanceTopUpRequested) -> Self {
        Self { value: value.value }
    }
}

impl From<IMirror::ExecutableBalanceTopUpRequested> for ExecutableBalanceTopUpRequestedEvent {
    fn from(value: IMirror::ExecutableBalanceTopUpRequested) -> Self {
        Self { value: value.value }
    }
}

impl From<IMirror::Message> for MessageEvent {
    fn from(value: IMirror::Message) -> Self {
        Self {
            id: bytes32_to_message_id(value.id),
            destination: address_to_actor_id(value.destination),
            payload: value.payload.into(),
            value: value.value,
        }
    }
}

impl From<IMirror::MessageCallFailed> for MessageCallFailedEvent {
    fn from(value: IMirror::MessageCallFailed) -> Self {
        Self {
            id: bytes32_to_message_id(value.id),
            destination: address_to_actor_id(value.destination),
            value: value.value,
        }
    }
}

impl From<IMirror::Reply> for ReplyEvent {
    fn from(value: IMirror::Reply) -> Self {
        Self {
            payload: value.payload.into(),
            value: value.value,
            reply_to: bytes32_to_message_id(value.replyTo),
            reply_code: ReplyCode::from_bytes(*value.replyCode),
        }
    }
}

impl From<IMirror::ReplyCallFailed> for ReplyCallFailedEvent {
    fn from(value: IMirror::ReplyCallFailed) -> Self {
        Self {
            value: value.value,
            reply_to: bytes32_to_message_id(value.replyTo),
            reply_code: ReplyCode::from_bytes(*value.replyCode),
        }
    }
}

impl From<IMirror::ValueClaimed> for ValueClaimedEvent {
    fn from(value: IMirror::ValueClaimed) -> Self {
        Self {
            claimed_id: bytes32_to_message_id(value.claimedId),
            value: value.value,
        }
    }
}

impl From<IMirror::TransferLockedValueToInheritorFailed>
    for TransferLockedValueToInheritorFailedEvent
{
    fn from(value: IMirror::TransferLockedValueToInheritorFailed) -> Self {
        Self {
            inheritor: address_to_actor_id(value.inheritor),
            value: value.value,
        }
    }
}

impl From<IMirror::ReplyTransferFailed> for ReplyTransferFailedEvent {
    fn from(value: IMirror::ReplyTransferFailed) -> Self {
        Self {
            destination: address_to_actor_id(value.destination),
            value: value.value,
        }
    }
}

impl From<IMirror::ValueClaimFailed> for ValueClaimFailedEvent {
    fn from(value: IMirror::ValueClaimFailed) -> Self {
        Self {
            claimed_id: bytes32_to_message_id(value.claimedId),
            value: value.value,
        }
    }
}
