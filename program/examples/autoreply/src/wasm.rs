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

use gstd::{ActorId, debug, exec, msg, prelude::*};

static mut DESTINATION: ActorId = ActorId::zero();
static mut RECEIVED_AUTO_REPLY: bool = false;

#[unsafe(no_mangle)]
extern "C" fn init() {
    debug!("init()");
    let destination = msg::load().expect("Failed to load destination");
    debug!("Destination: {destination:?}");
    unsafe { DESTINATION = destination };
}

#[unsafe(no_mangle)]
extern "C" fn handle() {
    debug!("handle()");
    let destination = unsafe { DESTINATION };
    if !destination.is_zero() {
        // Send message to receive an auto-reply
        let msg_id = msg::send_bytes(destination, b"Hi", 0).expect("Failed to send message");
        debug!("Sent message with ID: {msg_id:?}");

        exec::reply_deposit(msg_id, 10_000_000_000).expect("Failed to deposit reply");
    }
}

#[unsafe(no_mangle)]
extern "C" fn handle_reply() {
    debug!("handle_reply()");
    unsafe { RECEIVED_AUTO_REPLY = true };
}

#[unsafe(no_mangle)]
extern "C" fn state() {
    debug!("state()");
    msg::reply(unsafe { RECEIVED_AUTO_REPLY }, 0).expect("Failed to load reply");
}
