// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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

use crate::{
    BlockCount, BlockNumber, Config, MessageId,
    collections::BTreeMap,
    config::WaitType,
    errors::{Error, Result, UsageError},
    exec,
    sync::MutexId,
};
use core::cmp::Ordering;
use hashbrown::HashMap;

/// Type of wait locks.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum LockType {
    WaitFor(BlockCount),
    WaitUpTo(BlockCount),
}

/// Wait lock
#[derive(Debug, PartialEq, Eq)]
pub struct Lock {
    /// The start block number of this lock.
    pub at: BlockNumber,
    /// The type of this lock.
    ty: LockType,
}

impl Lock {
    /// Wait for
    pub fn exactly(b: BlockCount) -> Result<Self> {
        if b == 0 {
            return Err(Error::Gstd(UsageError::EmptyWaitDuration));
        }

        Ok(Self {
            at: exec::block_height(),
            ty: LockType::WaitFor(b),
        })
    }

    /// Wait up to
    pub fn up_to(b: BlockCount) -> Result<Self> {
        if b == 0 {
            return Err(Error::Gstd(UsageError::EmptyWaitDuration));
        }

        Ok(Self {
            at: exec::block_height(),
            ty: LockType::WaitUpTo(b),
        })
    }

    /// Call wait functions by the lock type.
    pub fn wait(&self) {
        if let Some(blocks) = self.deadline().checked_sub(exec::block_height()) {
            if blocks == 0 {
                unreachable!(
                    "Checked in `crate::msg::async::poll`, will trigger the timeout error automatically."
                );
            }

            match self.ty {
                LockType::WaitFor(_) => exec::wait_for(blocks),
                LockType::WaitUpTo(_) => exec::wait_up_to(blocks),
            }
        } else {
            unreachable!(
                "Checked in `crate::msg::async::poll`, will trigger the timeout error automatically."
            );
        }
    }

    /// Gets the deadline of the current lock.
    pub fn deadline(&self) -> BlockNumber {
        match &self.ty {
            LockType::WaitFor(d) | LockType::WaitUpTo(d) => self.at.saturating_add(*d),
        }
    }

    /// Check if this lock is timed out.
    pub fn timeout(&self) -> Option<(BlockNumber, BlockNumber)> {
        let current = exec::block_height();
        let expected = self.deadline();

        if current >= expected {
            Some((expected, current))
        } else {
            None
        }
    }
}

impl PartialOrd for Lock {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Lock {
    fn cmp(&self, other: &Self) -> Ordering {
        self.deadline().cmp(&other.deadline())
    }
}

impl Default for Lock {
    fn default() -> Self {
        Lock::up_to(Config::wait_up_to()).expect("Checked zero case in config.")
    }
}

impl Default for LockType {
    fn default() -> Self {
        match Config::wait_type() {
            WaitType::WaitFor => LockType::WaitFor(Config::wait_for()),
            WaitType::WaitUpTo => LockType::WaitUpTo(Config::wait_up_to()),
        }
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
enum LockContext {
    // Used for waiting a reply to message 'MessageId'
    ReplyTo(MessageId),
    // Used for sending a message to sleep until block 'BlockNumber'
    Sleep(BlockNumber),
    // Used for waking up a message for an attempt to seize lock for mutex 'MutexId'
    MxLockMonitor(MutexId),
}

/// DoubleMap for wait locks.
#[derive(Default, Debug)]
pub struct LocksMap(HashMap<MessageId, BTreeMap<LockContext, Lock>>);

impl LocksMap {
    /// Trigger waiting for the message.
    pub fn wait(&mut self, message_id: MessageId) {
        let map = self.message_locks(message_id);
        if map.is_empty() {
            // If there is no `waiting_reply_to` id specified, use
            // the message id as the key of the message lock.
            //
            // (this key should be `waiting_reply_to` in general )
            //
            // # TODO: refactor it better (#1737)
            map.insert(LockContext::ReplyTo(message_id), Default::default());
        }

        // For `deadline <= now`, we are checking them in `crate::msg::async::poll`.
        //
        // Locks with `deadline < now` should’ve been removed since
        // the node will trigger timeout when the locks reach their deadline.
        //
        // Locks with `deadline <= now`, they will be removed in the following polling.
        let now = exec::block_height();
        map.iter()
            .filter_map(|(_, lock)| (lock.deadline() > now).then_some(lock))
            .min_by(|lock1, lock2| lock1.cmp(lock2))
            .expect("Cannot find lock to be waited")
            .wait();
    }

    /// Lock message.
    pub fn lock(&mut self, message_id: MessageId, waiting_reply_to: MessageId, lock: Lock) {
        self.message_locks(message_id)
            .insert(LockContext::ReplyTo(waiting_reply_to), lock);
    }

    /// Remove message lock.
    pub fn remove(&mut self, message_id: MessageId, waiting_reply_to: MessageId) {
        self.message_locks(message_id)
            .remove(&LockContext::ReplyTo(waiting_reply_to));
    }

    /// Inserts a lock for putting a message into sleep.
    pub fn insert_sleep(&mut self, message_id: MessageId, wake_up_at: BlockNumber) {
        let locks = self.message_locks(message_id);
        let current_block = exec::block_height();
        if current_block < wake_up_at {
            locks.insert(
                LockContext::Sleep(wake_up_at),
                Lock::exactly(wake_up_at - current_block)
                    .expect("Never fails with block count > 0"),
            );
        } else {
            locks.remove(&LockContext::Sleep(wake_up_at));
        }
    }

    /// Removes a sleep lock.
    pub fn remove_sleep(&mut self, message_id: MessageId, wake_up_at: BlockNumber) {
        self.message_locks(message_id)
            .remove(&LockContext::Sleep(wake_up_at));
    }

    pub(crate) fn insert_mx_lock_monitor(
        &mut self,
        message_id: MessageId,
        mutex_id: MutexId,
        wake_up_at: BlockNumber,
    ) {
        let locks = self.message_locks(message_id);
        locks.insert(
            LockContext::MxLockMonitor(mutex_id),
            Lock::exactly(
                wake_up_at
                    .checked_sub(exec::block_height())
                    .expect("Value of after_block must be greater than current block"),
            )
            .expect("Never fails with block count > 0"),
        );
    }

    pub(crate) fn remove_mx_lock_monitor(&mut self, message_id: MessageId, mutex_id: MutexId) {
        self.message_locks(message_id)
            .remove(&LockContext::MxLockMonitor(mutex_id));
    }

    pub fn remove_message_entry(&mut self, message_id: MessageId) {
        // We're removing locks for the message to keep program's state clean.
        //
        // The locks for the message may not exist but this is ok, because not all
        // programs use locks. We'll still try to remove them.

        self.0.remove(&message_id);
    }

    /// Check if message is timed out.
    pub fn is_timeout(
        &mut self,
        message_id: MessageId,
        waiting_reply_to: MessageId,
    ) -> Option<(BlockNumber, BlockNumber)> {
        self.0.get(&message_id).and_then(|locks| {
            locks
                .get(&LockContext::ReplyTo(waiting_reply_to))
                .and_then(|l| l.timeout())
        })
    }

    fn message_locks(&mut self, message_id: MessageId) -> &mut BTreeMap<LockContext, Lock> {
        self.0.entry(message_id).or_default()
    }
}
