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

/// A reader-writer lock.
///
/// This type of lock allows a number of readers or at most one writer at any
/// point in time. The write portion of this lock typically allows modification
/// of the underlying data (exclusive access) and the read portion of this lock
/// typically allows for read-only access (shared access).
///
/// In comparison, a [`Mutex`](super::Mutex) does not distinguish between
/// readers or writers that acquire the lock, therefore blocking any actors
/// waiting for the lock to become available. An `RwLock` will allow any number
/// of readers to acquire the lock as long as a writer is not holding the lock.
///
/// The type parameter `T` represents the data that this lock protects. The RAII
/// guards returned from the locking methods implement [`Deref`] (and
/// [`DerefMut`] for the `write` methods) to allow access to the content of the
/// lock.
///
/// # Examples
///
/// The following program processes several messages. It locks the `RwLock` for
/// reading when processing one of the `get` commands and for writing in the
/// case of the `inc` command.
///
/// ```
/// use gstd::{lock::RwLock, msg, prelude::*, ActorId};
///
/// static mut DEST: ActorId = ActorId::zero();
/// static RWLOCK: RwLock<u32> = RwLock::new(0);
///
/// #[no_mangle]
/// extern "C" fn init() {
///     // `some_address` can be obtained from the init payload
///     # let some_address = ActorId::zero();
///     unsafe { DEST = some_address };
/// }
///
/// #[gstd::async_main]
/// async fn main() {
///     let payload = msg::load_bytes().expect("Unable to load payload bytes");
///
///     match payload.as_slice() {
///         b"get" => {
///             msg::reply(*RWLOCK.read().await, 0).unwrap();
///         }
///         b"inc" => {
///             let mut val = RWLOCK.write().await;
///             *val += 1;
///         }
///         b"ping&get" => {
///             let _ = msg::send_bytes_for_reply(unsafe { DEST }, b"PING", 0)
///                 .expect("Unable to send bytes")
///                 .await
///                 .expect("Error in async message processing");
///             msg::reply(*RWLOCK.read().await, 0).unwrap();
///         }
///         b"inc&ping" => {
///             let mut val = RWLOCK.write().await;
///             *val += 1;
///             let _ = msg::send_bytes_for_reply(unsafe { DEST }, b"PING", 0)
///                 .expect("Unable to send bytes")
///                 .await
///                 .expect("Error in async message processing");
///         }
///         b"get&ping" => {
///             let val = RWLOCK.read().await;
///             let _ = msg::send_bytes_for_reply(unsafe { DEST }, b"PING", 0)
///                 .expect("Unable to send bytes")
///                 .await
///                 .expect("Error in async message processing");
///             msg::reply(*val, 0).unwrap();
///         }
///         _ => {
///             let _write = RWLOCK.write().await;
///             RWLOCK.read().await;
///         }
///     }
/// }
///
/// # fn main() {}
/// ```
pub struct RwLock<T> {
    locked: UnsafeCell<Option<MessageId>>,
    value: UnsafeCell<T>,
    readers: Cell<ReadersCount>,
    queue: AccessQueue,
}

impl<T> RwLock<T> {
    /// Create a new instance of an `RwLock<T>` which is unlocked.
    pub const fn new(t: T) -> RwLock<T> {
        RwLock {
            value: UnsafeCell::new(t),
            locked: UnsafeCell::new(None),
            readers: Cell::new(0),
            queue: AccessQueue::new(),
        }
    }

    /// Locks this rwlock with shared read access, protecting the subsequent
    /// code from executing by other actors until it can be acquired.
    ///
    /// The underlying code section will be blocked until there are no more
    /// writers who hold the lock. There may be other readers currently inside
    /// the lock when this method returns. This method does not provide any
    /// guarantees with respect to the ordering of whether contentious readers
    /// or writers will acquire the lock first.
    ///
    /// Returns an RAII guard, which will release this thread's shared access
    /// once it is dropped.
    pub fn read(&self) -> RwLockReadFuture<'_, T> {
        RwLockReadFuture { lock: self }
    }

    /// Locks this rwlock with exclusive write access, blocking the underlying
    /// code section until it can be acquired.
    ///
    /// This function will not return while other writers or other readers
    /// currently have access to the lock.
    ///
    /// Returns an RAII guard which will drop the write access of this rwlock
    /// when dropped.
    pub fn write(&self) -> RwLockWriteFuture<'_, T> {
        RwLockWriteFuture { lock: self }
    }
}

// we are always single-threaded
unsafe impl<T> Sync for RwLock<T> {}

/// RAII structure used to release the shared read access of a lock when
/// dropped.
///
/// This structure wrapped in the future is returned by the
/// [`read`](RwLock::read) method on [`RwLock`].
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

                if let Some(message_id) = self.lock.queue.dequeue() {
                    crate::exec::wake(message_id).expect("Failed to wake the message");
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

/// RAII structure used to release the exclusive write access of a lock when
/// dropped.
///
/// This structure wrapped in the future is returned by the
/// [`write`](RwLock::write) method on [`RwLock`].
pub struct RwLockWriteGuard<'a, T> {
    lock: &'a RwLock<T>,
}

impl<'a, T> Drop for RwLockWriteGuard<'a, T> {
    fn drop(&mut self) {
        unsafe {
            *self.lock.locked.get() = None;
            if let Some(message_id) = self.lock.queue.dequeue() {
                crate::exec::wake(message_id).expect("Failed to wake the message");
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

/// The future returned by the [`read`](RwLock::read) method.
///
/// The output of the future is the [`RwLockReadGuard`] that can be obtained by
/// using `await` syntax.
///
/// # Examples
///
/// The following example explicitly annotates variable types for demonstration
/// purposes only. Usually, annotating types is unnecessary since
/// they can be inferred automatically.
///
/// ```
/// use gstd::lock::{RwLock, RwLockReadFuture, RwLockReadGuard};
///
/// #[gstd::async_main]
/// async fn main() {
///     let rwlock: RwLock<i32> = RwLock::new(42);
///     let future: RwLockReadFuture<i32> = rwlock.read();
///     let guard: RwLockReadGuard<i32> = future.await;
///     let value: i32 = *guard;
///     assert_eq!(value, 42);
/// }
///
/// # fn main() {}
/// ```
pub struct RwLockReadFuture<'a, T> {
    lock: &'a RwLock<T>,
}

/// The future returned by the [`write`](RwLock::write) method.
///
/// The output of the future is the [`RwLockWriteGuard`] that can be obtained by
/// using `await` syntax.
///
/// # Examples
///
/// ```
/// use gstd::lock::{RwLock, RwLockWriteFuture, RwLockWriteGuard};
///
/// #[gstd::async_main]
/// async fn main() {
///     let rwlock: RwLock<i32> = RwLock::new(42);
///     let future: RwLockWriteFuture<i32> = rwlock.write();
///     let mut guard: RwLockWriteGuard<i32> = future.await;
///     let value: i32 = *guard;
///     assert_eq!(value, 42);
///     *guard = 84;
///     assert_eq!(*guard, 42);
/// }
///
/// # fn main() {}
/// ```
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
            self.lock.queue.enqueue(crate::msg::id());
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
            self.lock.queue.enqueue(crate::msg::id());
            Poll::Pending
        }
    }
}
