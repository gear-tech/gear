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
