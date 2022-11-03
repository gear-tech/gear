// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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

//! Module for future-management.

use crate::{
    prelude::{BTreeMap, Box},
    MessageId,
};
use core::{
    future::Future,
    pin::Pin,
    task::{Context, Waker},
};
use futures::FutureExt;

pub(crate) type FuturesMap = BTreeMap<MessageId, Task>;

type PinnedFuture = Pin<Box<dyn Future<Output = ()> + 'static>>;

/// Matches a task to a some message in order to avoid duplicate execution
/// of code that was running before the program was interrupted by `wait`.
pub struct Task {
    waker: Waker,
    future: PinnedFuture,
}

impl Task {
    fn new<F>(future: F) -> Self
    where
        F: Future<Output = ()> + 'static,
    {
        Self {
            waker: super::waker::empty(),
            future: future.boxed_local(),
        }
    }
}

/// Gear allows users and programs to interact with other users and programs via
/// messages. This function enables an asynchronous message handling main loop.
pub fn message_loop<F>(future: F)
where
    F: Future<Output = ()> + 'static,
{
    let task = super::futures().entry(crate::msg::id()).or_insert_with(|| {
        // TODO: make this call configurable (#1380)
        crate::exec::system_reserve_gas(1_000_000_000)
            .expect("Failed to reserve gas for system signal");
        Task::new(future)
    });

    let mut cx = Context::from_waker(&task.waker);

    if Pin::new(&mut task.future).poll(&mut cx).is_ready() {
        super::futures().remove(&crate::msg::id());
    } else {
        // TODO: make this call configurable (#1380)
        crate::exec::wait_up_to(100)
    }
}
