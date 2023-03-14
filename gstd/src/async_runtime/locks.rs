// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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
    config::WaitType,
    errors::{ContractError, Result},
    exec, BTreeMap, Config, MessageId,
};
use core::cmp::Ordering;
use hashbrown::HashMap;

/// Type of wait locks.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum LockType {
    WaitFor(u32),
    WaitUpTo(u32),
}

/// Wait lock
#[derive(Debug, PartialEq, Eq)]
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
            return Err(ContractError::EmptyWaitDuration);
        }

        Ok(Self {
            at: exec::block_height(),
            ty: LockType::WaitFor(b),
        })
    }

    /// Wait up to
    pub fn up_to(b: u32) -> Result<Self> {
        if b == 0 {
            return Err(ContractError::EmptyWaitDuration);
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
                    "Checked in `crate::msg::async::poll`, will trigger the tiemout error automatically."
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

/// DoubleMap for wait locks.
#[derive(Default, Debug)]
pub struct LocksMap(HashMap<MessageId, BTreeMap<MessageId, Lock>>);

impl LocksMap {
    /// Trigger waiting for the message.
    pub fn wait(&mut self, message_id: MessageId) {
        let map = self.0.entry(message_id).or_insert_with(Default::default);
        if map.is_empty() {
            // If there is no `waiting_reply_to` id specified, use
            // the message id as the key of the message lock.
            //
            // (this key should be `waiting_reply_to` in general )
            //
            // # TODO: refactor it better (#1737)
            map.insert(message_id, Default::default());
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
        let locks = self.0.entry(message_id).or_insert_with(Default::default);
        locks.insert(waiting_reply_to, lock);
    }

    /// Remove message lock.
    pub fn remove(&mut self, message_id: MessageId, waiting_reply_to: MessageId) {
        let locks = self.0.entry(message_id).or_insert_with(Default::default);
        locks.remove(&waiting_reply_to);
    }

    pub fn remove_message_entry(&mut self, message_id: MessageId) {
        // TODO: check this place #2385
        self.0.remove(&message_id);
    }

    /// Check if message is timed out.
    pub fn is_timeout(
        &mut self,
        message_id: MessageId,
        waiting_reply_to: MessageId,
    ) -> Option<(u32, u32)> {
        self.0
            .get(&message_id)
            .and_then(|locks| locks.get(&waiting_reply_to).and_then(|l| l.timeout()))
    }
}
