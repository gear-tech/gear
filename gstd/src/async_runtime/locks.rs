//! Wait duration registry
use crate::{exec, prelude::BTreeMap, Config, MessageId};

/// Type of wait locks.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum LockType {
    WaitFor(u32),
    WaitUpTo(u32),
}

/// Wait lock
#[derive(Debug)]
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
            ty: LockType::WaitFor(b),
        }
    }

    /// Wait no more
    pub fn up_to(b: u32) -> Self {
        Self {
            at: exec::block_height(),
            ty: LockType::WaitUpTo(b),
        }
    }

    /// Call wait functions by the lock type.
    pub fn wait(&self) {
        match self.ty {
            LockType::WaitFor(d) => exec::wait_for(d),
            LockType::WaitUpTo(d) => exec::wait_up_to(d),
        }
    }

    /// Get bound of the current lock
    pub fn bound(&self) -> u32 {
        match &self.ty {
            LockType::WaitFor(d) | LockType::WaitUpTo(d) => self.at + *d,
        }
    }

    /// Check if this lock is timeout
    pub fn timeout(&self) -> Option<(u32, u32)> {
        let current = exec::block_height();
        let expected = self.bound();

        if current >= expected {
            Some((expected, current))
        } else {
            None
        }
    }
}

impl Default for Lock {
    fn default() -> Self {
        Lock::up_to(Config::wait_duration())
    }
}

impl Default for LockType {
    fn default() -> Self {
        LockType::WaitUpTo(Config::wait_duration())
    }
}

/// Map of wait locks.
pub(crate) type LocksMap = BTreeMap<MessageId, Lock>;
