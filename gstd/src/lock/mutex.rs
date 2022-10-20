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

//! Mutex async implementation.
//! The data protected by the mutex can be accessed through this guard via its
//! `deref` and `deref_mut` implementations.

use crate::MessageId;
use core::{
    cell::UnsafeCell,
    future::Future,
    ops::{Deref, DerefMut},
    pin::Pin,
    task::{Context, Poll},
};

use super::access::AccessQueue;

pub struct Mutex<T> {
    locked: UnsafeCell<Option<MessageId>>,
    value: UnsafeCell<T>,
    queue: AccessQueue,
}

impl<T> Mutex<T> {
    /// Method `lock` allows message to lock mutex.
    pub fn lock(&self) -> MutexLockFuture<'_, T> {
        MutexLockFuture { mutex: self }
    }

    pub const fn new(t: T) -> Mutex<T> {
        Mutex {
            value: UnsafeCell::new(t),
            locked: UnsafeCell::new(None),
            queue: AccessQueue::new(),
        }
    }
}

/// This structure is created by the lock and try_lock methods on Mutex.
pub struct MutexGuard<'a, T> {
    mutex: &'a Mutex<T>,
}

impl<'a, T> Drop for MutexGuard<'a, T> {
    fn drop(&mut self) {
        unsafe {
            *self.mutex.locked.get() = None;
            if let Some(message_id) = self.mutex.queue.dequeue() {
                crate::exec::wake(message_id).expect("Failed to wake the message");
            }
        }
    }
}

impl<'a, T> AsRef<T> for MutexGuard<'a, T> {
    fn as_ref(&self) -> &'a T {
        unsafe { &*self.mutex.value.get() }
    }
}

impl<'a, T> AsMut<T> for MutexGuard<'a, T> {
    fn as_mut(&mut self) -> &'a mut T {
        unsafe { &mut *self.mutex.value.get() }
    }
}

impl<T> Deref for MutexGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.mutex.value.get() }
    }
}

impl<T> DerefMut for MutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.mutex.value.get() }
    }
}

// we are always single-threaded
unsafe impl<T> Sync for Mutex<T> {}

pub struct MutexLockFuture<'a, T> {
    mutex: &'a Mutex<T>,
}

impl<'a, T> Future for MutexLockFuture<'a, T> {
    type Output = MutexGuard<'a, T>;

    /// In case of locked mutex and an `.await`, function `poll` checks if the
    /// mutex can be taken, else it waits (goes into *waiting queue*).
    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        let lock = unsafe { &mut *self.mutex.locked.get() };
        if lock.is_none() {
            *lock = Some(crate::msg::id());
            Poll::Ready(MutexGuard { mutex: self.mutex })
        } else {
            self.mutex.queue.enqueue(crate::msg::id());
            Poll::Pending
        }
    }
}
