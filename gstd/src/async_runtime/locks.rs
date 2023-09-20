// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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
    collections::BTreeMap,
    config::WaitType,
    errors::{Error, Result},
    exec, Config, MessageId,
};
use core::cmp::Ordering;
use hashbrown::{hash_map::Entry, HashMap};

/// Type of wait locks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LockType {
    WaitFor(u32),
    WaitUpTo(u32),
}

/// Wait lock
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lock {
    /// The start block number of this lock.
    pub at: u32,
    /// The type of this lock.
    ty: LockType,
}

impl Lock {
    /// Wait for
    pub fn exactly(b: u32) -> Result<Self> {
        if b == 0 {
            return Err(Error::EmptyWaitDuration);
        }

        Ok(Self {
            at: exec::block_height(),
            ty: LockType::WaitFor(b),
        })
    }

    /// Wait up to
    pub fn up_to(b: u32) -> Result<Self> {
        if b == 0 {
            return Err(Error::EmptyWaitDuration);
        }

        Ok(Self {
            at: exec::block_height(),
            ty: LockType::WaitUpTo(b),
        })
    }

    /// Call wait functions by the lock type.
    pub fn wait(&self) -> ! {
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
    pub fn deadline(&self) -> u32 {
        match &self.ty {
            LockType::WaitFor(d) | LockType::WaitUpTo(d) => self.at.saturating_add(*d),
        }
    }

    /// Check if this lock is timed out.
    pub fn timeout(&self) -> Option<(u32, u32)> {
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
        self.deadline().partial_cmp(&other.deadline())
    }
}

impl Ord for Lock {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap_or(Ordering::Equal)
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum LockContext {
    ReplyTo(MessageId),
    Sleep(u32),
}

/// DoubleMap for wait locks.
#[derive(Default, Debug)]
pub struct LocksMap(HashMap<MessageId, BTreeMap<LockContext, Lock>>);

impl LocksMap {
    /// Trigger waiting for the message.
    pub fn wait(&mut self, message_id: MessageId) {
        crate::log!("wait({message_id:.2?})");

        let map = self
            .0
            .entry(message_id)
            .and_modify(|_| crate::log!("wait({message_id:.2?}): entry is FOUND"))
            .or_insert_with(|| {
                crate::log!("wait({message_id:.2?}): entry is CREATED");

                // If there is no `waiting_reply_to` id specified, use
                // the message id as the key of the message lock.
                //
                // (this key should be `waiting_reply_to` in general)
                //
                // # TODO: refactor it better (#1737)
                [(LockContext::ReplyTo(message_id), Default::default())].into()
            });

        // For `deadline <= now`, we are checking them in `crate::msg::async::poll`.
        //
        // Locks with `deadline < now` should’ve been removed since
        // the node will trigger timeout when the locks reach their deadline.
        //
        // Locks with `deadline <= now`, they will be removed in the following polling.
        let now = exec::block_height();

        crate::log!(
            "wait({message_id:.2?}): current block is {}",
            crate::util::u32_with_sep(now)
        );

        crate::log!("wait({message_id:.2?}): looking for appropriate lock to be waited");

        if let Some(lock) = map
            .iter()
            .filter_map(|(_, lock)| {
                if lock.deadline() > now {
                    crate::log!("wait({message_id:.2?}): lookup... accepting {lock:.2?}");

                    Some(lock)
                } else {
                    crate::log!("wait({message_id:.2?}): lookup... filtering out {lock:.2?}");

                    None
                }
            })
            .min_by(|lock1, lock2| lock1.cmp(lock2))
        {
            crate::log!("wait({message_id:.2?}): waiting closest {lock:.2?}");

            lock.wait();
        } else {
            crate::log!("wait({message_id:.2?}): couldn't find appropriate lock, panicking...");

            panic!("Couldn't find lock to be waited");
        }
    }

    /// Lock message.
    pub fn lock(&mut self, message_id: MessageId, waiting_reply_to: MessageId, lock: Lock) {
        crate::log!("lock({message_id:.2?}, {waiting_reply_to:.2?}, {lock:.2?})");

        let lock_context = LockContext::ReplyTo(waiting_reply_to);

        self.0.entry(message_id)
            .and_modify(|locks| {
                crate::log!("lock({message_id:.2?}, {waiting_reply_to:.2?}, {lock:.2?}): entry is FOUND");

                crate::log!("lock({message_id:.2?}, {waiting_reply_to:.2?}, {lock:.2?}): inserting lock");

                locks.insert(lock_context, lock);
            }).or_insert_with(|| {
            crate::log!("lock({message_id:.2?}, {waiting_reply_to:.2?}, {lock:.2?}): entry is CREATED with lock");

            [(lock_context, lock)].into()
        });
    }

    /// Remove message lock.
    pub fn remove(&mut self, message_id: MessageId, waiting_reply_to: MessageId) {
        crate::log!("remove({message_id:.2?}, {waiting_reply_to:.2?})");

        match self.0.entry(message_id) {
            Entry::Occupied(entry) => {
                crate::log!("remove({message_id:.2?}, {waiting_reply_to:.2?}): entry is FOUND");

                crate::log!("remove({message_id:.2?}, {waiting_reply_to:.2?}): removing lock");

                entry
                    .into_mut()
                    .remove(&LockContext::ReplyTo(waiting_reply_to));
            }
            Entry::Vacant(_) => {
                crate::log!("remove({message_id:.2?}, {waiting_reply_to:.2?}): entry is NOT FOUND")
            }
        }
    }

    /// Inserts a lock for putting a message into sleep.
    pub fn insert_sleep(&mut self, message_id: MessageId, until_block: u32) {
        crate::log!("insert_sleep({message_id:.2?}, {until_block})");

        let now = exec::block_height();

        crate::log!(
            "insert_sleep({message_id:.2?}, {until_block}): current block is {}",
            crate::util::u32_with_sep(now)
        );

        let lock_context = LockContext::Sleep(until_block);

        if now < until_block {
            crate::log!("insert_sleep({message_id:.2?}, {until_block}): now < until_block, need to insert lock");

            let lock = Lock::exactly(until_block - now).expect("Never fails with block count > 0");

            self.0.entry(message_id).and_modify(|locks| {
                crate::log!("insert_sleep({message_id:.2?}, {until_block}): entry is FOUND");

                crate::log!("insert_sleep({message_id:.2?}, {until_block}): inserting {lock:.2?}");

                locks.insert(lock_context, lock);
            }).or_insert_with(|| {
                crate::log!("insert_sleep({message_id:.2?}, {until_block}): entry is CREATED with {lock:.2?}");

                [(lock_context, lock)].into()
            });
        } else {
            crate::log!("insert_sleep({message_id:.2?}, {until_block}): now >= until_block, need to remove lock");

            match self.0.entry(message_id) {
                Entry::Occupied(entry) => {
                    crate::log!("insert_sleep({message_id:.2?}, {until_block}): entry is FOUND");

                    crate::log!("insert_sleep({message_id:.2?}, {until_block}): removing lock");

                    entry.into_mut().remove(&lock_context);
                }
                Entry::Vacant(_) => {
                    crate::log!("insert_sleep({message_id:.2?}, {until_block}): entry is NOT FOUND")
                }
            }
        }
    }

    /// Removes a sleep lock.
    pub fn remove_sleep(&mut self, message_id: MessageId, until_block: u32) {
        crate::log!("remove_sleep({message_id:.2?}, {until_block})");

        match self.0.entry(message_id) {
            Entry::Occupied(entry) => {
                crate::log!("remove_sleep({message_id:.2?}, {until_block}): entry is FOUND");

                crate::log!("remove_sleep({message_id:.2?}, {until_block}): removing lock");

                entry.into_mut().remove(&LockContext::Sleep(until_block));
            }
            Entry::Vacant(_) => {
                crate::log!("remove_sleep({message_id:.2?}, {until_block}): entry is NOT FOUND");
            }
        }
    }

    // We're removing locks for the message to keep contract's state clean.
    //
    // The locks for the message may not exist but this is ok, because not all
    // contracts use locks. We'll still try to remove them.
    pub fn remove_message_entry(&mut self, message_id: MessageId) {
        crate::log!("remove_message_entry({message_id:.2?})");

        crate::log!("remove_message_entry({message_id:.2?}): removing entry");

        self.0.remove(&message_id);
    }

    /// Check if message is timed out.
    pub fn is_timeout(
        &mut self,
        message_id: MessageId,
        waiting_reply_to: MessageId,
    ) -> Option<(u32, u32)> {
        crate::log!("is_timeout({message_id:.2?}, {waiting_reply_to:.2?})");

        let Some(locks) = self.0.get(&message_id) else {
            crate::log!("is_timeout({message_id:.2?}, {waiting_reply_to:.2?}): locks for message are NOT FOUND");

            return None;
        };

        let Some(lock) = locks.get(&LockContext::ReplyTo(waiting_reply_to)) else {
            crate::log!("is_timeout({message_id:.2?}, {waiting_reply_to:.2?}): entry in locks for message is NOT FOUND");

            return None;
        };

        if let Some((expected, now)) = lock.timeout() {
            crate::log!("is_timeout({message_id:.2?}, {waiting_reply_to:.2?}): lock is TIMED OUT since {}, current is {}",
            crate::util::u32_with_sep(expected), crate::util::u32_with_sep(now));

            Some((expected, now))
        } else {
            crate::log!(
                "is_timeout({message_id:.2?}, {waiting_reply_to:.2?}): lock is NOT TIMED OUT"
            );

            None
        }
    }
}
