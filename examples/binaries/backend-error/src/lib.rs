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
    extern crate gstd;
    use gsys::{HashWithValue, LengthWithHash};

    #[no_mangle]
    extern "C" fn init() {
        // Code below is copied and simplified from `gcore::msg::send`.
        let pid_value = HashWithValue {
            hash: [0; 32],
            value: 0,
        };

        let mut res: LengthWithHash = Default::default();

        // u32::MAX ptr + 42 len of the payload triggers error of payload read.
        unsafe {
            gsys::gr_send(
                pid_value.as_ptr(),
                u32::MAX as *const u8,
                42,
                0,
                res.as_mut_ptr(),
            )
        };

        assert!(res.length != 0)
    }
}
