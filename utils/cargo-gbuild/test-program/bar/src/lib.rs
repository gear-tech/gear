// This file is part of Gear.

// Copyright (C) 2024 Gear Technologies Inc.
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

use gstd::msg;

static mut STATE: bool = false;

#[no_mangle]
extern "C" fn init() {
    let payload = msg::load_bytes().expect("Failed to load payload");
    gstd::debug!("Received payload: {payload:?}");
    if payload == b"PING" {
        msg::reply_bytes("INIT_PONG", 0).expect("Failed to send reply");
    }
}

#[no_mangle]
extern "C" fn handle() {
    let payload = msg::load_bytes().expect("Failed to load payload");

    if payload == b"PING" {
        msg::reply_bytes("HANDLE_PONG", 0).expect("Failed to send reply");
    }

    unsafe { STATE = true };
}

// State-sharing function
#[no_mangle]
extern "C" fn state() {
    msg::reply(unsafe { STATE.clone() }, 0).expect("Failed to share state");
}
