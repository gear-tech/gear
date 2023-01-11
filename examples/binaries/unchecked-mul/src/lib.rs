// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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

// Unchecked multiplication (overflow-prone) of two u64 numebrs.

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

#[cfg(not(feature = "std"))]
mod wasm {
    extern crate alloc;

    use gstd::{debug, exec, msg};

    #[no_mangle]
    extern "C" fn handle() {
        let (x, y): (u64, u64) = msg::load().expect("Expected a pair of u64 numbers");
        let z: u64 = x.checked_mul(y).expect("Multiplication overflow");
        debug!(
            "[unchecked-multiplier::handle] Calculated {} x {} == {}",
            x, y, z
        );

        msg::reply(z, 0).unwrap();
    }

    #[no_mangle]
    extern "C" fn init() {
        msg::reply_bytes([], 0).unwrap();
    }
}
