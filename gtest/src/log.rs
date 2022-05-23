use crate::program::ProgramIdWrapper;
use codec::{Codec, Encode};
use gear_core::{
    ids::{MessageId, ProgramId},
    message::{ExitCode, Payload, StoredMessage},
};
use std::fmt::Debug;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CoreLog {
    id: MessageId,
    source: ProgramId,
    destination: ProgramId,
    payload: Payload,
    exit_code: Option<ExitCode>,
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
        self.payload.as_slice()
    }

    pub fn exit_code(&self) -> Option<ExitCode> {
        self.exit_code
    }
}

impl From<StoredMessage> for CoreLog {
    fn from(other: StoredMessage) -> Self {
        Self {
            id: other.id(),
            source: other.source(),
            destination: other.destination(),
            payload: other.payload().to_vec(),
            exit_code: other.exit_code(),
        }
    }
}

#[derive(Debug)]
pub struct DecodedCoreLog<T: Codec + Debug> {
    id: MessageId,
    source: ProgramId,
    destination: ProgramId,
    payload: T,
    exit_code: Option<i32>,
}

impl<T: Codec + Debug> DecodedCoreLog<T> {
    pub(crate) fn try_from_log(log: CoreLog) -> Option<Self> {
        let payload = T::decode(&mut log.payload.as_ref()).ok()?;

        Some(Self {
            id: log.id,
            source: log.source,
            destination: log.destination,
            payload,
            exit_code: log.exit_code,
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Log {
    source: Option<ProgramId>,
    destination: Option<ProgramId>,
    payload: Option<Vec<u8>>,
    exit_code: i32,
}

impl<ID: Into<ProgramIdWrapper>, T: AsRef<[u8]>> From<(ID, T)> for Log {
    fn from(other: (ID, T)) -> Self {
        Self::builder().dest(other.0).payload_bytes(other.1)
    }
}

impl<ID: Into<ProgramIdWrapper>, T: AsRef<[u8]>> From<(ID, ID, T)> for Log {
    fn from(other: (ID, ID, T)) -> Self {
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

    pub fn error_builder(exit_code: ExitCode) -> Self {
        let mut log = Self::builder();
        log.exit_code = exit_code;
        log.payload = Some(Vec::new());

        log
    }

    pub fn payload<E: Encode>(self, payload: E) -> Self {
        self.payload_bytes(payload.encode())
    }

    pub fn payload_bytes<T: AsRef<[u8]>>(mut self, payload: T) -> Self {
        if self.payload.is_some() {
            panic!("Payload was already set for this log");
        }

        self.payload = Some(payload.as_ref().to_vec());

        self
    }

    pub fn source<T: Into<ProgramIdWrapper>>(mut self, source: T) -> Self {
        if self.source.is_some() {
            panic!("Source was already set for this log");
        }

        self.source = Some(source.into().0);

        self
    }

    pub fn dest<T: Into<ProgramIdWrapper>>(mut self, dest: T) -> Self {
        if self.destination.is_some() {
            panic!("Destination was already set for this log");
        }
        self.destination = Some(dest.into().0);

        self
    }
}

impl PartialEq<StoredMessage> for Log {
    fn eq(&self, other: &StoredMessage) -> bool {
        if matches!(other.reply(), Some(reply) if reply.1 != self.exit_code) {
            return false;
        }
        if matches!(self.source, Some(source) if source != other.source()) {
            return false;
        }
        if matches!(self.destination, Some(dest) if dest != other.destination()) {
            return false;
        }
        if matches!(&self.payload, Some(payload) if payload != other.payload()) {
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
            payload: other.payload.encode(),
            exit_code: other.exit_code,
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
        if let Some(exit_code) = other.exit_code {
            if exit_code != self.exit_code {
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

        if let Some(payload) = &self.payload {
            if payload != &other.payload {
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

pub struct RunResult {
    pub(crate) log: Vec<CoreLog>,
    pub(crate) main_failed: bool,
    pub(crate) others_failed: bool,
    pub(crate) message_id: MessageId,
    pub(crate) total_processed: u32,
}

impl RunResult {
    pub fn contains<T: Into<Log> + Clone>(&self, log: &T) -> bool {
        let log = log.clone().into();

        self.log.iter().any(|e| e == &log)
    }

    pub fn log(&self) -> &Vec<CoreLog> {
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

    pub fn decoded_log<T: Codec + Debug>(&self) -> Vec<DecodedCoreLog<T>> {
        self.log
            .clone()
            .into_iter()
            .flat_map(DecodedCoreLog::try_from_log)
            .collect()
    }
}
