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

use gstd::{msg, MessageId, exec};

static mut MID: Option<MessageId> = None;
static mut DONE: bool = false;

#[no_mangle]
extern "C" fn init() {
    let delay: u32 = msg::load().unwrap();

    msg::send_bytes_delayed(msg::source(), "Delayed hello!", 0, delay).unwrap();
}

#[no_mangle]
extern "C" fn handle() {
    if let Some(message_id) = unsafe { MID.take() } {
        let delay: u32 = msg::load().unwrap();

        unsafe { DONE = true; }

        exec::wake_delayed(message_id, delay);
    } else if unsafe { !DONE } {
        unsafe { MID = Some(msg::id()); }

        exec::wait();
    }
}
