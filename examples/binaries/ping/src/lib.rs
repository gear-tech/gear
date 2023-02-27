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

#![feature(alloc_error_handler)]

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

#[cfg(target_arch = "wasm32")]
extern crate galloc;

#[cfg(not(feature = "std"))]
mod wasm {
    use core::str;
    use galloc::prelude::vec;
    use gcore::msg;

    #[no_mangle]
    extern "C" fn handle() {
        let mut bytes = vec![0; msg::size()];
        msg::read(&mut bytes).unwrap();

        if let Ok(received_msg) = str::from_utf8(&bytes) {
            if received_msg == "PING" {
                let _ = msg::reply(b"PONG", 0);
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    #[alloc_error_handler]
    pub fn oom(_: core::alloc::Layout) -> ! {
        core::arch::wasm32::unreachable()
    }

    #[cfg(target_arch = "wasm32")]
    #[panic_handler]
    fn panic(_: &core::panic::PanicInfo) -> ! {
        core::arch::wasm32::unreachable();
    }
}
