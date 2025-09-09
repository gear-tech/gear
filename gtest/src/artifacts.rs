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

use crate::{GAS_MULTIPLIER, Gas, ProgramIdWrapper, Value, error::usage_panic};
use core_processor::configs::BlockInfo;
use gear_core::{
    buffer::Payload,
    ids::{ActorId, MessageId},
    message::{StoredMessage, UserStoredMessage},
};
use gear_core_errors::{ErrorReplyReason, ReplyCode, SimpleExecutionError, SuccessReplyReason};
use parity_scale_codec::{Codec, Encode};
use std::{
    collections::{BTreeMap, BTreeSet},
    convert::TryInto,
    fmt::Debug,
};

/// A user message that was stored as an event during execution.
/// todo [sab] add comprehensive docs
#[derive(Clone, Debug, Eq)]
pub struct UserMessageEvent {
    id: MessageId,
    source: ActorId,
    destination: ActorId,
    payload: Payload,
    reply_code: Option<ReplyCode>,
    reply_to: Option<MessageId>,
}

impl UserMessageEvent {
    /// Get the id of the message that emitted this log.
    pub fn id(&self) -> MessageId {
        self.id
    }

    /// Get the source of the message that emitted this log.
    pub fn source(&self) -> ActorId {
        self.source
    }

    /// Get the destination of the message that emitted this log.
    pub fn destination(&self) -> ActorId {
        self.destination
    }

    /// Get the payload of the message that emitted this log.
    pub fn payload(&self) -> &[u8] {
        self.payload.inner()
    }

    /// Get the reply code of the message that emitted this log.
    pub fn reply_code(&self) -> Option<ReplyCode> {
        self.reply_code
    }

    /// Get the reply destination that the reply code was sent to.
    pub fn reply_to(&self) -> Option<MessageId> {
        self.reply_to
    }
    /// todo [sab]
    pub fn decode_payload<T: Codec>(&self) -> Option<T> {
        T::decode(&mut self.payload.inner()).ok()
    }
}

impl From<StoredMessage> for UserMessageEvent {
    fn from(other: StoredMessage) -> Self {
        Self {
            id: other.id(),
            source: other.source(),
            destination: other.destination(),
            payload: other.payload_bytes().to_vec().try_into().unwrap(),
            reply_code: other
                .details()
                .and_then(|d| d.to_reply_details().map(|d| d.to_reply_code())),
            reply_to: other
                .details()
                .and_then(|d| d.to_reply_details().map(|d| d.to_message_id())),
        }
    }
}

impl PartialEq<UserMessageEvent> for UserMessageEvent {
    fn eq(&self, other: &UserMessageEvent) -> bool {
        // Compare all fields unless both fields are non-default and different.

        if self.id() != other.id()
            && self.id() != MessageId::default()
            && other.id() != MessageId::default()
        {
            return false;
        }

        if self.source() != other.source()
            && self.source() != ActorId::default()
            && other.source() != ActorId::default()
        {
            return false;
        }

        if self.destination() != other.destination()
            && self.destination() != ActorId::default()
            && other.destination() != ActorId::default()
        {
            return false;
        }

        if self.payload() != other.payload()
            && !self.payload().is_empty()
            && !other.payload().is_empty()
        {
            return false;
        }

        if self.reply_code() != other.reply_code()
            && (matches!((self.reply_code(), other.reply_code()), (None, None)))
        {
            return false;
        }

        if self.reply_to() != other.reply_to()
            && (matches!((self.reply_to(), other.reply_to()), (None, None)))
        {
            return false;
        }

        true
    }
}

impl PartialEq<UserStoredMessage> for UserMessageEvent {
    fn eq(&self, other: &UserStoredMessage) -> bool {
        if self.id() != other.id() && self.id() != MessageId::default() {
            return false;
        }

        if self.source() != other.source() && self.source() != ActorId::default() {
            return false;
        }

        if self.destination() != other.destination() && self.destination() != ActorId::default() {
            return false;
        }

        if self.payload() != other.payload_bytes() && !self.payload().is_empty() {
            return false;
        }

        true
    }
}

impl From<EventBuilder> for UserMessageEvent {
    fn from(builder: EventBuilder) -> Self {
        builder.build()
    }
}

/// todo [sab] add docs for the builder
#[derive(Clone, Debug, Default)]
pub struct EventBuilder {
    pub(crate) message_id: Option<MessageId>,
    pub(crate) source: Option<ActorId>,
    pub(crate) destination: Option<ActorId>,
    pub(crate) payload: Option<Payload>,
    pub(crate) reply_code: Option<ReplyCode>,
    pub(crate) reply_to: Option<MessageId>,
}

impl EventBuilder {
    /// todo [sab]
    pub fn new() -> Self {
        Default::default()
    }
    /// todo [sab]
    pub fn with_message_id(mut self, message_id: MessageId) -> Self {
        self.message_id = Some(message_id);
        self
    }
    /// todo [sab]
    pub fn with_source(mut self, source: impl Into<ProgramIdWrapper>) -> Self {
        self.source = Some(source.into().0);
        self
    }
    /// todo [sab]
    pub fn with_destination(mut self, destination: impl Into<ProgramIdWrapper>) -> Self {
        self.destination = Some(destination.into().0);
        self
    }
    /// todo [sab]
    pub fn with_payload(self, payload: impl Encode) -> Self {
        self.with_payload_bytes(payload.encode())
    }
    /// todo [sab]
    pub fn with_payload_bytes(mut self, payload: impl AsRef<[u8]>) -> Self {
        if let Some(ReplyCode::Success(SuccessReplyReason::Auto)) = self.reply_code {
            usage_panic!("Cannot set payload for auto reply");
        }

        let payload = payload.as_ref().try_into().unwrap_or_else(|e| {
            usage_panic!("Failed to convert payload bytes: {e}");
        });

        self.payload = Some(payload);
        self
    }
    /// todo [sab]
    pub fn with_reply_code(mut self, reply_code: ReplyCode) -> Self {
        if self.payload.is_some() && reply_code == ReplyCode::Success(SuccessReplyReason::Auto) {
            usage_panic!("Cannot set auto reply for event with payload");
        }

        self.reply_code = Some(reply_code);
        self
    }
    /// todo [sab]
    pub fn with_reply_to(mut self, reply_to: MessageId) -> Self {
        self.reply_to = Some(reply_to);
        self
    }
    /// todo [sab]
    pub fn build(self) -> UserMessageEvent {
        UserMessageEvent {
            id: self.message_id.unwrap_or_default(),
            source: self.source.unwrap_or_default(),
            destination: self.destination.unwrap_or_default(),
            payload: self.payload.unwrap_or_default(),
            reply_code: self.reply_code,
            reply_to: self.reply_to,
        }
    }
}

/// A log that can be emitted by a program.
///
/// ```ignore
/// use gtest::{Log, Program, System};
///
/// let system = System::new();
/// let program = Program::current(&system);
/// let from = 42;
/// let res = program.send(from, ());
///
/// // Check that the log is emitted.
/// let log = Log::builder().source(program.id()).dest(from);
/// assert!(res.contains(&log));
/// ```
///
/// The Log instance is also possible being parsed from tuples.
///
/// ```
/// use gtest::Log;
///
/// let log: Log = (1, "payload").into();
/// assert_eq!(Log::builder().dest(1).payload_bytes("payload"), log);
///
/// assert_eq!(
///     Log::builder().source(1).dest(2).payload_bytes("payload"),
///     Log::from((1, 2, "payload")),
/// );
///
/// let v = vec![1; 32];
/// assert_eq!(
///     Log::builder().source(1).dest(&v).payload_bytes("payload"),
///     Log::from((1, v, "payload"))
/// );
/// ```
///

/// Result of running the block.
#[derive(Debug, Default)]
pub struct BlockRunResult {
    /// Executed block info.
    pub block_info: BlockInfo,
    /// Gas allowance spent during the execution.
    pub gas_allowance_spent: Gas,
    /// Set of successfully executed messages
    /// during the current block execution.
    pub succeed: BTreeSet<MessageId>,
    /// Set of failed messages during the current
    /// block execution.
    pub failed: BTreeSet<MessageId>,
    /// Set of not executed messages
    /// during the current block execution.
    pub not_executed: BTreeSet<MessageId>,
    /// Total messages processed during the current
    /// execution.
    pub total_processed: u32,
    /// User message events created during the current execution.
    pub events: Vec<UserMessageEvent>,
    /// Mapping gas burned for each message during
    /// the current block execution.
    pub gas_burned: BTreeMap<MessageId, Gas>,
}

impl BlockRunResult {
    /// Check, if the result contains a specific log.
    pub fn contains<T: Into<UserMessageEvent> + Clone>(&self, event: &T) -> bool {
        let events = event.clone().into();

        self.events.iter().any(|e| e == &events)
    }

    /// Get the events.
    pub fn events(&self) -> &[UserMessageEvent] {
        &self.events
    }

    /// Asserts that the message panicked and that the panic contained a
    /// given message.
    pub fn assert_panicked_with(&self, message_id: MessageId, msg: impl Into<String>) {
        let panic_event = self.message_panic_event(message_id);
        assert!(panic_event.is_some(), "Program did not panic");
        let msg = msg.into();
        let payload = String::from_utf8(
            panic_event
                .expect("Asserted using `Option::is_some()`")
                .payload()
                .into(),
        )
        .expect("Unable to decode panic message");

        assert!(
            payload.starts_with(&format!("panicked with '{msg}'")),
            "expected panic message that contains `{msg}`, but the actual panic message is `{payload}`"
        );
    }

    /// Calculate the total spent value for the gas consumption.
    pub fn spent_value(&self) -> Value {
        let spent_gas = self
            .gas_burned
            .values()
            .fold(0u64, |acc, &x| acc.saturating_add(x));

        GAS_MULTIPLIER.gas_to_value(spent_gas)
    }

    /// Trying to get the panic event.
    fn message_panic_event(&self, message_id: MessageId) -> Option<&UserMessageEvent> {
        let msg_event = self
            .events
            .iter()
            .find(|event| event.reply_to == Some(message_id))?;
        let is_panic = matches!(
            msg_event.reply_code(),
            Some(ReplyCode::Error(ErrorReplyReason::Execution(
                SimpleExecutionError::UserspacePanic
            )))
        );
        is_panic.then_some(msg_event)
    }
}
