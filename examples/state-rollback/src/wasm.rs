// This file is part of Gear.

// Copyright (C) 2023-2025 Gear Technologies Inc.
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

use gstd::{exec, msg, prelude::*};

static mut PAYLOAD: Option<Vec<u8>> = None;

#[unsafe(no_mangle)]
extern "C" fn handle() {
    let payload = msg::load_bytes().expect("Failed to load payload");

    // Previous value
    msg::send(msg::source(), unsafe { static_ref!(PAYLOAD) }, 0).expect("Failed to send message");

    let is_panic = payload == b"panic";
    let is_leave = payload == b"leave";

    // New value setting
    unsafe { PAYLOAD = Some(payload) };

    // Newly set value
    msg::reply(unsafe { static_ref!(PAYLOAD) }, 0).expect("Failed to send reply");

    // Stop execution with panic.
    is_panic.then(|| panic!());

    // Stop execution with leave.
    is_leave.then(|| exec::leave());
}
