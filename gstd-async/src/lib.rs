#![no_std]
#![cfg_attr(feature = "strict", deny(warnings))]

extern crate alloc;

use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};

mod waker;

/// Blocks the current thread on a future.
pub fn block_on<F, T>(future: F) -> T
where
    F: Future<Output = T>,
{
    let mut future = future;
    let mut future = unsafe { Pin::new_unchecked(&mut future) };

    let waker = waker::from_fn(gr_wake);
    let mut cx = Context::from_waker(&waker);

    loop {
        if let Poll::Ready(t) = future.as_mut().poll(&mut cx) {
            return t;
        }
    }
}

fn gr_wake() {
    // TODO: (?) Replace it by syscall for more advanced use cases
}
