// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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
    message::{Payload, StoredMessage},
};
use gear_core_errors::{ErrorReplyReason, ReplyCode, SuccessReplyReason};
use std::{convert::TryInto, fmt::Debug};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CoreLog {
    id: MessageId,
    source: ProgramId,
    destination: ProgramId,
    payload: Payload,
    reply_code: Option<ReplyCode>,
}

impl CoreLog {
    pub fn id(&self) -> MessageId {
        self.id
    }

    pub fn source(&self) -> ProgramId {
        self.source
    }

    pub fn destination(&self) -> ProgramId {
        self.destination
    }

    pub fn payload(&self) -> &[u8] {
        self.payload.inner()
    }

    pub fn reply_code(&self) -> Option<ReplyCode> {
        self.reply_code
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
        }
    }
}

#[derive(Debug)]
pub struct DecodedCoreLog<T: Codec + Debug> {
    id: MessageId,
    source: ProgramId,
    destination: ProgramId,
    payload: T,
    reply_code: Option<ReplyCode>,
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
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Log {
    source: Option<ProgramId>,
    destination: Option<ProgramId>,
    payload: Option<Payload>,
    reply_code: Option<ReplyCode>,
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
    pub fn builder() -> Self {
        Default::default()
    }

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

    /// Create a `Log` with a reply code of `SuccessReplyReason::Auto`.
    pub fn auto_reply_builder() -> Self {
        Self {
            reply_code: Some(ReplyCode::Success(SuccessReplyReason::Auto)),
            ..Default::default()
        }
    }

    pub fn payload(self, payload: impl Encode) -> Self {
        self.payload_bytes(payload.encode())
    }

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

    #[track_caller]
    pub fn source(mut self, source: impl Into<ProgramIdWrapper>) -> Self {
        if self.source.is_some() {
            panic!("Source was already set for this log");
        }

        self.source = Some(source.into().0);

        self
    }

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
}

impl PartialEq<StoredMessage> for Log {
    fn eq(&self, other: &StoredMessage) -> bool {
        if matches!(other.reply_details(), Some(reply) if Some(reply.to_reply_code()) != self.reply_code)
        {
            return false;
        }
        if matches!(self.source, Some(source) if source != other.source()) {
            return false;
        }
        if matches!(self.destination, Some(dest) if dest != other.destination()) {
            return false;
        }
        if matches!(&self.payload, Some(payload) if payload.inner() != other.payload_bytes()) {
            return false;
        }
        true
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

        if matches!(self.reply_code, Some(c) if c.is_success()) {
            if let Some(payload) = &self.payload {
                if payload.inner() != other.payload.inner() {
                    return false;
                }
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

#[derive(Debug, Clone)]
pub struct RunResult {
    pub(crate) log: Vec<CoreLog>,
    pub(crate) main_failed: bool,
    pub(crate) others_failed: bool,
    pub(crate) message_id: MessageId,
    pub(crate) total_processed: u32,
    pub(crate) main_gas_burned: Gas,
    pub(crate) others_gas_burned: Gas,
}

impl RunResult {
    pub fn contains<T: Into<Log> + Clone>(&self, log: &T) -> bool {
        let log = log.clone().into();

        self.log.iter().any(|e| e == &log)
    }

    pub fn log(&self) -> &[CoreLog] {
        &self.log
    }

    pub fn main_failed(&self) -> bool {
        self.main_failed
    }

    pub fn others_failed(&self) -> bool {
        self.others_failed
    }

    pub fn sent_message_id(&self) -> MessageId {
        self.message_id
    }

    pub fn total_processed(&self) -> u32 {
        self.total_processed
    }

    pub fn main_gas_burned(&self) -> Gas {
        self.main_gas_burned
    }

    pub fn others_gas_burned(&self) -> Gas {
        self.others_gas_burned
    }

    pub fn decoded_log<T: Codec + Debug>(&self) -> Vec<DecodedCoreLog<T>> {
        self.log
            .clone()
            .into_iter()
            .flat_map(DecodedCoreLog::try_from_log)
            .collect()
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
