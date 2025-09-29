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

use super::access::AccessQueue;
use crate::{
    BlockCount, BlockNumber, Config, MessageId, async_runtime,
    errors::{Error, Result, UsageError},
    exec, format, msg,
};
use core::{
    cell::UnsafeCell,
    future::Future,
    ops::{Deref, DerefMut},
    pin::Pin,
    task::{Context, Poll},
};

static mut NEXT_MUTEX_ID: MutexId = MutexId::new();

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub(crate) struct MutexId(u32);

impl MutexId {
    pub const fn new() -> Self {
        MutexId(0)
    }

    pub fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }
}

/// A mutual exclusion primitive useful for protecting shared data.
///
/// This mutex will block the execution waiting for the lock to become
/// available. The mutex can be created via a [`new`](Mutex::new) constructor.
/// Each mutex has a type parameter which represents the data that it is
/// protecting. The data can only be accessed through the RAII guard
/// [`MutexGuard`] returned from [`lock`](Mutex::lock),
/// which guarantees that data access only occurs when the mutex is
/// locked.
///
/// # Examples
///
/// This example (program A), after locking the mutex, sends the `PING` message
/// to another program (program B) and waits for a reply. If any other program
/// (program C) tries to invoke program A, it will wait until program A receives
/// the `PONG` reply from program B and unlocks the mutex.
///
/// ```ignored
/// use gstd::{msg, sync::Mutex, ActorId};
///
/// static mut DEST: ActorId = ActorId::zero();
/// static MUTEX: Mutex<()> = Mutex::new(());
///
/// #[unsafe(no_mangle)]
/// extern "C" fn init() {
///     // `some_address` can be obtained from the init payload
///     # let some_address = ActorId::zero();
///     unsafe { DEST = some_address };
/// }
///
/// #[gstd::async_main]
/// async fn main() {
///     let payload = msg::load_bytes().expect("Unable to load payload bytes");
///     if payload == b"START" {
///         let _unused = MUTEX.lock().await;
///
///         let reply = msg::send_bytes_for_reply(unsafe { DEST }, b"PING", 0, 0)
///             .expect("Unable to send bytes")
///             .await
///             .expect("Error in async message processing");
///
///         if reply == b"PONG" {
///             msg::reply(b"SUCCESS", 0).unwrap();
///         } else {
///             msg::reply(b"FAIL", 0).unwrap();
///         }
///     }
/// }
/// # fn main() {}
/// ```
pub struct Mutex<T> {
    id: UnsafeCell<Option<MutexId>>,
    locked: UnsafeCell<Option<(MessageId, BlockNumber)>>,
    value: UnsafeCell<T>,
    queue: AccessQueue,
}

impl<T> From<T> for Mutex<T> {
    fn from(t: T) -> Self {
        Mutex::new(t)
    }
}

impl<T: Default> Default for Mutex<T> {
    fn default() -> Self {
        <T as Default>::default().into()
    }
}

impl<T> Mutex<T> {
    /// Create a new mutex in an unlocked state ready for use.
    pub const fn new(t: T) -> Mutex<T> {
        Mutex {
            id: UnsafeCell::new(None),
            value: UnsafeCell::new(t),
            locked: UnsafeCell::new(None),
            queue: AccessQueue::new(),
        }
    }

    /// Acquire a mutex, protecting the subsequent code from execution by other
    /// actors until the mutex hasn't been unlocked.
    ///
    /// This function will block access to the section of code by
    /// other programs or users that invoke the same program. If another
    /// actor reaches the code blocked by the mutex, it goes to the wait
    /// state until the mutex unlocks. RAII guard wrapped in the future is
    /// returned to allow scoped unlock of the lock. When the guard goes out
    /// of scope, the mutex will be unlocked.
    pub fn lock(&self) -> MutexLockFuture<'_, T> {
        MutexLockFuture {
            mutex_id: self.get_or_assign_id(),
            mutex: self,
            own_up_for: None,
        }
    }

    // Returns a mutable reference to the mutex lock owner. The function uses unsafe
    // code because it is called from the places where there is only non-mutable
    // reference to the mutex exists, and the latter can't be turned into a
    // mutable one as it will break logic around the `Mutex.lock` function which
    // must be called on a non-mutable reference to the mutex.
    #[allow(clippy::mut_from_ref)]
    fn locked_by_mut(&self) -> &mut Option<(MessageId, BlockNumber)> {
        unsafe { &mut *self.locked.get() }
    }

    fn get_or_assign_id(&self) -> MutexId {
        let id = unsafe { &mut *self.id.get() };
        *id.get_or_insert_with(|| unsafe {
            let id = NEXT_MUTEX_ID;
            NEXT_MUTEX_ID = NEXT_MUTEX_ID.next();
            id
        })
    }
}

/// An RAII implementation of a "scoped lock" of a mutex. When this structure is
/// dropped (falls out of scope), the lock will be unlocked.
///
/// The data protected by the mutex is accessible through this guard via its
/// [`Deref`] and [`DerefMut`] implementations.
///
/// This structure wrapped in the future is returned by the
/// [`lock`](Mutex::lock) method on [`Mutex`].
pub struct MutexGuard<'a, T> {
    mutex: &'a Mutex<T>,
    holder_msg_id: MessageId,
}

impl<T> MutexGuard<'_, T> {
    #[track_caller]
    fn ensure_access_by_holder(&self) {
        let current_msg_id = msg::id();
        if self.holder_msg_id != current_msg_id {
            panic!(
                "Mutex guard held by message 0x{} is being accessed by message 0x{}",
                hex::encode(self.holder_msg_id),
                hex::encode(current_msg_id)
            );
        }
    }
}

impl<T> Drop for MutexGuard<'_, T> {
    fn drop(&mut self) {
        let is_holder_msg_signal_handler = match () {
            #[cfg(not(feature = "gearexe"))]
            () => msg::signal_from() == Ok(self.holder_msg_id),
            #[cfg(feature = "gearexe")]
            () => false,
        };

        if !is_holder_msg_signal_handler {
            self.ensure_access_by_holder();
        }

        let locked_by = self.mutex.locked_by_mut();
        let owner_msg_id = locked_by.map(|v| v.0);

        if owner_msg_id != Some(self.holder_msg_id) && !is_holder_msg_signal_handler {
            // If owner_msg_id is None or not equal to the holder_msg_id, firstly, it means
            // we are in the message signal handler and, secondly, the lock was seized by
            // some other message. In this case, the next rival message was
            // awoken by the ousting mechanism in the MutexLockFuture::poll
            panic!(
                "Mutex guard held by message 0x{} does not match lock owner message {}",
                hex::encode(self.holder_msg_id),
                owner_msg_id.map_or("None".into(), |v| format!("0x{}", hex::encode(v)))
            );
        }

        if owner_msg_id == Some(self.holder_msg_id) {
            if let Some(message_id) = self.mutex.queue.dequeue() {
                exec::wake(message_id).expect("Failed to wake the message");
            }
            *locked_by = None;
        }
    }
}

impl<'a, T> AsRef<T> for MutexGuard<'a, T> {
    fn as_ref(&self) -> &'a T {
        self.ensure_access_by_holder();
        unsafe { &*self.mutex.value.get() }
    }
}

impl<'a, T> AsMut<T> for MutexGuard<'a, T> {
    fn as_mut(&mut self) -> &'a mut T {
        self.ensure_access_by_holder();
        unsafe { &mut *self.mutex.value.get() }
    }
}

impl<T> Deref for MutexGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.ensure_access_by_holder();
        unsafe { &*self.mutex.value.get() }
    }
}

impl<T> DerefMut for MutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.ensure_access_by_holder();
        unsafe { &mut *self.mutex.value.get() }
    }
}

// we are always single-threaded
unsafe impl<T> Sync for Mutex<T> {}

/// The future returned by the [`lock`](Mutex::lock) method.
///
/// The output of the future is the [`MutexGuard`] that can be obtained by using
/// `await` syntax.
///
/// # Examples
///
/// In the following example, variable types are annotated explicitly for
/// demonstration purposes only. Usually, annotating them is unnecessary because
/// they can be inferred automatically.
///
/// ```
/// use gstd::sync::{Mutex, MutexGuard, MutexLockFuture};
///
/// #[gstd::async_main]
/// async fn main() {
///     let mutex: Mutex<i32> = Mutex::new(42);
///     let future: MutexLockFuture<i32> = mutex.lock();
///     let guard: MutexGuard<i32> = future.await;
///     let value: i32 = *guard;
///     assert_eq!(value, 42);
/// }
/// # fn main() {}
/// ```
pub struct MutexLockFuture<'a, T> {
    mutex_id: MutexId,
    mutex: &'a Mutex<T>,
    // The maximum number of blocks the mutex lock can be owned.
    // If the value is None, the default value taken from the `Config::mx_lock_duration` is used.
    own_up_for: Option<BlockCount>,
}

impl<'a, T> MutexLockFuture<'a, T> {
    /// Sets the maximum number of blocks the mutex lock can be owned by
    /// some message before the ownership can be seized by another rival
    pub fn own_up_for(self, block_count: BlockCount) -> Result<Self> {
        if block_count == 0 {
            Err(Error::Gstd(UsageError::ZeroMxLockDuration))
        } else {
            Ok(MutexLockFuture {
                mutex_id: self.mutex_id,
                mutex: self.mutex,
                own_up_for: Some(block_count),
            })
        }
    }

    fn acquire_lock_ownership(
        &mut self,
        owner_msg_id: MessageId,
        current_block: BlockNumber,
    ) -> Poll<MutexGuard<'a, T>> {
        let owner_deadline_block =
            current_block.saturating_add(self.own_up_for.unwrap_or_else(Config::mx_lock_duration));
        async_runtime::locks().remove_mx_lock_monitor(owner_msg_id, self.mutex_id);
        if let Some(next_rival_msg_id) = self.mutex.queue.first() {
            // Give the next rival message a chance to own the lock after this owner
            // exceeds the lock ownership duration
            async_runtime::locks().insert_mx_lock_monitor(
                *next_rival_msg_id,
                self.mutex_id,
                owner_deadline_block,
            );
        }
        let locked_by = self.mutex.locked_by_mut();
        *locked_by = Some((owner_msg_id, owner_deadline_block));
        Poll::Ready(MutexGuard {
            mutex: self.mutex,
            holder_msg_id: owner_msg_id,
        })
    }

    fn queue_for_lock_ownership(
        &mut self,
        rival_msg_id: MessageId,
        owner_deadline_block: Option<BlockNumber>,
    ) -> Poll<MutexGuard<'a, T>> {
        // If the message is already in the access queue, and we come here,
        // it means the message has just been woken up from the waitlist.
        // In that case we do not want to register yet another access attempt
        // and just go back to the waitlist
        if !self.mutex.queue.contains(&rival_msg_id) {
            self.mutex.queue.enqueue(rival_msg_id);
            if let Some(owner_deadline_block) = owner_deadline_block {
                // Lock owner did not know about this message when it was getting into
                // lock ownership. We have to take care of ourselves and give us a chance
                // to oust the lock owner when the lock ownership duration expires
                if self.mutex.queue.len() == 1 {
                    async_runtime::locks().insert_mx_lock_monitor(
                        rival_msg_id,
                        self.mutex_id,
                        owner_deadline_block,
                    );
                }
            }
        }
        Poll::Pending
    }
}

impl<'a, T> Future for MutexLockFuture<'a, T> {
    type Output = MutexGuard<'a, T>;

    // In case of locked mutex and an `.await`, function `poll` checks if the
    // mutex can be taken, else it waits (goes into *waiting queue*).
    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        let current_msg_id = msg::id();
        let current_block = exec::block_height();
        let locked_by = self.mutex.locked_by_mut();

        if locked_by.is_none() {
            return self
                .get_mut()
                .acquire_lock_ownership(current_msg_id, current_block);
        }

        let (lock_owner_msg_id, deadline_block) =
            (*locked_by).unwrap_or_else(|| unreachable!("Checked above"));

        if current_block < deadline_block {
            return self
                .get_mut()
                .queue_for_lock_ownership(current_msg_id, Some(deadline_block));
        }

        if let Some(msg_future_task) = async_runtime::futures().get_mut(&lock_owner_msg_id) {
            msg_future_task.set_lock_exceeded();
            exec::wake(lock_owner_msg_id).expect("Failed to wake the message");
        }

        while let Some(next_msg_id) = self.mutex.queue.dequeue() {
            if next_msg_id == lock_owner_msg_id {
                continue;
            }
            if next_msg_id == current_msg_id {
                break;
            }
            exec::wake(next_msg_id).expect("Failed to wake the message");
            *locked_by = None;
            // We have just woken up the next lock owner, but we don't know its ownership
            // duration, thus we pass None as owner_deadline_block. The woken up message
            // will give us a chance to own the lock itself by registering a
            // lock monitor for us
            return self
                .get_mut()
                .queue_for_lock_ownership(current_msg_id, None);
        }

        self.get_mut()
            .acquire_lock_ownership(current_msg_id, current_block)
    }
}
