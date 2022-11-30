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
    use gstd::{exec, msg, ActorId};

    static mut USER: ActorId = ActorId::zero();

    #[gstd::async_init(handle_reply = my_handle_reply, handle_signal = my_handle_signal)]
    async fn init() {
        unsafe {
            USER = msg::source();
        }
    }

    #[gstd::async_main]
    async fn main() {
        loop {}
    }

    fn my_handle_reply() {
        unsafe {
            msg::send_bytes(USER, b"my_handle_reply", 0).unwrap();
        }
    }

    fn my_handle_signal() {
        unsafe {
            msg::send_bytes(USER, b"my_handle_signal", 0).unwrap();
        }
    }
}
