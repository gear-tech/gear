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

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

#[cfg(not(feature = "std"))]
mod wasm {
    use gstd::{debug, msg, prelude::*};

    static mut MESSAGE_LOG: Vec<String> = vec![];

    #[no_mangle]
    extern "C" fn handle() {
        let size = msg::load::<i32>().expect("Failed to load `i32`") as usize;

        let request = format!("Request: size = {size}");

        debug!("{request}");
        unsafe { MESSAGE_LOG.push(request) };

        let vec = vec![42u8; size];
        let last_idx = size - 1;

        debug!("vec.len() = {:?}", vec.len());
        debug!(
            "vec[{last_idx}]: {:p} -> {:#04x}",
            &vec[last_idx], vec[last_idx]
        );

        msg::reply(size as i32, 0).expect("Failed to send reply");

        // The test idea is to allocate two wasm pages and check this allocation,
        // so we must skip `v` destruction.
        core::mem::forget(vec);

        let requests_amount = unsafe { MESSAGE_LOG.len() };
        debug!("Total requests amount: {requests_amount}");
        unsafe {
            MESSAGE_LOG.iter().for_each(|log| debug!("{log}"));
        }
    }
}
