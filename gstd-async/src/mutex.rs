use alloc::collections::VecDeque;
use core::{
    cell::UnsafeCell,
    future::Future,
    ops::{Deref, DerefMut},
    pin::Pin,
    task::{Context, Poll},
};
use gcore::MessageId;

// Option<VecDeque> to make new `const fn`
struct MutexWakes(UnsafeCell<Option<VecDeque<MessageId>>>);

impl MutexWakes {
    fn add_wake(&self, message_id: MessageId) {
        unsafe {
            let mutable_option = &mut *self.0.get();

            let mut vec_deque = mutable_option.take().unwrap_or_else(VecDeque::new);
            vec_deque.push_back(message_id);

            *mutable_option = Some(vec_deque);
        }
    }

    fn dequeue_wake(&self) -> Option<MessageId> {
        unsafe {
            match &mut *self.0.get() {
                Some(vec_deque) => vec_deque.pop_front(),
                None => None,
            }
        }
    }

    const fn new() -> Self {
        MutexWakes(UnsafeCell::new(None))
    }
}

pub struct Mutex<T> {
    locked: UnsafeCell<Option<MessageId>>,
    value: UnsafeCell<T>,
    wakes: MutexWakes,
}

pub struct MutexGuard<'a, T> {
    mutex: &'a Mutex<T>,
}

impl<'a, T> Drop for MutexGuard<'a, T> {
    fn drop(&mut self) {
        unsafe {
            *self.mutex.locked.get() = None;
            if let Some(message_id) = self.mutex.wakes.dequeue_wake() {
                gcore::exec::wake(message_id, 0);
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

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        let lock = unsafe { &mut *self.mutex.locked.get() };
        if lock.is_none() {
            *lock = Some(gcore::msg::id());
            Poll::Ready(MutexGuard { mutex: self.mutex })
        } else {
            self.mutex.wakes.add_wake(gcore::msg::id());
            Poll::Pending
        }
    }
}

impl<T> Mutex<T> {
    pub fn lock(&self) -> MutexLockFuture<'_, T> {
        MutexLockFuture { mutex: self }
    }

    pub const fn new(t: T) -> Mutex<T> {
        Mutex {
            value: UnsafeCell::new(t),
            locked: UnsafeCell::new(None),
            wakes: MutexWakes::new(),
        }
    }
}
