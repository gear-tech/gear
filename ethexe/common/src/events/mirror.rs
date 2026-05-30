// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use alloc::vec::Vec;
use gear_core::message::ReplyCode;
use gprimitives::{ActorId, H256, MessageId};
use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;

// TODO: consider to sort events in same way as in IMirror.sol

/// Emitted by a Mirror contract when a user requests to top up its owned (non-executable) balance.
#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, TypeInfo, Hash)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct OwnedBalanceTopUpRequestedEvent {
    /// Amount of value (in native token units) requested to be added.
    pub value: u128,
}

/// Emitted by a Mirror contract when a user requests to top up the program's executable balance.
#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, TypeInfo, Hash)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct ExecutableBalanceTopUpRequestedEvent {
    /// Amount of value (in native token units) requested to be added.
    pub value: u128,
}

/// Emitted by a Mirror contract when an outgoing message from the program is successfully delivered.
#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, TypeInfo, Hash)]
pub struct MessageEvent {
    /// Unique identifier of the outgoing message.
    pub id: MessageId,
    /// Actor the message was sent to.
    pub destination: ActorId,
    /// Encoded message payload.
    pub payload: Vec<u8>,
    /// Value transferred with the message (in native token units).
    pub value: u128,
}

/// Emitted by a Mirror contract when a message call to the destination actor failed.
#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, TypeInfo, Hash)]
pub struct MessageCallFailedEvent {
    /// Unique identifier of the message whose call failed.
    pub id: MessageId,
    /// Intended destination actor for the failed call.
    pub destination: ActorId,
    /// Value that was attached to the failed call (in native token units).
    pub value: u128,
}

/// Emitted by a Mirror contract when a user requests to queue a new message for the program.
#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, TypeInfo, Hash)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct MessageQueueingRequestedEvent {
    /// Unique identifier assigned to the queued message.
    pub id: MessageId,
    /// Actor that submitted the queueing request.
    pub source: ActorId,
    /// Encoded message payload.
    pub payload: Vec<u8>,
    /// Value attached to the message (in native token units).
    pub value: u128,
    /// Whether the message is a reply call rather than a plain handle dispatch.
    pub call_reply: bool,
}

/// Emitted by a Mirror contract when the program sends a reply message successfully.
#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, TypeInfo, Hash)]
pub struct ReplyEvent {
    /// Encoded reply payload.
    pub payload: Vec<u8>,
    /// Value transferred with the reply (in native token units).
    pub value: u128,
    /// Identifier of the message this reply is responding to.
    pub reply_to: MessageId,
    /// Code indicating the outcome of the reply (success, error category, etc.).
    pub reply_code: ReplyCode,
}

/// Emitted by a Mirror contract when a reply call from the program failed to be delivered.
#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, TypeInfo, Hash)]
pub struct ReplyCallFailedEvent {
    /// Value that was attached to the failed reply (in native token units).
    pub value: u128,
    /// Identifier of the message that was being replied to.
    pub reply_to: MessageId,
    /// Code indicating the intended outcome of the reply.
    pub reply_code: ReplyCode,
}

/// Emitted by a Mirror contract when a user requests to queue a reply to a specific message.
#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, TypeInfo, Hash)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct ReplyQueueingRequestedEvent {
    /// Identifier of the message being replied to.
    pub replied_to: MessageId,
    /// Actor that submitted the reply queueing request.
    pub source: ActorId,
    /// Encoded reply payload.
    pub payload: Vec<u8>,
    /// Value attached to the reply (in native token units).
    pub value: u128,
}

/// Emitted by a Mirror contract after the program's state hash is updated following execution.
#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, TypeInfo, Hash)]
pub struct StateChangedEvent {
    /// New state hash stored in the Mirror after the state transition.
    pub state_hash: H256,
}

/// Emitted by a Mirror contract when a user successfully claims value from a mailbox message.
#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, TypeInfo, Hash)]
pub struct ValueClaimedEvent {
    /// Identifier of the mailbox message whose value was claimed.
    pub claimed_id: MessageId,
    /// Amount of value claimed (in native token units).
    pub value: u128,
}

/// Emitted by a Mirror contract when a user requests to claim value from a mailbox message.
#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, TypeInfo, Hash)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct ValueClaimingRequestedEvent {
    /// Identifier of the mailbox message to claim value from.
    pub claimed_id: MessageId,
    /// Actor that submitted the claim request.
    pub source: ActorId,
}

/// Emitted by a Mirror contract when transferring locked value to the program's inheritor failed.
#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, TypeInfo, Hash)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct TransferLockedValueToInheritorFailedEvent {
    /// Actor designated as the inheritor that was the intended recipient.
    pub inheritor: ActorId,
    /// Amount of value that could not be transferred (in native token units).
    pub value: u128,
}

/// Emitted by a Mirror contract when the value transfer accompanying a reply failed.
#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, TypeInfo, Hash)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct ReplyTransferFailedEvent {
    /// Actor that was the intended recipient of the transferred value.
    pub destination: ActorId,
    /// Amount of value that failed to transfer (in native token units).
    pub value: u128,
}

/// Emitted by a Mirror contract when a value claim attempt for a mailbox message failed.
#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, TypeInfo, Hash)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct ValueClaimFailedEvent {
    /// Identifier of the mailbox message whose value could not be claimed.
    pub claimed_id: MessageId,
    /// Amount of value that failed to be claimed (in native token units).
    pub value: u128,
}

/// All events that a Mirror contract can emit, covering both user requests and execution outcomes.
#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, TypeInfo, Hash)]
pub enum Event {
    /// A user requested to top up the program's owned (non-executable) balance.
    OwnedBalanceTopUpRequested(OwnedBalanceTopUpRequestedEvent),
    /// A user requested to top up the program's executable balance.
    ExecutableBalanceTopUpRequested(ExecutableBalanceTopUpRequestedEvent),
    /// The program emitted an outgoing message that was successfully delivered.
    Message(MessageEvent),
    /// An outgoing message call from the program failed.
    MessageCallFailed(MessageCallFailedEvent),
    /// A user requested to queue a new message for the program.
    MessageQueueingRequested(MessageQueueingRequestedEvent),
    /// The program sent a reply message successfully.
    Reply(ReplyEvent),
    /// A reply call from the program failed.
    ReplyCallFailed(ReplyCallFailedEvent),
    /// A user requested to queue a reply for the program.
    ReplyQueueingRequested(ReplyQueueingRequestedEvent),
    /// The program's state hash was updated after execution.
    StateChanged(StateChangedEvent),
    /// A user successfully claimed value from a mailbox message.
    ValueClaimed(ValueClaimedEvent),
    /// A user requested to claim value from a mailbox message.
    ValueClaimingRequested(ValueClaimingRequestedEvent),
    /// Transferring locked value to the program's inheritor failed.
    TransferLockedValueToInheritorFailed(TransferLockedValueToInheritorFailedEvent),
    /// The value transfer accompanying a reply failed.
    ReplyTransferFailed(ReplyTransferFailedEvent),
    /// A value claim attempt for a mailbox message failed.
    ValueClaimFailed(ValueClaimFailedEvent),
}

impl Event {
    /// Converts this event into a [`RequestEvent`] if it represents a user-initiated request.
    ///
    /// Returns `None` for execution-outcome variants (`StateChanged`, `ValueClaimed`,
    /// `Message`, `MessageCallFailed`, `Reply`, `ReplyCallFailed`,
    /// `TransferLockedValueToInheritorFailed`, `ReplyTransferFailed`, `ValueClaimFailed`).
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

/// Subset of [`Event`] containing only user-initiated request events from a Mirror contract.
///
/// These are the events that drive state transitions on the observer side;
/// execution-outcome events are excluded. Produced via [`Event::to_request`].
#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub enum RequestEvent {
    /// A user requested to top up the program's owned balance.
    OwnedBalanceTopUpRequested(OwnedBalanceTopUpRequestedEvent),
    /// A user requested to top up the program's executable balance.
    ExecutableBalanceTopUpRequested(ExecutableBalanceTopUpRequestedEvent),
    /// A user requested to queue a new message for the program.
    MessageQueueingRequested(MessageQueueingRequestedEvent),
    /// A user requested to queue a reply for the program.
    ReplyQueueingRequested(ReplyQueueingRequestedEvent),
    /// A user requested to claim value from a mailbox message.
    ValueClaimingRequested(ValueClaimingRequestedEvent),
}
