// This file is part of Gear.

// Copyright (C) 2021-2024 Gear Technologies Inc.
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

use crate::program::{Gas, ProgramIdWrapper};
use codec::{Codec, Encode};
use gear_core::{
    ids::{MessageId, ProgramId},
    message::{Payload, StoredMessage, UserStoredMessage},
};
use gear_core_errors::{ErrorReplyReason, ReplyCode, SimpleExecutionError, SuccessReplyReason};
use std::{collections::BTreeMap, convert::TryInto, fmt::Debug};

/// A log that emitted by a program, for user defined logs,
/// see [`Log`].
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CoreLog {
    id: MessageId,
    source: ProgramId,
    destination: ProgramId,
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
    pub fn source(&self) -> ProgramId {
        self.source
    }

    /// Get the destination of the message that emitted this log.
    pub fn destination(&self) -> ProgramId {
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
    source: ProgramId,
    destination: ProgramId,
    payload: T,
    reply_code: Option<ReplyCode>,
    reply_to: Option<MessageId>,
}

impl<T: Codec + Debug> DecodedCoreLog<T> {
    pub(crate) fn try_from_log(log: CoreLog) -> Option<Self> {
        let payload = T::decode(&mut log.payload.inner()).ok()?;

        Some(Self {
            id: log.id,
            source: log.source,
            destination: log.destination,
            payload,
            reply_code: log.reply_code,
            reply_to: log.reply_to,
        })
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
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Log {
    pub(crate) source: Option<ProgramId>,
    pub(crate) destination: Option<ProgramId>,
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
        log.payload = Some(
            error_reason
                .to_string()
                .into_bytes()
                .try_into()
                .expect("Infallible"),
        );

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
    #[track_caller]
    pub fn payload_bytes(mut self, payload: impl AsRef<[u8]>) -> Self {
        if self.payload.is_some() {
            panic!("Payload was already set for this log");
        }

        if let Some(ReplyCode::Success(SuccessReplyReason::Auto)) = self.reply_code {
            panic!("Cannot set payload for auto reply");
        }

        self.payload = Some(payload.as_ref().to_vec().try_into().unwrap());

        self
    }

    /// Set the source of the log.
    #[track_caller]
    pub fn source(mut self, source: impl Into<ProgramIdWrapper>) -> Self {
        if self.source.is_some() {
            panic!("Source was already set for this log");
        }

        self.source = Some(source.into().0);

        self
    }

    /// Set the destination of the log.
    #[track_caller]
    pub fn dest(mut self, dest: impl Into<ProgramIdWrapper>) -> Self {
        if self.destination.is_some() {
            panic!("Destination was already set for this log");
        }
        self.destination = Some(dest.into().0);

        self
    }

    /// Set the reply code for this log.
    #[track_caller]
    pub fn reply_code(mut self, reply_code: ReplyCode) -> Self {
        if self.reply_code.is_some() {
            panic!("Reply code was already set for this log");
        }
        if self.payload.is_some() && reply_code == ReplyCode::Success(SuccessReplyReason::Auto) {
            panic!("Cannot set auto reply for log with payload");
        }

        self.reply_code = Some(reply_code);

        self
    }

    /// Set the reply destination for this log.
    #[track_caller]
    pub fn reply_to(mut self, reply_to: MessageId) -> Self {
        if self.reply_to.is_some() {
            panic!("Reply destination was already set for this log");
        }

        self.reply_to = Some(reply_to);

        self
    }
}

impl PartialEq<UserStoredMessage> for Log {
    fn eq(&self, other: &UserStoredMessage) -> bool {
        // self.source
        //     .as_ref()
        //     .map_or(Some((self.destination.as_ref(), false)), |source| {
        //         (source == other.source()).then_some((self.destination.as_ref(),
        // true))     })
        //     .and_then(|(maybe_dest, has_any)| {
        //         let ret = self.payload.as_ref();
        //         maybe_dest.map_or(Some((ret, has_any)), |dest| (dest ==
        // other.destination()).then_some((ret, true)))     })
        //     .and_then(|(maybe_payload, has_any)| {
        //         let valid_payload = maybe_payload
        //             .and_then(|payload| Some(payload.inner() ==
        // other.payload_bytes()))             .unwrap_or(true);

        //         (valid_payload && has_any).then_some(())
        //     })
        //     .is_some()

        // Any log field is set.
        let has_any = self.source.is_some() || self.destination.is_some() || self.payload.is_some();

        // If any of log field doesn't match, then there's no equality.
        if matches!(self.source, Some(source) if source != other.source()) {
            return false;
        }

        if matches!(self.destination, Some(dest) if dest != other.destination()) {
            return false;
        }

        if matches!(&self.payload, Some(payload) if payload.inner() != other.payload_bytes()) {
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

        if let Some(source) = self.source {
            if source != other.source {
                return false;
            }
        }

        if let Some(destination) = self.destination {
            if destination != other.destination {
                return false;
            }
        }

        if let Some(payload) = &self.payload {
            if payload.inner() != other.payload.inner() {
                return false;
            }
        }

        true
    }
}

impl PartialEq<Log> for CoreLog {
    fn eq(&self, other: &Log) -> bool {
        other.eq(self)
    }
}

/// The result of a message run.
#[derive(Debug, Clone)]
pub struct RunResult {
    pub(crate) log: Vec<CoreLog>,
    pub(crate) main_failed: bool,
    pub(crate) others_failed: bool,
    pub(crate) message_id: MessageId,
    pub(crate) total_processed: u32,
    pub(crate) main_gas_burned: Gas,
    pub(crate) others_gas_burned: BTreeMap<u32, Gas>,
}

impl RunResult {
    /// If the result contains a specific log.
    pub fn contains<T: Into<Log> + Clone>(&self, log: &T) -> bool {
        let log = log.clone().into();

        self.log.iter().any(|e| e == &log)
    }

    /// Get the logs.
    pub fn log(&self) -> &[CoreLog] {
        &self.log
    }

    /// If main message failed.
    pub fn main_failed(&self) -> bool {
        self.main_failed
    }

    /// If any other messages failed.
    pub fn others_failed(&self) -> bool {
        self.others_failed
    }

    /// Get the message id.
    pub fn sent_message_id(&self) -> MessageId {
        self.message_id
    }

    /// Get the total number of processed messages.
    pub fn total_processed(&self) -> u32 {
        self.total_processed
    }

    /// Get the total gas burned by the main message.
    pub fn main_gas_burned(&self) -> Gas {
        self.main_gas_burned
    }

    /// Get the total gas burned by the other messages.
    pub fn others_gas_burned(&self) -> &BTreeMap<u32, Gas> {
        &self.others_gas_burned
    }

    /// Returns decoded logs.
    pub fn decoded_log<T: Codec + Debug>(&self) -> Vec<DecodedCoreLog<T>> {
        self.log
            .clone()
            .into_iter()
            .flat_map(DecodedCoreLog::try_from_log)
            .collect()
    }

    /// If the main message panicked.
    pub fn main_panicked(&self) -> bool {
        self.main_panic_log().is_some()
    }

    /// Asserts that the main message panicked and that the panic contained a
    /// given message.
    #[track_caller]
    pub fn assert_panicked_with(&self, msg: impl Into<String>) {
        let panic_log = self.main_panic_log();
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
            payload.starts_with(&format!("Panic occurred: panicked with '{msg}'")),
            "expected panic message that contains `{msg}`, but the actual panic message is `{payload}`"
        );
    }

    /// Trying to get the panic log.
    fn main_panic_log(&self) -> Option<&CoreLog> {
        let main_log = self
            .log
            .iter()
            .find(|log| log.reply_to == Some(self.message_id))?;
        let is_panic = matches!(
            main_log.reply_code(),
            Some(ReplyCode::Error(ErrorReplyReason::Execution(
                SimpleExecutionError::UserspacePanic
            )))
        );
        is_panic.then_some(main_log)
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
