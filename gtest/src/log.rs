// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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
    message::{Payload, StatusCode, StoredMessage},
};
use std::{convert::TryInto, fmt::Debug};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CoreLog {
    id: MessageId,
    source: ProgramId,
    destination: ProgramId,
    payload: Payload,
    status_code: Option<StatusCode>,
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
        self.payload.get()
    }

    pub fn status_code(&self) -> Option<StatusCode> {
        self.status_code
    }
}

impl From<StoredMessage> for CoreLog {
    fn from(other: StoredMessage) -> Self {
        Self {
            id: other.id(),
            source: other.source(),
            destination: other.destination(),
            payload: other.payload().to_vec().try_into().unwrap(),
            status_code: other.status_code(),
        }
    }
}

#[derive(Debug)]
pub struct DecodedCoreLog<T: Codec + Debug> {
    id: MessageId,
    source: ProgramId,
    destination: ProgramId,
    payload: T,
    status_code: Option<i32>,
}

impl<T: Codec + Debug> DecodedCoreLog<T> {
    pub(crate) fn try_from_log(log: CoreLog) -> Option<Self> {
        let payload = T::decode(&mut log.payload.get()).ok()?;

        Some(Self {
            id: log.id,
            source: log.source,
            destination: log.destination,
            payload,
            status_code: log.status_code,
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Log {
    source: Option<ProgramId>,
    destination: Option<ProgramId>,
    payload: Option<Payload>,
    status_code: i32,
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

    pub fn error_builder(status_code: StatusCode) -> Self {
        let mut log = Self::builder();
        log.status_code = status_code;
        log.payload = Some(Default::default());

        log
    }

    pub fn payload(self, payload: impl Encode) -> Self {
        self.payload_bytes(payload.encode())
    }

    pub fn payload_bytes(mut self, payload: impl AsRef<[u8]>) -> Self {
        if self.payload.is_some() {
            panic!("Payload was already set for this log");
        }

        self.payload = Some(payload.as_ref().to_vec().try_into().unwrap());

        self
    }

    pub fn source(mut self, source: impl Into<ProgramIdWrapper>) -> Self {
        if self.source.is_some() {
            panic!("Source was already set for this log");
        }

        self.source = Some(source.into().0);

        self
    }

    pub fn dest(mut self, dest: impl Into<ProgramIdWrapper>) -> Self {
        if self.destination.is_some() {
            panic!("Destination was already set for this log");
        }
        self.destination = Some(dest.into().0);

        self
    }
}

impl PartialEq<StoredMessage> for Log {
    fn eq(&self, other: &StoredMessage) -> bool {
        if matches!(other.reply(), Some(reply) if reply.status_code() != self.status_code) {
            return false;
        }
        if matches!(self.source, Some(source) if source != other.source()) {
            return false;
        }
        if matches!(self.destination, Some(dest) if dest != other.destination()) {
            return false;
        }
        if matches!(&self.payload, Some(payload) if payload.get() != other.payload()) {
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
            status_code: other.status_code,
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
        if let Some(status_code) = other.status_code {
            if status_code != self.status_code {
                return false;
            }
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

        if self.status_code == 0 {
            if let Some(payload) = &self.payload {
                if payload.get() != other.payload.get() {
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
