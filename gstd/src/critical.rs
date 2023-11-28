// This file is part of Gear.

// Copyright (C) 2023 Gear Technologies Inc.
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

//! Critical section that guarantees code section execution
//!
//! Code is executed in `handle_signal` entry point in case of failure
//! only across `.await` calls because section has to be saved.
//!
//! ```rust,no_run
//! use gstd::{critical::{self, SectionFutureExt}, msg};
//!
//! # async fn _dummy() {
//!
//! // get source outside of critical section
//! // because `gr_source` sys-call is forbidden inside `handle_signal` entry point
//! let source = msg::source();
//!
//! msg::send_for_reply(msg::source(), "for_reply", 0, 0)
//!     .expect("Failed to send message")
//!     // register section
//!     .critical(|| {
//!         msg::send(source, "example", 0).expect("Failed to send message");
//!     })
//!     // section will be saved now during `.await`
//!     .await
//!     .expect("Received error reply");
//!
//! // if some code fails (panic, out of gas, etc) after `.await`
//! // then saved section will be executed in `handle_signal`
//!
//! // your code
//! // ...
//!
//! # }
//! ```

use alloc::boxed::Box;
use core::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use pin_project::{pin_project, pinned_drop};

static mut SECTION: Option<Box<dyn FnMut()>> = None;

pub(crate) fn section() -> &'static mut Option<Box<dyn FnMut()>> {
    unsafe { &mut SECTION }
}

/// Critical section future.
#[pin_project(PinnedDrop)]
#[must_use = "Future must be polled"]
pub struct SectionFuture<Fut> {
    #[pin]
    fut: Fut,
}

impl<Fut> Future for SectionFuture<Fut>
where
    Fut: Future,
{
    type Output = Fut::Output;

    fn poll(self: Pin<&mut SectionFuture<Fut>>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.project().fut.poll(cx)
    }
}

#[pinned_drop]
impl<Fut> PinnedDrop for SectionFuture<Fut> {
    fn drop(self: Pin<&mut Self>) {
        let _ = section().take();
    }
}

/// Extension for [`Future`].
pub trait SectionFutureExt: Future + Sized {
    /// Register critical section.
    fn critical<Func>(self, f: Func) -> SectionFuture<Self>
    where
        Func: FnMut() + 'static;
}

impl<F> SectionFutureExt for F
where
    F: Future,
{
    fn critical<Func>(self, func: Func) -> SectionFuture<Self>
    where
        Func: FnMut() + 'static,
    {
        let prev = section().replace(Box::new(func));
        assert!(prev.is_none());
        SectionFuture { fut: self }
    }
}
