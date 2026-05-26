// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use gstd::{ActorId, ext, msg, prelude::*};

static mut PANICKING_ID: ActorId = ActorId::zero();

#[unsafe(no_mangle)]
extern "C" fn init() {
    let id = msg::load().unwrap();
    unsafe {
        PANICKING_ID = id;
    }
}

#[unsafe(no_mangle)]
extern "C" fn handle() {
    if !unsafe { PANICKING_ID }.is_zero() {
        msg::send_bytes(unsafe { PANICKING_ID }, b"1234", 0).expect("Failed to send message");
    } else {
        ext::panic(b"\xE0\x80\x80");
    }
}

#[unsafe(no_mangle)]
extern "C" fn handle_reply() {
    let payload = msg::load_bytes().unwrap();
    assert_eq!(payload, b"\xE0\x80\x80");
}
