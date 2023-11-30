// This file is part of Gear.

// Copyright (C) Gear Technologies Inc.
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

//! This contract shows the use of `delayed` syscalls, which make the syscall wait a given number
//! of blocks before being executed.
//!
//! The `init()` function calls [`send_bytes_delayed()`](msg::send_bytes_delayed), with a delay
//! which is taken from the payload.
//!
//! The `handle()` method, when given any non empty payload, will save the current message id, and
//! then call [`wait()`](exec::wait). A second execution of the `handle()` method will call
//! [`wake_delayed()`](exec::wake_delayed) with the saved message id, and a delay which is taken
//! from the payload. If the payload is empty however, it will
//! [`send_bytes_delayed()`](msg::send_bytes_delayed) twice to the source, with an empty payload
//! and a delay of [`DELAY`](crate::DELAY).

use crate::DELAY;
use gstd::{exec, msg, MessageId};

static mut MID: Option<MessageId> = None;
static mut DONE: bool = false;

#[no_mangle]
extern "C" fn init() {
    let delay: u32 = msg::load().unwrap();

    msg::send_bytes_delayed(msg::source(), "Delayed hello!", 0, delay).unwrap();
}

#[no_mangle]
extern "C" fn handle() {
    let size = msg::size();

    if size == 0 {
        // Another case of delayed sending, representing possible panic case of
        // sending delayed gasless messages.
        msg::send_bytes_delayed(msg::source(), [], 0, DELAY).expect("Failed to send msg");

        msg::send_bytes_delayed(msg::source(), [], 0, DELAY).expect("Failed to send msg");

        return;
    }

    // Common delayed sender case.
    if let Some(message_id) = unsafe { MID.take() } {
        let delay: u32 = msg::load().unwrap();

        unsafe {
            DONE = true;
        }

        exec::wake_delayed(message_id, delay).expect("Failed to wake message");
    } else if unsafe { !DONE } {
        unsafe {
            MID = Some(msg::id());
        }

        exec::wait();
    }
}
