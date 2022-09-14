//! Wait duration registry
use crate::{exec, prelude::BTreeMap, Config, MessageId};

/// Type of wait locks.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum LockType {
    For(u32),
    NoMore(u32),
}

/// Wait lock
pub struct Lock {
    /// The start block number of this lock.
    pub at: u32,
    /// The type of this lock.
    ty: LockType,
}

impl Lock {
    /// Wait for
    pub fn exactly(b: u32) -> Self {
        Self {
            at: exec::block_height(),
            ty: LockType::For(b),
        }
    }

    /// Wait no more
    pub fn no_more(b: u32) -> Self {
        Self {
            at: exec::block_height(),
            ty: LockType::NoMore(b),
        }
    }

    /// Call wait functions by the lock type.
    pub fn wait(&self) {
        match self.ty {
            LockType::For(d) => exec::wait_for(d),
            LockType::NoMore(d) => exec::wait_no_more(d),
        }
    }

    /// Check if this lock is timeout
    pub fn timeout(&self) -> Option<(u32, u32)> {
        let current = exec::block_height();
        let expected = match &self.ty {
            LockType::For(d) => *d,
            LockType::NoMore(d) => self.at + *d,
        };

        if current >= expected {
            Some((expected, current))
        } else {
            None
        }
    }
}

impl Default for Lock {
    fn default() -> Self {
        Lock::no_more(Config::wait_duration())
    }
}

/// Map of wait locks.
pub(crate) type LocksMap = BTreeMap<MessageId, Lock>;
