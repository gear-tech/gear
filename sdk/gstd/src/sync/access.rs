// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! This module gives mechanism of waking for async lockers.

use crate::{MessageId, collections::VecDeque};
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

    pub fn contains(&self, message_id: &MessageId) -> bool {
        let inner = unsafe { &*self.0.get() };

        inner.as_ref().is_some_and(|v| v.contains(message_id))
    }

    pub fn len(&self) -> usize {
        let inner = unsafe { &*self.0.get() };

        inner.as_ref().map_or(0, |v| v.len())
    }

    pub fn first(&self) -> Option<&MessageId> {
        let inner = unsafe { &*self.0.get() };

        inner.as_ref().and_then(|v| v.front())
    }

    pub const fn new() -> Self {
        AccessQueue(UnsafeCell::new(None))
    }
}
