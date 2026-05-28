// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::{HANDLE_REPLY_EXPECT, InputArgs};
use gstd::{ActorId, msg};

static mut DESTINATION: ActorId = ActorId::new([0u8; 32]);

#[unsafe(no_mangle)]
extern "C" fn init() {
    let args: InputArgs = msg::load().expect("Failed to decode `InputArgs'");
    unsafe { DESTINATION = args.destination.into() };
}

#[unsafe(no_mangle)]
extern "C" fn handle() {
    let payload = msg::load_bytes().expect("failed to load bytes");
    msg::send_bytes(unsafe { DESTINATION }, payload, msg::value()).expect("failed to send bytes");
}

#[unsafe(no_mangle)]
extern "C" fn handle_reply() {
    // Will panic here as replies denied in `handle_reply`.
    msg::reply_bytes([], 0).expect(HANDLE_REPLY_EXPECT);
}
