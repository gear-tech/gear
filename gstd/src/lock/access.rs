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

//! This module gives mechanism of waking for async lockers.

use crate::{prelude::VecDeque, MessageId};
use core::cell::UnsafeCell;

// Option<VecDeque> to make new `const fn`
pub struct AccessQueue(UnsafeCell<Option<VecDeque<MessageId>>>);

impl AccessQueue {
    pub fn enqueue(&self, message_id: MessageId) {
        let inner = unsafe { &mut *self.0.get() };

        let vec_deque = inner.get_or_insert_with(VecDeque::new);
        vec_deque.push_back(message_id);
    }

    pub fn dequeue(&self) -> Option<MessageId> {
        let inner = unsafe { &mut *self.0.get() };

        inner.as_mut().and_then(|v| v.pop_front())
    }

    pub const fn new() -> Self {
        AccessQueue(UnsafeCell::new(None))
    }
}
