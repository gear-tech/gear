// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Executable message implementations.

use super::{
    stored::{BaseQueueMessage, BaseWaitlistMessage},
    IncrementNonce, MessageKind, OutgoingMessage, OutgoingMessageDetails, WithId,
};
use crate::message::{Payload, ReplyDetails, SignalDetails};
use gear_core_errors::{
    ErrorReplyReason, ReplyCode, SimpleExecutionError, SimpleUnavailableActorError,
    SuccessReplyReason,
};
use gprimitives::{ActorId, MessageId};
use parity_scale_codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

/// Ready-to-be-executed message.
///
/// Consists of the base message and additional necessary data to execute it,
/// such as the message source, message kind, kind-specific data,
/// and the history of previous executions, if any.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Encode,
    Decode,
    MaxEncodedLen,
    TypeInfo,
    derive_more::Deref,
    derive_more::DerefMut,
)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "test-utils", derive(derive_more::From, derive_more::Into))]
pub struct ExecutableMessage(WithId<BaseQueueMessage<Payload>>);

impl ExecutableMessage {
    /// Creates a new executable message with the given queue message with id.
    ///
    /// For internal use only.
    pub(crate) const fn _new(id: MessageId, queue_message: BaseQueueMessage<Payload>) -> Self {
        Self(WithId::new(queue_message, id))
    }

    /// Creates an auto reply message for this executable message.
    ///
    /// Returns `None` if the message is not repliable, which is the case
    /// when it is a reply or signal message.
    pub fn try_auto_reply(&self) -> Option<OutgoingMessage> {
        let payload = Default::default();
        let gas = Some(0);
        let value = 0;
        let code = ReplyCode::Success(SuccessReplyReason::Auto);

        self._try_reply(payload, gas, value, code)
    }

    /// Creates a manual reply message for this executable message.
    ///
    /// Returns `None` if the message is not repliable, which is the case
    /// when it is a reply or signal message.
    pub fn try_reply(
        &self,
        payload: Payload,
        gas: Option<u64>,
        value: u128,
    ) -> Option<OutgoingMessage> {
        let code = ReplyCode::Success(SuccessReplyReason::Manual);

        self._try_reply(payload, gas, value, code)
    }

    /// Creates an error reply for this executable message due to an actor's permanent inactivity (exit).
    ///
    /// Returns `None` if the message is not repliable, which is the case
    /// when it is a reply or signal message.
    pub fn try_exited_error_reply(&self, inheritor: ActorId) -> Option<OutgoingMessage> {
        let payload = Payload::try_from(inheritor.as_ref()).expect("infallible due to small size");
        let reason = ErrorReplyReason::UnavailableActor(SimpleUnavailableActorError::ProgramExited);

        self.try_error_reply(payload, reason)
    }

    /// Creates an error reply for this executable message due to a userspace panic.
    ///
    /// Returns `None` if the message is not repliable, which is the case
    /// when it is a reply or signal message.
    pub fn try_panic_error_reply(&self, payload: Payload) -> Option<OutgoingMessage> {
        let reason = ErrorReplyReason::Execution(SimpleExecutionError::UserspacePanic);

        self.try_error_reply(payload, reason)
    }

    /// Creates an error reply for this executable message.
    ///
    /// Returns `None` if the message is not repliable, which is the case
    /// when it is a reply or signal message.
    pub fn try_error_reply(
        &self,
        payload: Payload,
        reason: ErrorReplyReason,
    ) -> Option<OutgoingMessage> {
        let gas = None;
        let value = if self.has_never_waited() {
            self.value()
        } else {
            0
        };
        let code = ReplyCode::Error(reason);

        self._try_reply(payload, gas, value, code)
    }

    /// Converts the executable message into a waitlist message.
    ///
    /// Increments the number of times the message was placed on the waitlist
    /// in its execution history.
    /// Uses saturating logic to prevent overflow.
    pub fn into_waitlist(self) -> BaseWaitlistMessage<Payload> {
        self.0.into_inner().into_waitlist()
    }

    fn _try_reply(
        &self,
        payload: Payload,
        gas: Option<u64>,
        value: u128,
        code: ReplyCode,
    ) -> Option<OutgoingMessage> {
        self.details()
            .as_kind()
            .is_repliable()
            .then(|| OutgoingMessage::reply(self.id(), code, self.source(), payload, gas, value))
    }
}

/// Kind-specific details for message execution.
///
/// This enum provides additional data required for executing messages based on
/// their kind, such as reply or signal details.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Encode,
    Decode,
    MaxEncodedLen,
    TypeInfo,
    derive_more::Display,
)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub enum ExecutableMessageDetails {
    /// Initialization message details: no additional data is required.
    #[display("init")]
    Init,

    /// Handle message details: no additional data is required.
    #[display("handle")]
    Handle,

    /// Reply message details: provides the message ID being replied to,
    /// as well as the reply code.
    #[display("reply {{ to: {}, code: {} }}", _0.to_message_id(), _0.to_reply_code())]
    Reply(ReplyDetails),

    /// Signal message details: provides the message ID the signal is sent to,
    /// as well as the signal code.
    #[display("signal {{ to: {}, code: {} }}", _0.to_message_id(), _0.to_signal_code())]
    Signal(SignalDetails),
}

impl ExecutableMessageDetails {
    /// Returns the kind of message these details relate to.
    pub fn as_kind(&self) -> MessageKind {
        match self {
            Self::Init => MessageKind::Init,
            Self::Handle => MessageKind::Handle,
            Self::Reply(_) => MessageKind::Reply,
            Self::Signal(_) => MessageKind::Signal,
        }
    }

    /// Returns the reply details if this is a reply message.
    pub fn reply(&self) -> Option<ReplyDetails> {
        match self {
            Self::Reply(details) => Some(*details),
            _ => None,
        }
    }

    /// Returns the signal details if this is a signal message.
    pub fn signal(&self) -> Option<SignalDetails> {
        match self {
            Self::Signal(details) => Some(*details),
            _ => None,
        }
    }
}

impl From<OutgoingMessageDetails> for ExecutableMessageDetails {
    fn from(value: OutgoingMessageDetails) -> Self {
        match value {
            OutgoingMessageDetails::Init { .. } => Self::Init,
            OutgoingMessageDetails::Handle { .. } => Self::Handle,
            OutgoingMessageDetails::Reply(details) => Self::Reply(details),
        }
    }
}

/// Represents the history of the message execution, including the associated nonces.
#[derive(
    Debug,
    Default,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Encode,
    Decode,
    MaxEncodedLen,
    TypeInfo,
)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct ExecutionHistory {
    /// Nonce used for generating outgoing message IDs.
    pub(crate) messaging_nonce: IncrementNonce,
    /// Nonce used for generating reservation IDs.
    pub(crate) reservation_nonce: IncrementNonce,
    /// The amount of gas reserved for executing a signal for this message,
    /// if it occurs.
    pub(crate) system_reservation: Option<u64>,
    /// The number of times the message has been placed on the waitlist.
    ///
    /// This effectively represents the number of times the message has been executed.
    ///
    /// It increments by 1 each time the message is converted into waitlist message.
    /// If it is zero, this indicates the first execution.
    ///
    /// This information is necessary to determine whether to return a value
    /// to the caller in case of error or not.
    pub(crate) waits: u32,
}

impl ExecutionHistory {
    /// Returns the amount of gas reserved for system operations.
    pub fn system_reservation(&self) -> Option<u64> {
        self.system_reservation
    }

    /// Returns the number of times the message has been placed on the waitlist.
    ///
    /// This indicates how many times the message has been executed.
    pub fn waits(&self) -> u32 {
        self.waits
    }
}
