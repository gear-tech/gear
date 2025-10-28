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

use crate::{GAS_MULTIPLIER, Gas, Value, error::usage_panic, program::ProgramIdWrapper};
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

/// A log that emitted by a program, for user defined logs,
/// see [`Log`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreLog {
    id: MessageId,
    source: ActorId,
    destination: ActorId,
    payload: Payload,
    reply_code: Option<ReplyCode>,
    reply_to: Option<MessageId>,
}

impl CoreLog {
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
        &self.payload
    }

    /// Get the reply code of the message that emitted this log.
    pub fn reply_code(&self) -> Option<ReplyCode> {
        self.reply_code
    }

    /// Get the reply destination that the reply code was sent to.
    pub fn reply_to(&self) -> Option<MessageId> {
        self.reply_to
    }
}

impl From<StoredMessage> for CoreLog {
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

/// A log that has been decoded into a Rust type which implements
/// [`codec::Encodable`] \( [`codec::Decode`] \).
///
/// Used for pretty-printing.
#[derive(Debug)]
pub struct DecodedCoreLog<T: Codec + Debug> {
    id: MessageId,
    source: ActorId,
    destination: ActorId,
    payload: T,
    reply_code: Option<ReplyCode>,
    reply_to: Option<MessageId>,
}

impl<T: Codec + Debug> DecodedCoreLog<T> {
    pub(crate) fn try_from_log(log: CoreLog) -> Option<Self> {
        let payload = T::decode(&mut log.payload.as_ref()).ok()?;

        Some(Self {
            id: log.id,
            source: log.source,
            destination: log.destination,
            payload,
            reply_code: log.reply_code,
            reply_to: log.reply_to,
        })
    }

    pub fn id(&self) -> MessageId {
        self.id
    }

    pub fn source(&self) -> ActorId {
        self.source
    }

    pub fn destination(&self) -> ActorId {
        self.destination
    }

    pub fn payload(&self) -> &T {
        &self.payload
    }

    pub fn reply_code(&self) -> Option<ReplyCode> {
        self.reply_code
    }

    pub fn reply_to(&self) -> Option<MessageId> {
        self.reply_to
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
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Log {
    pub(crate) source: Option<ActorId>,
    pub(crate) destination: Option<ActorId>,
    pub(crate) payload: Option<Payload>,
    pub(crate) reply_code: Option<ReplyCode>,
    pub(crate) reply_to: Option<MessageId>,
}

impl<ID, T> From<(ID, T)> for Log
where
    ID: Into<ProgramIdWrapper>,
    T: AsRef<[u8]>,
{
    fn from(other: (ID, T)) -> Self {
        Self::builder().dest(other.0).payload_bytes(other.1)
    }
}

impl<ID1, ID2, T> From<(ID1, ID2, T)> for Log
where
    ID1: Into<ProgramIdWrapper>,
    ID2: Into<ProgramIdWrapper>,
    T: AsRef<[u8]>,
{
    fn from(other: (ID1, ID2, T)) -> Self {
        Self::builder()
            .source(other.0)
            .dest(other.1)
            .payload_bytes(other.2)
    }
}

impl Log {
    /// Set up a builder for a `Log`.
    pub fn builder() -> Self {
        Default::default()
    }

    /// Set up a builder with error reason.
    pub fn error_builder(error_reason: ErrorReplyReason) -> Self {
        let mut log = Self::builder();
        log.reply_code = Some(error_reason.into());

        log
    }

    /// Set up a log builder with success reason.
    pub fn auto_reply_builder() -> Self {
        Self {
            reply_code: Some(ReplyCode::Success(SuccessReplyReason::Auto)),
            ..Default::default()
        }
    }

    /// Set the payload of the log.
    pub fn payload(self, payload: impl Encode) -> Self {
        self.payload_bytes(payload.encode())
    }

    /// Set the payload of the log with bytes.
    pub fn payload_bytes(mut self, payload: impl AsRef<[u8]>) -> Self {
        if self.payload.is_some() {
            usage_panic!("Payload was already set for this log");
        }

        if let Some(ReplyCode::Success(SuccessReplyReason::Auto)) = self.reply_code {
            usage_panic!("Cannot set payload for auto reply");
        }

        self.payload = Some(payload.as_ref().to_vec().try_into().unwrap());

        self
    }

    /// Set the source of the log.
    pub fn source(mut self, source: impl Into<ProgramIdWrapper>) -> Self {
        if self.source.is_some() {
            usage_panic!("Source was already set for this log");
        }

        self.source = Some(source.into().0);

        self
    }

    /// Set the destination of the log.
    pub fn dest(mut self, dest: impl Into<ProgramIdWrapper>) -> Self {
        if self.destination.is_some() {
            usage_panic!("Destination was already set for this log");
        }
        self.destination = Some(dest.into().0);

        self
    }

    /// Set the reply code for this log.
    pub fn reply_code(mut self, reply_code: ReplyCode) -> Self {
        if self.reply_code.is_some() {
            usage_panic!("Reply code was already set for this log");
        }
        if self.payload.is_some() && reply_code == ReplyCode::Success(SuccessReplyReason::Auto) {
            usage_panic!("Cannot set auto reply for log with payload");
        }

        self.reply_code = Some(reply_code);

        self
    }

    /// Set the reply destination for this log.
    pub fn reply_to(mut self, reply_to: MessageId) -> Self {
        if self.reply_to.is_some() {
            usage_panic!("Reply destination was already set for this log");
        }

        self.reply_to = Some(reply_to);

        self
    }
}

impl PartialEq<UserStoredMessage> for Log {
    fn eq(&self, other: &UserStoredMessage) -> bool {
        // Any log field is set.
        let has_any = self.source.is_some()
            || self.destination.is_some()
            || self.payload.is_some()
            || self.reply_to.is_some();

        // If any of log field doesn't match, then there's no equality.
        if matches!(self.source, Some(source) if source != other.source()) {
            return false;
        }

        if matches!(self.destination, Some(dest) if dest != other.destination()) {
            return false;
        }

        if matches!(&self.payload, Some(payload) if payload.as_slice() != other.payload_bytes()) {
            return false;
        }

        if matches!(self.reply_to, Some(reply_to) if reply_to != other.id()) {
            return false;
        }

        has_any
    }
}

impl<T: Codec + Debug> PartialEq<DecodedCoreLog<T>> for Log {
    fn eq(&self, other: &DecodedCoreLog<T>) -> bool {
        let core_log = CoreLog {
            id: other.id,
            source: other.source,
            destination: other.destination,
            payload: other.payload.encode().try_into().unwrap(),
            reply_code: other.reply_code,
            reply_to: other.reply_to,
        };

        core_log.eq(self)
    }
}

impl<T: Codec + Debug> PartialEq<Log> for DecodedCoreLog<T> {
    fn eq(&self, other: &Log) -> bool {
        other.eq(self)
    }
}

impl PartialEq<CoreLog> for Log {
    fn eq(&self, other: &CoreLog) -> bool {
        // Asserting the field if only reply code specified for `Log`.
        if self.reply_code.is_some() && self.reply_code != other.reply_code {
            return false;
        }

        if self.reply_to.is_some() && self.reply_to != other.reply_to {
            return false;
        }

        if let Some(source) = self.source
            && source != other.source
        {
            return false;
        }

        if let Some(destination) = self.destination
            && destination != other.destination
        {
            return false;
        }

        if let Some(payload) = &self.payload
            && payload.as_slice() != other.payload.as_slice()
        {
            return false;
        }

        true
    }
}

impl PartialEq<Log> for CoreLog {
    fn eq(&self, other: &Log) -> bool {
        other.eq(self)
    }
}

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
    /// User message logs (events) created during the current execution.
    pub log: Vec<CoreLog>,
    /// Mapping gas burned for each message during
    /// the current block execution.
    pub gas_burned: BTreeMap<MessageId, Gas>,
}

impl BlockRunResult {
    /// Check, if the result contains a specific log.
    pub fn contains<T: Into<Log> + Clone>(&self, log: &T) -> bool {
        let log = log.clone().into();

        self.log.iter().any(|e| e == &log)
    }

    /// Get the logs.
    pub fn log(&self) -> &[CoreLog] {
        &self.log
    }

    /// Returns decoded logs.
    pub fn decoded_log<T: Codec + Debug>(&self) -> Vec<DecodedCoreLog<T>> {
        self.log
            .clone()
            .into_iter()
            .flat_map(DecodedCoreLog::try_from_log)
            .collect()
    }

    /// Asserts that the message panicked and that the panic contained a
    /// given message.
    pub fn assert_panicked_with(&self, message_id: MessageId, msg: impl Into<String>) {
        let panic_log = self.message_panic_log(message_id);
        assert!(panic_log.is_some(), "Program did not panic");
        let msg = msg.into();
        let payload = String::from_utf8(
            panic_log
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

    /// Trying to get the panic log.
    fn message_panic_log(&self, message_id: MessageId) -> Option<&CoreLog> {
        let msg_log = self
            .log
            .iter()
            .find(|log| log.reply_to == Some(message_id))?;
        let is_panic = matches!(
            msg_log.reply_code(),
            Some(ReplyCode::Error(ErrorReplyReason::Execution(
                SimpleExecutionError::UserspacePanic
            )))
        );
        is_panic.then_some(msg_log)
    }
}

#[test]
fn soft_into() {
    let log: Log = (1, "payload").into();
    assert_eq!(Log::builder().dest(1).payload_bytes("payload"), log);

    assert_eq!(
        Log::builder().source(1).dest(2).payload_bytes("payload"),
        Log::from((1, 2, "payload")),
    );

    let v = vec![1; 32];
    assert_eq!(
        Log::builder().source(1).dest(&v).payload_bytes("payload"),
        Log::from((1, v, "payload"))
    );
}
