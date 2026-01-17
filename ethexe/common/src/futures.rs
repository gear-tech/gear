// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Future utilities for ethexe.

pub use futures::*;
use std::task::{Context, Poll};

/// A future that measures the time taken to complete.
/// Designed to use like this:
/// ```no_run
/// let future = async {
///     // some async work
/// };
/// let timed_future = future.boxed().timed();
/// let (delay, result) = timed_future.await;
/// ```
#[pin_project::pin_project]
pub struct TimedFuture<F> {
    /// The inner future being measured.
    #[pin]
    inner: F,
    /// The start time of the future.
    start: std::time::Instant,
}

/// Extension trait for futures to add timing functionality.
pub trait TimedFutureExt: Future + Sized {
    /// Wraps the future to measured [`TimedFuture`].
    fn timed(self) -> TimedFuture<Self> {
        TimedFuture {
            inner: self,
            start: std::time::Instant::now(),
        }
    }
}

/// Blanked implementation for all futures.
impl<F: Future> TimedFutureExt for F {}

/// Implementation [`Future`] trait for [`TimedFuture`].
impl<F> Future for TimedFuture<F>
where
    F: Future,
{
    type Output = (f64, F::Output);

    fn poll(mut self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let result = futures::ready!(self.as_mut().project().inner.poll_unpin(cx));
        let delay = std::time::Instant::now()
            .duration_since(self.start)
            .as_secs_f64();
        Poll::Ready((delay, result))
    }
}

/// Extension trait to transpose a timed result.
/// For a value of type `(f64, Result<T, E>)`, it produces a value of type `Result<(f64, T), E>`.
/// This is useful for handling results from timed futures.
pub trait TransposeTimedResult<T, E> {
    fn transpose(self) -> Result<(f64, T), E>;
}

impl<T, E> TransposeTimedResult<T, E> for (f64, Result<T, E>) {
    fn transpose(self) -> Result<(f64, T), E> {
        self.1.map(|res| (self.0, res))
    }
}
