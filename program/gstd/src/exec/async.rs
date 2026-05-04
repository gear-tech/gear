// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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

//! Module for asynchronous execution control functions which can be used
//! during message handling.

use crate::{MessageId, async_runtime, msg};
use core::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use gcore::exec;

/// Delays message execution in asynchronous way for the specified number of
/// blocks. It works pretty much like the [`exec::wait_for`] function, but
/// allows to continue execution after the delay in the same handler. It is
/// worth mentioning that the program state gets persisted inside the call, and
/// the execution resumes with potentially different state.
pub fn sleep_for(block_count: u32) -> impl Future<Output = ()> {
    MessageSleepFuture::new(msg::id(), exec::block_height().saturating_add(block_count))
}

struct MessageSleepFuture {
    msg_id: MessageId,
    till_block_number: u32,
}

impl MessageSleepFuture {
    fn new(msg_id: MessageId, till_block_number: u32) -> Self {
        Self {
            msg_id,
            till_block_number,
        }
    }
}

impl Future for MessageSleepFuture {
    type Output = ();

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        let current_block_number = exec::block_height();

        if current_block_number < self.till_block_number {
            async_runtime::locks().insert_sleep(self.msg_id, self.till_block_number);
            Poll::Pending
        } else {
            async_runtime::locks().remove_sleep(self.msg_id, self.till_block_number);
            Poll::Ready(())
        }
    }
}
