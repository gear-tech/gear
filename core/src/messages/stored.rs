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

//! Implementations for storing messages.

use super::{
    utils::WithSource, BlankMessage, ExecutableMessage, ExecutableMessageDetails, ExecutionHistory,
    MessageKind, OutgoingMessage, WithId, WrapWithDestination, WrapWithId, WrapWithSource,
};
use crate::{
    buffer::Payload,
    ids::prelude::{ActorIdExt, MessageIdExt},
};
use gear_core_errors::{ErrorReplyReason, ReplyCode, SignalCode, SuccessReplyReason};
use gprimitives::{ActorId, MessageId};
use parity_scale_codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

/// Base message type used for storing messages in a mailbox.
///
/// Intended usage due to platform specifics and data storing characteristics:
/// - GearExe: `BaseMailboxMessage<PayloadOrHash>`;
/// - Vara: `WithSource<BaseMailboxMessage<Payload>>`.
#[derive(
    Debug,
    Clone,
    Copy,
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
pub struct BaseMailboxMessage<P>(BlankMessage<P>);

impl<P> BaseMailboxMessage<P> {
    /// Creates a new mailbox message with the given blank message.
    ///
    /// For internal use only.
    pub(crate) const fn _new(blank: BlankMessage<P>) -> Self {
        Self(blank)
    }
}

impl<P> WrapWithDestination for BaseMailboxMessage<P> {}
impl<P> WrapWithId for BaseMailboxMessage<P> {}
impl<P> WrapWithSource for BaseMailboxMessage<P> {}

impl<P> WithId<WithSource<BaseMailboxMessage<P>>> {
    /// Creates an auto reply message for this mailbox message.
    pub fn auto_reply(&self) -> OutgoingMessage {
        let code = ReplyCode::Success(SuccessReplyReason::Auto);
        let payload = Default::default();
        let gas = Some(0);
        let value = 0;

        OutgoingMessage::reply(self.id(), code, self.source(), payload, gas, value)
    }

    /// Creates a manual reply message for this mailbox message.
    pub fn reply(&self, payload: Payload, gas: u64, value: u128) -> OutgoingMessage {
        let code = ReplyCode::Success(SuccessReplyReason::Manual);

        OutgoingMessage::reply(self.id(), code, self.source(), payload, Some(gas), value)
    }
}

/// Base message type used for storing messages in a message queue.
///
/// Intended usage due to platform specifics and data storing characteristics:
/// - GearExe: `WithId<BaseQueueMessage<PayloadOrHash>>`;
/// - Vara: `WithDestination<BaseQueueMessage<Payload>>`.
#[derive(
    Debug,
    Clone,
    Copy,
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
pub struct BaseQueueMessage<P> {
    #[deref]
    #[deref_mut]
    blank: WithSource<BlankMessage<P>>,

    details: ExecutableMessageDetails,
    history: Option<ExecutionHistory>,
}

impl<P> BaseQueueMessage<P> {
    /// Creates a new queue message with the given parameters.
    ///
    /// For testing purposes only.
    #[cfg(feature = "test-utils")]
    pub const fn new(
        blank: BlankMessage<P>,
        source: ActorId,
        details: ExecutableMessageDetails,
        history: Option<ExecutionHistory>,
    ) -> Self {
        let blank = WithSource::new(blank, source);

        Self {
            blank,
            details,
            history,
        }
    }

    /// Creates a new queue message with the given parameters.
    ///
    /// For internal use only.
    pub(crate) const fn _new(
        blank: WithSource<BlankMessage<P>>,
        details: ExecutableMessageDetails,
        history: Option<ExecutionHistory>,
    ) -> Self {
        Self {
            blank,
            details,
            history,
        }
    }

    /// Creates a new signal message with the given parameters.
    pub fn signal(to: MessageId, code: SignalCode) -> WithId<Self>
    where
        P: Default,
    {
        let blank = WithSource::new(BlankMessage::default(), ActorId::SYSTEM);
        let details = ExecutableMessageDetails::Signal { to, code };
        let history = None;

        let message_id = MessageId::generate_signal(to);

        WithId::new(Self::_new(blank, details, history), message_id)
    }

    /// Returns the kind-specific details of the message.
    pub fn details(&self) -> ExecutableMessageDetails {
        self.details
    }

    /// Returns the execution history of the message, if any.
    pub fn history(&self) -> Option<ExecutionHistory> {
        self.history
    }

    /// Returns a boolean indicating if the message has never been waited on,
    /// meaning it was executed at most once.
    pub fn has_never_waited(&self) -> bool {
        self.history.is_none_or(|h| h.waits() == 0)
    }

    /// Converts the payload of `self` using the provided function.
    pub fn convert<U>(self, f: impl FnOnce(P) -> U) -> BaseQueueMessage<U> {
        let (blank, source) = self.blank.into_parts();
        let blank = WithSource::new(blank.convert(f), source);

        BaseQueueMessage {
            blank,
            details: self.details,
            history: self.history,
        }
    }
}

impl BaseQueueMessage<Payload> {
    /// Converts a queue message into an executable message.
    pub fn into_executable(self, id: MessageId) -> ExecutableMessage {
        ExecutableMessage::_new(id, self)
    }

    /// Converts a queue message into a waitlist message.
    ///
    /// For internal use only.
    ///
    /// Increments the number of times the message was placed on the waitlist
    /// in its execution history.
    /// Uses saturating logic to prevent overflow.
    pub(crate) fn into_waitlist(self) -> BaseWaitlistMessage<Payload> {
        let mut history = self.history.unwrap_or_default();
        history.waits = history.waits.saturating_add(1);

        BaseWaitlistMessage::_new(self.blank, self.details, history)
    }
}

impl<P> WrapWithDestination for BaseQueueMessage<P> {}
impl<P> WrapWithId for BaseQueueMessage<P> {}

/// Base message type used for storing messages in a stash.
///
/// Intended usage due to platform specifics and data storing characteristics:
/// - GearExe: `WithId<BaseStashMessage<PayloadOrHash>>`;
/// - Vara: `WithDestination<BaseStashMessage<Payload>>`.
#[derive(
    Debug,
    Clone,
    Copy,
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
pub struct BaseStashMessage<P> {
    #[deref]
    #[deref_mut]
    blank: WithSource<BlankMessage<P>>,

    is_init: bool,
}

impl<P> BaseStashMessage<P> {
    /// Creates a new stash message with the given parameters.
    ///
    /// For testing purposes only.
    #[cfg(feature = "test-utils")]
    pub const fn new(blank: BlankMessage<P>, source: ActorId, is_init: bool) -> Self {
        let blank = WithSource::new(blank, source);

        Self { blank, is_init }
    }

    /// Creates a new stash message with the given parameters.
    ///
    /// For internal use only.
    pub(crate) const fn _new(blank: WithSource<BlankMessage<P>>, is_init: bool) -> Self {
        Self { blank, is_init }
    }

    /// Returns the message kind.
    pub fn kind(&self) -> MessageKind {
        if self.is_init {
            MessageKind::Init
        } else {
            MessageKind::Handle
        }
    }

    /// Converts the payload of `self` using the provided function.
    pub fn convert<U>(self, f: impl FnOnce(P) -> U) -> BaseStashMessage<U> {
        let (blank, source) = self.blank.into_parts();
        let blank = WithSource::new(blank.convert(f), source);

        BaseStashMessage {
            blank,
            is_init: self.is_init,
        }
    }

    /// Converts a stash message into a mailbox message.
    ///
    /// Returns `None` if the message is of `Init` kind.
    pub fn try_into_mailbox(self) -> Option<BaseMailboxMessage<P>> {
        (!self.is_init).then_some(BaseMailboxMessage::_new(self.blank.into_inner()))
    }

    /// Converts a stash message into a queue message.
    pub fn into_queue(self) -> BaseQueueMessage<P> {
        let details = if self.is_init {
            ExecutableMessageDetails::Init
        } else {
            ExecutableMessageDetails::Handle
        };
        let history = None;

        BaseQueueMessage::_new(self.blank, details, history)
    }
}

impl<P> WrapWithDestination for BaseStashMessage<P> {}
impl<P> WrapWithId for BaseStashMessage<P> {}

/// Base message type used for storing messages in a waitlist.
///
/// Intended usage due to platform specifics and data storing characteristics:
/// - GearExe: `WithId<BaseWaitlistMessage<PayloadOrHash>>`;
/// - Vara: `WithDestination<BaseWaitlistMessage<Payload>>`.
#[derive(
    Debug,
    Clone,
    Copy,
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
pub struct BaseWaitlistMessage<P> {
    #[deref]
    #[deref_mut]
    blank: WithSource<BlankMessage<P>>,

    details: ExecutableMessageDetails,
    history: ExecutionHistory,
}

impl<P> BaseWaitlistMessage<P> {
    /// Creates a new waitlist message with the given parameters.
    ///
    /// For testing purposes only.
    #[cfg(feature = "test-utils")]
    pub const fn new(
        blank: BlankMessage<P>,
        source: ActorId,
        details: ExecutableMessageDetails,
        history: ExecutionHistory,
    ) -> Self {
        let blank = WithSource::new(blank, source);

        Self {
            blank,
            details,
            history,
        }
    }

    /// Creates a new waitlist message with the given parameters.
    ///
    /// For internal use only.
    pub(crate) const fn _new(
        blank: WithSource<BlankMessage<P>>,
        details: ExecutableMessageDetails,
        history: ExecutionHistory,
    ) -> Self {
        Self {
            blank,
            details,
            history,
        }
    }

    /// Returns the kind-specific details of the message.
    pub fn details(&self) -> ExecutableMessageDetails {
        self.details
    }

    /// Returns the execution history of the message.
    pub fn history(&self) -> ExecutionHistory {
        self.history
    }

    /// Converts the payload of `self` using the provided function.
    pub fn convert<U>(self, f: impl FnOnce(P) -> U) -> BaseWaitlistMessage<U> {
        let (blank, source) = self.blank.into_parts();
        let blank = WithSource::new(blank.convert(f), source);

        BaseWaitlistMessage {
            blank,
            details: self.details,
            history: self.history,
        }
    }

    /// Converts a waitlist message into a queue message.
    pub fn into_queue(self) -> BaseQueueMessage<P> {
        BaseQueueMessage::_new(self.blank, self.details, Some(self.history))
    }
}

impl<P> WrapWithDestination for BaseWaitlistMessage<P> {}
impl<P> WrapWithId for BaseWaitlistMessage<P> {}

impl<P> WithId<BaseWaitlistMessage<P>> {
    /// Creates an error reply message for this waitlist message,
    /// when it is removed from the waitlist by the system.
    ///
    /// Returns `None` if the message is not repliable, which is the case
    /// when it is a reply or signal message.
    pub fn try_removed_error_reply(&self) -> Option<OutgoingMessage> {
        self.details().as_kind().is_repliable().then(|| {
            let code = ReplyCode::Error(ErrorReplyReason::RemovedFromWaitlist);
            let payload = Default::default();
            let gas = None;
            // Waitlist message has already been executed at least once,
            // so value is always 0.
            let value = 0;

            OutgoingMessage::reply(self.id(), code, self.source(), payload, gas, value)
        })
    }
}
