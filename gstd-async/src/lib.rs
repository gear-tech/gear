#![no_std]
#![cfg_attr(feature = "strict", deny(warnings))]

extern crate alloc;

use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};

pub mod msg;
mod waker;

/// Blocks the current thread on a future.
pub fn block_on<F, T>(future: F) -> Option<T>
where
    F: Future<Output = T>,
{
    // Pin future
    let mut future = future;
    let future = unsafe { Pin::new_unchecked(&mut future) };

    // Create context based on an empty waker
    let waker = waker::empty();
    let mut cx = Context::from_waker(&waker);

    // Poll
    if let Poll::Ready(v) = future.poll(&mut cx) {
        Some(v)
    } else {
        None
    }
}
