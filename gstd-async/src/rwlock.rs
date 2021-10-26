use alloc::collections::VecDeque;
use core::{
    cell::{Cell, UnsafeCell},
    future::Future,
    ops::{Deref, DerefMut},
    pin::Pin,
    task::{Context, Poll},
};
use gcore::MessageId;

type ReadersCount = u8;
const READERS_LIMIT: ReadersCount = 32;

// Option<VecDeque> to make new `const fn`
struct RwLockWakes(UnsafeCell<Option<VecDeque<MessageId>>>);

impl RwLockWakes {
    fn add_wake(&self, message_id: MessageId) {
        unsafe {
            let mutable_option = &mut *self.0.get();

            let vec_deque = mutable_option.get_or_insert_with(VecDeque::new);
            vec_deque.push_back(message_id);
        }
    }

    fn dequeue_wake(&self) -> Option<MessageId> {
        unsafe { (*self.0.get()).as_mut().and_then(|v| v.pop_front()) }
    }

    const fn new() -> Self {
        RwLockWakes(UnsafeCell::new(None))
    }
}

pub struct RwLock<T> {
    locked: UnsafeCell<Option<MessageId>>,
    value: UnsafeCell<T>,
    readers: Cell<ReadersCount>,
    wakes: RwLockWakes,
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

                if let Some(message_id) = self.lock.wakes.dequeue_wake() {
                    gcore::exec::wake(message_id, 0);
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
            if let Some(message_id) = self.lock.wakes.dequeue_wake() {
                gcore::exec::wake(message_id, 0);
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
            self.lock.wakes.add_wake(gcore::msg::id());
            Poll::Pending
        }
    }
}

impl<'a, T> Future for RwLockWriteFuture<'a, T> {
    type Output = RwLockWriteGuard<'a, T>;

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        let lock = unsafe { &mut *self.lock.locked.get() };
        if lock.is_none() && self.lock.readers.get() == 0 {
            *lock = Some(gcore::msg::id());
            Poll::Ready(RwLockWriteGuard { lock: self.lock })
        } else {
            self.lock.wakes.add_wake(gcore::msg::id());
            Poll::Pending
        }
    }
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
            wakes: RwLockWakes::new(),
        }
    }
}
