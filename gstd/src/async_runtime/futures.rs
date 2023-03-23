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

use crate::{prelude::Box, MessageId};
use core::{
    future::Future,
    pin::Pin,
    task::{Context, Waker},
};
use futures::FutureExt;
use hashbrown::HashMap;

pub(crate) type FuturesMap = HashMap<MessageId, Task>;

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

/// The main asynchronous message handling loop.
///
/// Gear allows user and program interaction via
/// messages. This function is the entry point to run the asynchronous message
/// processing.
pub fn message_loop<F>(future: F)
where
    F: Future<Output = ()> + 'static,
{
    let msg_id = crate::msg::id();
    let task = super::futures().entry(msg_id).or_insert_with(|| {
        let system_reserve_amount = crate::Config::system_reserve();
        crate::exec::system_reserve_gas(system_reserve_amount)
            .expect("Failed to reserve gas for system signal");
        Task::new(future)
    });

    let mut cx = Context::from_waker(&task.waker);

    if Pin::new(&mut task.future).poll(&mut cx).is_ready() {
        super::futures().remove(&msg_id);
        super::locks().remove_message_entry(msg_id);
    } else {
        super::locks().wait(msg_id);
    }
}
