// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! # ethexe-service-utils
//!
//! Async-runtime helpers that make `tokio::select!`-style event loops cleaner when
//! service branches are optional or conditionally present.
//!
//! ## Responsibilities
//!
//! Three focused primitives, no ethexe domain logic:
//!
//! - **Optional-branch adapters** — [`OptionFuture`] and [`OptionStreamNext`] let an
//!   `Option<Future>` or `&mut Option<Stream>` sit in a `select!` arm without special
//!   casing: a `None` variant stays pending forever rather than resolving or panicking.
//! - **Named restartable timer** — [`Timer`] wraps `tokio::time::Sleep` and carries
//!   arbitrary data `T`; it resolves to that data when the deadline elapses and stays
//!   pending until restarted with `start`.
//! - **Mutable task-local storage** — the [`task_local!`] macro declares a
//!   `static LocalKey<T>` that behaves like `tokio::task_local!` but permits mutable
//!   access via [`LocalKey::with_mut`] and returns the stored value out of the scope.
//!
//! ## Role in the Stack
//!
//! This crate has no dependency on any other ethexe crate. It is consumed by:
//!
//! - `ethexe-service` — imports [`OptionFuture`] and [`OptionStreamNext`] in its main
//!   service `select!` loop to drive optional subsystems (consensus, network, RPC,
//!   Prometheus) without conditional branches.
//! - `ethexe-network` — uses [`task_local!`] in its database-sync request handler for
//!   mutable per-task state.
//!
//! ## Public API
//!
//! | Item | Kind | Notes |
//! |------|------|-------|
//! | [`OptionFuture`] | sealed trait | `.maybe()` on `Option<F: Future>` |
//! | [`OptionStreamNext`] | sealed trait | `.maybe_next()` / `.maybe_next_some()` on `&mut Option<S>` and `&mut FuturesUnordered<F>` |
//! | [`Timer`] | struct | `new` / `new_from_secs` / `new_from_millis`; `start` / `stop` / `started` |
//! | [`LocalKey`] | struct | `scope` / `with_mut` / `poll_fn` |
//! | `task_local!` | macro | declares a `static LocalKey<T>` |
//!
//! Both traits are sealed: only the impls provided in this crate satisfy them.
//!
//! Calling `maybe_next` (not `maybe_next_some`) on a `&mut FuturesUnordered` panics by
//! design — only `maybe_next_some` is supported for that type.
//!
//! ## Usage
//!
//! ```rust,no_run
//! use ethexe_service_utils::{OptionFuture as _, OptionStreamNext as _, Timer};
//!
//! // Bring traits into scope as `_` (they are sealed; only their methods matter).
//! // Optional services stay pending when absent; present ones yield events normally.
//! tokio::select! {
//!     event = consensus.maybe_next_some() => handle(event),
//!     event = network.maybe_next_some()   => handle(event),
//!     data  = &mut retry_timer            => retry(data),
//! }
//!
//! // Timer: arm with data, await, rearm.
//! let mut t: Timer<u32> = Timer::new_from_secs("retry", 5);
//! t.start(42);
//! // resolves to 42 after 5 s; pending again until start() is called
//! ```
#![allow(async_fn_in_trait)]

use futures::{
    StreamExt, future,
    stream::{FusedStream, FuturesUnordered},
};
use std::future::Future;

pub use task_local::LocalKey;
pub use timer::Timer;

mod task_local;
mod timer;

mod private {
    use futures::stream::FuturesUnordered;

    pub trait Sealed {}

    impl<T> Sealed for Option<T> {}
    impl<T> Sealed for &mut Option<T> {}
    impl<F> Sealed for &mut FuturesUnordered<F> {}
}

/// Extends `Option<F>` with a `.maybe()` combinator for use in `tokio::select!` arms.
///
/// When the option is `Some`, the inner future is driven to completion. When it is `None`,
/// the resulting future stays pending forever, so the `select!` arm is silently skipped.
pub trait OptionFuture<T>: private::Sealed {
    /// Await the inner future if `Some`, otherwise remain pending indefinitely.
    async fn maybe(self) -> T;
}

impl<F: Future> OptionFuture<F::Output> for Option<F> {
    async fn maybe(self) -> F::Output {
        if let Some(f) = self {
            f.await
        } else {
            future::pending().await
        }
    }
}

/// Extends `&mut Option<S>` and `&mut FuturesUnordered<F>` with stream-polling combinators
/// suitable for `tokio::select!` arms.
///
/// A `None` stream or an empty `FuturesUnordered` stays pending, so the owning `select!`
/// arm is silently skipped rather than resolving or panicking.
pub trait OptionStreamNext<T>: private::Sealed {
    /// Poll the next item from an optional stream, yielding `None` when the stream ends.
    ///
    /// Panics if called on `&mut FuturesUnordered`; use [`maybe_next_some`](Self::maybe_next_some) instead.
    async fn maybe_next(self) -> Option<T>;

    /// Poll the next item from an optional stream or `FuturesUnordered`, staying pending when
    /// there are no items rather than returning `None`.
    async fn maybe_next_some(self) -> T;
}

impl<S: StreamExt + FusedStream + Unpin> OptionStreamNext<S::Item> for &mut Option<S> {
    async fn maybe_next(self) -> Option<S::Item> {
        self.as_mut().map(StreamExt::next).maybe().await
    }

    async fn maybe_next_some(self) -> S::Item {
        self.as_mut().map(StreamExt::select_next_some).maybe().await
    }
}

impl<F: Future> OptionStreamNext<F::Output> for &mut FuturesUnordered<F> {
    async fn maybe_next(self) -> Option<F::Output> {
        unimplemented!("Do not use maybe_next on FuturesUnordered");
    }

    async fn maybe_next_some(self) -> F::Output {
        if let Some(res) = self.next().await {
            res
        } else {
            future::pending().await
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::OptionFuture;
    use futures::future::{self, Ready};
    use std::{
        future::Future,
        pin::pin,
        task::{Context, Poll, Waker},
        thread,
        time::{Duration, Instant},
    };

    #[test]
    fn maybe_polling() {
        let mut cx = Context::from_waker(Waker::noop());

        let some_ready = Some(future::ready(42)).maybe();
        assert_eq!(pin!(some_ready).poll(&mut cx), Poll::Ready(42));

        let some_pending = Some(future::pending::<()>()).maybe();
        assert!(pin!(some_pending).poll(&mut cx).is_pending());

        let none_ready = None::<Ready<()>>.maybe();
        assert!(pin!(none_ready).poll(&mut cx).is_pending());

        let none_pending = None::<Ready<()>>.maybe();
        assert!(pin!(none_pending).poll(&mut cx).is_pending());

        let instant = Instant::now();

        let some_changes_fut = Some(future::poll_fn(move |_cx| {
            if instant.elapsed() >= Duration::from_secs(3) {
                Poll::Ready(())
            } else {
                Poll::Pending
            }
        }))
        .maybe();
        let mut some_changes = pin!(some_changes_fut);

        assert!(some_changes.as_mut().poll(&mut cx).is_pending());

        thread::sleep(Duration::from_secs(1));
        assert!(some_changes.as_mut().poll(&mut cx).is_pending());

        thread::sleep(Duration::from_secs(2));
        assert!(some_changes.poll(&mut cx).is_ready());
    }
}
