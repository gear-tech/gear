// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

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
