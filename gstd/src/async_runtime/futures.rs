// This file is part of Gear.

// Copyright (C) 2021 Gear Technologies Inc.
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

use crate::common::MessageId;
use crate::prelude::{BTreeMap, Box};
use core::{
    future::Future,
    pin::Pin,
    ptr,
    task::{Context, RawWaker, RawWakerVTable, Waker},
};
use futures::FutureExt;

const VTABLE: RawWakerVTable = RawWakerVTable::new(clone_waker, wake, wake_by_ref, drop_waker);

fn empty_waker() -> Waker {
    unsafe { Waker::from_raw(RawWaker::new(ptr::null(), &VTABLE)) }
}

type LocalBoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;
pub(crate) type FuturesMap = BTreeMap<MessageId, LocalBoxFuture<'static, ()>>;

/// Asynchronous message handling main loop.
pub fn event_loop<F>(future: F)
where
    F: Future<Output = ()> + 'static,
{
    let mut current_future = crate::async_runtime::futures()
        .remove(&crate::msg::id())
        .unwrap_or_else(|| future.boxed_local());

    // Create context based on an empty waker
    let waker = empty_waker();
    let mut cx = Context::from_waker(&waker);

    let pinned = Pin::new(&mut current_future);

    if pinned.poll(&mut cx).is_ready() {
        // Done!
    } else {
        crate::async_runtime::futures().insert(crate::msg::id(), current_future);
        crate::exec::wait()
    }
}

unsafe fn clone_waker(ptr: *const ()) -> RawWaker {
    RawWaker::new(ptr, &VTABLE)
}
unsafe fn wake(_ptr: *const ()) {}
unsafe fn wake_by_ref(_ptr: *const ()) {}
unsafe fn drop_waker(_ptr: *const ()) {}
