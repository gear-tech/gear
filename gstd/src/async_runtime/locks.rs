//! Wait duration registry
use crate::{async_runtime, exec, prelude::BTreeMap, MessageId};

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
        unsafe { Lock::NoMore(crate::config::DEFAULT_WAIT_NO_MORE_DURATION) }
    }
}

/// Wait trait for async sending messages.
pub trait Wait {
    /// Message which is waiting for.
    fn waiting_reply_to(&self) -> MessageId;

    /// Delays handling for given specific amount of blocks.
    fn no_more(&self, duration: u32) {
        async_runtime::locks().insert(self.waiting_reply_to(), Lock::NoMore(duration));
    }

    /// Delays handling for maximal amount of blocks that could be payed, that
    /// doesn't exceed given duration.
    fn till(&self, duration: u32) {
        async_runtime::locks().insert(self.waiting_reply_to(), Lock::For(duration));
    }
}

/// Map of wait locks.
pub(crate) type Locks = BTreeMap<MessageId, Lock>;

#[test]
fn wait_lock_config_works() {
    let lock_a = Lock::default();
    assert_eq!(lock_a, unsafe {
        Lock::NoMore(crate::config::DEFAULT_WAIT_NO_MORE_DURATION)
    });

    // update default duration
    unsafe {
        crate::config::DEFAULT_WAIT_NO_MORE_DURATION = 42;
    }

    // test updated deafult lock
    let lock_b = Lock::default();
    assert_eq!(lock_b, unsafe {
        Lock::NoMore(crate::config::DEFAULT_WAIT_NO_MORE_DURATION)
    });

    assert_ne!(lock_a, lock_b);
}
