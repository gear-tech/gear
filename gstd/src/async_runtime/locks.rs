//! Wait duration registry
use crate::{exec, prelude::BTreeMap, Config, MessageId};

/// Wait locks.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Lock {
    For(u32),
    NoMore(u32),
}

impl Lock {
    /// Call wait functions by the lock type.
    pub fn wait(&self) {
        match self {
            Lock::For(d) => exec::wait_for(*d),
            Lock::NoMore(d) => exec::wait_no_more(*d),
        }
    }
}

impl Default for Lock {
    fn default() -> Self {
        Lock::NoMore(Config::wait_duration())
    }
}

/// Map of wait locks.
pub(crate) type LocksMap = BTreeMap<MessageId, Lock>;
