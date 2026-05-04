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

use crate::DELAY;
use gstd::{MessageId, exec, msg, prelude::*};

static mut MID: Option<MessageId> = None;
static mut DONE: bool = false;

fn send_delayed_to_self() -> bool {
    let to_self = msg::load_bytes().unwrap().as_slice() == b"self";
    if to_self {
        gstd::debug!("sending delayed message to self");
        msg::send_bytes_delayed(exec::program_id(), b"self", 0, DELAY).unwrap();
    }

    to_self
}

#[unsafe(no_mangle)]
extern "C" fn init() {
    // Send message to self
    if send_delayed_to_self() {
        return;
    }

    let delay: u32 = msg::load().unwrap();

    msg::send_bytes_delayed(msg::source(), "Delayed hello!", 0, delay).unwrap();
}

#[unsafe(no_mangle)]
extern "C" fn handle() {
    if send_delayed_to_self() {
        return;
    }

    let size = msg::size();

    if size == 0 {
        // Another case of delayed sending, representing possible panic case of
        // sending delayed gasless messages.
        msg::send_bytes_delayed(msg::source(), [], 0, DELAY).expect("Failed to send msg");

        msg::send_bytes_delayed(msg::source(), [], 0, DELAY).expect("Failed to send msg");

        return;
    }

    // Common delayed sender case.
    if let Some(message_id) = unsafe { static_mut!(MID).take() } {
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
