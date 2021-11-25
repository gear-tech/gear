// This file is part of Gear.

// Copyright (C) 2021 Gear Technologies Inc.
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

//! RwLock async implementation.

use super::access::AccessQueue;
use crate::MessageId;
use core::{
    cell::{Cell, UnsafeCell},
    future::Future,
    ops::{Deref, DerefMut},
    pin::Pin,
    task::{Context, Poll},
};

type ReadersCount = u8;
const READERS_LIMIT: ReadersCount = 32;

pub struct RwLock<T> {
    locked: UnsafeCell<Option<MessageId>>,
    value: UnsafeCell<T>,
    readers: Cell<ReadersCount>,
    queueu: AccessQueue,
}

impl<T> RwLock<T> {
    pub fn read(&self) -> RwLockReadFuture<'_, T> {
        RwLockReadFuture { lock: self }
    }

    pub fn write(&self) -> RwLockWriteFuture<'_, T> {
        RwLockWriteFuture { lock: self }
    }

    pub const fn new(t: T) -> RwLock<T> {
        RwLock {
            value: UnsafeCell::new(t),
            locked: UnsafeCell::new(None),
            readers: Cell::new(0),
            queueu: AccessQueue::new(),
        }
    }
}

// we are always single-threaded
unsafe impl<T> Sync for RwLock<T> {}

pub struct RwLockReadGuard<'a, T> {
    lock: &'a RwLock<T>,
}

impl<'a, T> Drop for RwLockReadGuard<'a, T> {
    fn drop(&mut self) {
        unsafe {
            let readers = &self.lock.readers;
            let readers_count = readers.get().saturating_sub(1);

            readers.replace(readers_count);

            if readers_count == 0 {
                *self.lock.locked.get() = None;

                if let Some(message_id) = self.lock.queueu.dequeue() {
                    crate::exec::wake(message_id);
                }
            }
        }
    }
}

impl<'a, T> AsRef<T> for RwLockReadGuard<'a, T> {
    fn as_ref(&self) -> &'a T {
        unsafe { &*self.lock.value.get() }
    }
}

impl<T> Deref for RwLockReadGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.lock.value.get() }
    }
}

impl<T> Deref for RwLockWriteGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.lock.value.get() }
    }
}

impl<T> DerefMut for RwLockWriteGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.lock.value.get() }
    }
}

pub struct RwLockWriteGuard<'a, T> {
    lock: &'a RwLock<T>,
}

impl<'a, T> Drop for RwLockWriteGuard<'a, T> {
    fn drop(&mut self) {
        unsafe {
            *self.lock.locked.get() = None;
            if let Some(message_id) = self.lock.queueu.dequeue() {
                crate::exec::wake(message_id);
            }
        }
    }
}

impl<'a, T> AsRef<T> for RwLockWriteGuard<'a, T> {
    fn as_ref(&self) -> &'a T {
        unsafe { &*self.lock.value.get() }
    }
}

impl<'a, T> AsMut<T> for RwLockWriteGuard<'a, T> {
    fn as_mut(&mut self) -> &'a mut T {
        unsafe { &mut *self.lock.value.get() }
    }
}

pub struct RwLockReadFuture<'a, T> {
    lock: &'a RwLock<T>,
}

pub struct RwLockWriteFuture<'a, T> {
    lock: &'a RwLock<T>,
}

impl<'a, T> Future for RwLockReadFuture<'a, T> {
    type Output = RwLockReadGuard<'a, T>;

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        let readers = &self.lock.readers;
        let readers_count = readers.get().saturating_add(1);

        let lock = unsafe { &mut *self.lock.locked.get() };
        if lock.is_none() && readers_count <= READERS_LIMIT {
            readers.replace(readers_count);
            Poll::Ready(RwLockReadGuard { lock: self.lock })
        } else {
            self.lock.queueu.enqueue(crate::msg::id());
            Poll::Pending
        }
    }
}

impl<'a, T> Future for RwLockWriteFuture<'a, T> {
    type Output = RwLockWriteGuard<'a, T>;

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        let lock = unsafe { &mut *self.lock.locked.get() };
        if lock.is_none() && self.lock.readers.get() == 0 {
            *lock = Some(crate::msg::id());
            Poll::Ready(RwLockWriteGuard { lock: self.lock })
        } else {
            self.lock.queueu.enqueue(crate::msg::id());
            Poll::Pending
        }
    }
}
