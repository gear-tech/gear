// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use gstd::{ActorId, msg};

static mut DESTINATION: ActorId = ActorId::new([0u8; 32]);
static mut REPLY_DEPOSIT: u64 = 0;

#[gstd::async_main]
async fn main() {
    let input = msg::load_bytes().expect("Failed to load payload bytes");
    if let Ok(outcome) =
        msg::send_bytes_for_reply(unsafe { DESTINATION }, input, 0, unsafe { REPLY_DEPOSIT })
            .expect("Error sending message")
            .await
    {
        msg::reply_bytes(outcome, 0).expect("Failed to send reply");
    }
}

#[unsafe(no_mangle)]
extern "C" fn init() {
    let (destination, reply_deposit) = msg::load().expect("Expecting a program address");
    unsafe {
        DESTINATION = destination;
        REPLY_DEPOSIT = reply_deposit;
    }
    msg::reply((), 0).expect("Failed to send reply");
}
