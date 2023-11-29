// This file is part of Gear.

// Copyright (C) 2023 Gear Technologies Inc.
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

//! This program demonstrates use of an async `handle` entry point using the `gstd::async_main`
//! macro.
//!
//! `Init` method gets an address of a program in the payload, which should accept "PING" messages
//! and reply with "PONG".
//!
//! `Handle` is async and gets a [`Command`] in the payload, executes a certain action based on it,
//! and send a [`reply()`] with a payload containing the id of the current message. There are two commands
//! that can be executed: [`Common`] and [`Mutex`].
//!
//! [`Common`] sends three async messages to the ping program, with the payload "PING",
//! awaits the reply and asserts that the reply is "PONG".
//!
//! [`Mutex`] asynchronously locks the mutex, awaiting it, sends a message back to the
//! source of the current message, containing the current message id in the payload, and then
//! it sends a ping message, awaiting the reply and asserting that the reply is "PONG".
//!
//! [`Common`]: Command::Common
//! [`Mutex`]: Command::Mutex
//! [`reply()`]: msg::reply

use crate::Command;
use gstd::{msg, prelude::*, sync::Mutex, ActorId};

static mut DESTINATION: ActorId = ActorId::zero();
static MUTEX: Mutex<u32> = Mutex::new(0);

#[no_mangle]
extern "C" fn init() {
    let destination = msg::load().expect("Failed to load destination");
    unsafe { DESTINATION = destination };
}

async fn ping() -> Vec<u8> {
    msg::send_bytes_for_reply(unsafe { DESTINATION }, "PING", 0, 0)
        .expect("Failed to send message")
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
