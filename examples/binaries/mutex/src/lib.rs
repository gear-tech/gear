// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
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

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

#[cfg(not(feature = "std"))]
mod wasm {
    use core::num::ParseIntError;
    use gstd::{lock::mutex::Mutex, msg, prelude::*, ActorId};

    static mut PING_DEST: ActorId = ActorId::new([0u8; 32]);
    static MUTEX: Mutex<u32> = Mutex::new(0);

    #[no_mangle]
    extern "C" fn init() {
        let dest = String::from_utf8(msg::load_bytes().expect("Failed to load payload bytes"))
            .expect("Invalid message: should be utf-8");
        unsafe {
            PING_DEST = ActorId::from_slice(
                &decode_hex(dest.as_ref()).expect("INTIALIZATION FAILED: INVALID DEST PROGRAM ID"),
            )
            .expect("Unable to create ActorId")
        };
    }

    fn decode_hex(s: &str) -> Result<Vec<u8>, ParseIntError> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16))
            .collect()
    }

    #[gstd::async_main]
    async fn main() {
        let message = String::from_utf8(msg::load_bytes().expect("Failed to load payload bytes"))
            .expect("Invalid message: should be utf-8");
        if message == "START" {
            let _val = MUTEX.lock().await;

            let reply = msg::send_bytes_for_reply(unsafe { PING_DEST }, b"PING", 0)
                .unwrap()
                .await
                .expect("Error in async message processing");

            if reply == b"PONG" {
                msg::reply(b"SUCCESS", 0).unwrap();
            } else {
                msg::reply(b"FAIL", 0).unwrap();
            }
        }
    }
}
