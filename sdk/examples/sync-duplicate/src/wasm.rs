// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use gstd::{ActorId, msg};

static mut COUNTER: i32 = 0;
static mut DESTINATION: ActorId = ActorId::zero();

#[unsafe(no_mangle)]
extern "C" fn init() {
    let destination = msg::load().expect("Failed to load destination");
    unsafe { DESTINATION = destination };
}

#[gstd::async_main]
async fn main() {
    let payload = msg::load_bytes().expect("Failed to load payload");

    if payload == b"async" {
        unsafe { COUNTER += 1 };

        let _ = msg::send_bytes_for_reply(unsafe { DESTINATION }, "PING", 0, 0)
            .expect("Failed to send message")
            .await
            .expect("Received error reply");

        msg::reply(unsafe { COUNTER }, 0).expect("Failed to send reply");

        unsafe { COUNTER = 0 };
    }
}
