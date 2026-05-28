// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::Command;
use gstd::{ActorId, msg, prelude::*, sync::Mutex};

static mut DESTINATION: ActorId = ActorId::zero();
static MUTEX: Mutex<u32> = Mutex::new(0);

#[unsafe(no_mangle)]
extern "C" fn init() {
    let destination = msg::load().expect("Failed to load destination");
    unsafe { DESTINATION = destination };
}

async fn ping() -> Vec<u8> {
    #[cfg(not(feature = "ethexe"))]
    let fut = msg::send_bytes_for_reply(unsafe { DESTINATION }, "PING", 0, 0);
    #[cfg(feature = "ethexe")]
    let fut = msg::send_bytes_for_reply(unsafe { DESTINATION }, "PING", 0);

    fut.expect("Failed to send message")
        .await
        .expect("Received error reply")
}

#[gstd::async_main]
async fn main() {
    let command = msg::load().expect("Failed to load command");

    match command {
        Command::Common => {
            let r1 = ping().await;
            let r2 = ping().await;
            let r3 = ping().await;

            assert_eq!(r1, b"PONG");
            assert_eq!(r1, r2);
            assert_eq!(r2, r3);
        }
        Command::Mutex => {
            let _val = MUTEX.lock().await;

            msg::send(msg::source(), msg::id(), 0).expect("Failed to send message");
            let r = ping().await;

            assert_eq!(r, b"PONG");
        }
    }

    msg::reply(msg::id(), 0).expect("Failed to send reply");
}
