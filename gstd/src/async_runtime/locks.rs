//! Wait duration registry
use crate::{exec, prelude::BTreeMap, MessageId};

/// Wait locks.
pub(crate) enum Lock {
    #[allow(dead_code)]
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

/// Map of wait locks.
pub(crate) type Locks = BTreeMap<MessageId, Lock>;
