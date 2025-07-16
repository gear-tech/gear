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

use crate::Command;
use futures::{
    join, select_biased,
    stream::{FuturesUnordered, StreamExt},
};
use gstd::{ActorId, debug, msg, prelude::*};

static mut DEMO_ASYNC: ActorId = ActorId::new([0u8; 32]);
static mut DEMO_PING: ActorId = ActorId::new([0u8; 32]);

#[unsafe(no_mangle)]
extern "C" fn init() {
    let (demo_async, demo_ping) = msg::load().expect("Failed to load destination");
    unsafe {
        DEMO_ASYNC = demo_async;
        DEMO_PING = demo_ping;
    }
}

enum Dest {
    Async,
    Ping,
}

#[gstd::async_main]
async fn main() {
    let command = msg::load().expect("Failed to load command");
    let source = msg::source();

    let send_fut = |dest: Dest| {
        let (destination, payload) = match dest {
            Dest::Async => (unsafe { DEMO_ASYNC }, vec![0]), // demo_async::Command::Common
            Dest::Ping => (unsafe { DEMO_PING }, b"PING".to_vec()),
        };

        msg::send_bytes_for_reply(destination, payload, 0, 0).expect("Failed to send message")
    };

    match command {
        // Directly using stream from futures unordered
        // to step through each future ready
        Command::Unordered => {
            debug!("UNORDERED: Before any sending");

            let requests = vec![send_fut(Dest::Async), send_fut(Dest::Ping)];
            let mut unordered: FuturesUnordered<_> = requests.into_iter().collect();

            debug!("Before any polls");

            let first = unordered.next().await;
            msg::send_bytes(
                source,
                first.expect("Infallible").expect("Received error reply"),
                0,
            )
            .expect("Failed to send message");

            debug!("First (from demo_ping) done");

            let second = unordered.next().await;
            msg::send_bytes(
                source,
                second.expect("Infallible").expect("Received error reply"),
                0,
            )
            .expect("Failed to send message");

            debug!("Second (from demo_async) done");
        }
        // Using select! macro to wait for first future ready
        Command::Select => {
            debug!("SELECT: Before any sending");

            select_biased! {
                res = send_fut(Dest::Async) => {
                    debug!("Received msg from demo_async");
                    msg::send_bytes(source, res.expect("Received error reply"), 0).expect("Failed to send message");
                },
                res = send_fut(Dest::Ping) => {
                    debug!("Received msg from demo_ping");
                    msg::send_bytes(source, res.expect("Received error reply"), 0).expect("Failed to send message");
                },
            };

            debug!("Finish after select");
        }
        // Using join! macros to wait all features ready
        Command::Join => {
            debug!("JOIN: Before any sending");

            let res = join!(send_fut(Dest::Async), send_fut(Dest::Ping));

            debug!("Finish after join");

            let mut r1 = res.0.expect("Received error reply");
            let mut r2 = res.1.expect("Received error reply");

            let mut res = Vec::with_capacity(r1.len() + r2.len());

            res.append(&mut r1);
            res.append(&mut r2);

            msg::send_bytes(source, res, 0).expect("Failed to send message");
        }
    }

    msg::reply_bytes(msg::id(), 0).expect("Failed to send reply");
}
