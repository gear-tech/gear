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

// for panic/oom handlers

use gstd::{MessageId, debug, exec, msg, prelude::*};

static mut STATE: u32 = 0;
static mut MSG_ID_1: MessageId = MessageId::zero();
static mut MSG_ID_2: MessageId = MessageId::zero();

#[unsafe(no_mangle)]
extern "C" fn handle() {
    let state = unsafe { static_mut!(STATE) };
    debug!("{state}");

    match *state {
        0 => {
            *state = 1;
            unsafe { MSG_ID_1 = msg::id() };
            exec::wait();
        }
        1 => {
            *state = 2;
            unsafe { MSG_ID_2 = msg::id() };
            exec::wait();
        }
        2 => {
            *state = 3;
            exec::wake(unsafe { MSG_ID_1 }).unwrap();
            exec::wake(unsafe { MSG_ID_2 }).unwrap();
        }
        _ => {
            msg::send_bytes(msg::source(), msg::id(), 0).unwrap();
        }
    }
}
