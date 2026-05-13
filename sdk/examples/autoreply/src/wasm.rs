// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

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
