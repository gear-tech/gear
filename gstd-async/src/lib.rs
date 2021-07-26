#![no_std]
#![cfg_attr(feature = "strict", deny(warnings))]

use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll, RawWaker, Waker};

/// Blocks the current thread on a future.
pub fn block_on<F, T>(future: F) -> T
where
    F: Future<Output = T>
{
    let mut future = unsafe {
        Pin::new_unchecked(&mut future)
    };

    let waker = unsafe { Waker::from_raw(RawWaker::new(raw, vtable)) };
    let cx = Context::from_waker(&waker);

    loop {
        // Poll the future.
        if let Poll::Ready(t) = future.poll(&mut cx) {
            return t;
        }
    }
}
