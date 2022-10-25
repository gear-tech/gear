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

/// DoubleMap for wait locks.
#[derive(Default, Debug)]
pub struct LocksMap(BTreeMap<MessageId, BTreeMap<MessageId, Lock>>);

impl LocksMap {
    /// Trigger waiting for the message.
    pub fn wait(&mut self, message_id: MessageId) {
        let locks = self.0.entry(message_id).or_insert_with(Default::default);
        if locks.is_empty() {
            // If there is no `waiting_reply_to` id specfied, use
            // the message id as key for the message lock.
            locks.insert(message_id, Default::default());
        }

        locks.iter().for_each(|(_, l)| l.wait());
    }

    /// Lock message.
    pub fn lock(&mut self, message_id: MessageId, waiting_reply_to: MessageId, lock: Lock) {
        let locks = self.0.entry(message_id).or_insert_with(Default::default);
        locks.insert(waiting_reply_to, lock);
    }

    /// Remove all locks of message.
    pub fn remove_all(&mut self, message_id: MessageId) {
        self.0.remove(&message_id);
    }

    /// Remove lock of message.
    pub fn remove(&mut self, message_id: MessageId, waiting_reply_to: MessageId) {
        let locks = self.0.entry(message_id).or_insert_with(Default::default);
        locks.remove(&waiting_reply_to);
    }

    /// Check if message is timeout.
    pub fn is_timeout(
        &mut self,
        message_id: MessageId,
        waiting_reply_to: MessageId,
    ) -> Option<(u32, u32)> {
        let locks = self.0.entry(message_id).or_insert_with(Default::default);
        locks.get(&waiting_reply_to).and_then(|l| l.timeout())
    }
}
