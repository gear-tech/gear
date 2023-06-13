// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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

#![no_std]

use parity_scale_codec::{Decode, Encode};

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

#[derive(Debug, Decode, Encode)]
pub enum Command {
    Common,
    Mutex,
}

#[cfg(not(feature = "std"))]
mod wasm {
    use crate::Command;
    use gstd::{lock::Mutex, msg, prelude::*, ActorId};

    static mut DESTINATION: ActorId = ActorId::zero();
    static MUTEX: Mutex<u32> = Mutex::new(0);

    #[no_mangle]
    extern "C" fn init() {
        let destination = msg::load().expect("Failed to load destination");
        unsafe { DESTINATION = destination };
    }

    async fn ping() -> Vec<u8> {
        msg::send_bytes_for_reply(unsafe { DESTINATION }, "PING", 0)
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
}
