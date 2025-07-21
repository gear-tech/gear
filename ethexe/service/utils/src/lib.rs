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

#![allow(async_fn_in_trait)]

use futures::{StreamExt, future, stream::FusedStream};
use std::future::Future;

pub use task_local::LocalKey;
pub use timer::Timer;

mod task_local;
mod timer;

mod private {
    pub trait Sealed {}

    impl<T> Sealed for Option<T> {}
    impl<T> Sealed for &mut Option<T> {}
}

pub trait OptionFuture<T>: private::Sealed {
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

pub trait OptionStreamNext<T>: private::Sealed {
    async fn maybe_next(self) -> Option<T>;

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
