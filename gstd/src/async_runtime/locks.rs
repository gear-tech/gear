//! Wait duration registry
use crate::{async_runtime, exec, prelude::BTreeMap, Config, MessageId};

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

/// Wait trait for async sending messages.
pub trait Wait: Sized {
    /// Delays handling for given specific amount of blocks.
    fn no_more(self, duration: u32) -> Self {
        async_runtime::locks().insert(crate::msg::id(), Lock::NoMore(duration));
        self
    }

    /// Delays handling for maximal amount of blocks that could be payed, that
    /// doesn't exceed given duration.
    fn till(self, duration: u32) -> Self {
        async_runtime::locks().insert(crate::msg::id(), Lock::For(duration));
        self
    }
}

/// Map of wait locks.
pub(crate) type LocksMap = BTreeMap<MessageId, Lock>;
