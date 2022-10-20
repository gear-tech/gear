//! Wait duration registry
use crate::{exec, prelude::BTreeMap, Config, MessageId};

/// Wait locks.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Lock {
    WaitFor(u32),
    WaitUpTo(u32),
}

impl Lock {
    /// Call wait functions by the lock type.
    pub fn wait(&self) {
        match self {
            Lock::WaitFor(d) => exec::wait_for(*d),
            Lock::WaitUpTo(d) => exec::wait_up_to(*d),
        }
    }
}

impl Default for Lock {
    fn default() -> Self {
        Lock::WaitUpTo(Config::wait_duration())
    }
}

/// Map of wait locks.
pub(crate) type LocksMap = BTreeMap<MessageId, Lock>;
