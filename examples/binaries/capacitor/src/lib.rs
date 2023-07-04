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

    static mut CHARGE: u32 = 0;
    static mut LIMIT: u32 = 0;
    static mut DISCHARGE_HISTORY: Vec<u32> = Vec::new();

    #[no_mangle]
    extern "C" fn init() {
        let initstr = String::from_utf8(msg::load_bytes().expect("Failed to load payload bytes"))
            .expect("Invalid message: should be utf-8");
        let limit = u32::from_str(initstr.as_ref()).expect("Invalid number");
        unsafe { LIMIT = limit };
        debug!("Init capacitor with limit capacity {limit}, {initstr}");
    }

    #[no_mangle]
    extern "C" fn handle() {
        let new_msg = String::from_utf8(msg::load_bytes().expect("Failed to load payload bytes"))
            .expect("Invalid message: should be utf-8");
        let to_add = u32::from_str(new_msg.as_ref()).expect("Invalid number");

        unsafe {
            CHARGE += to_add;
            debug!("Charge capacitor with {to_add}, new charge {CHARGE}");
            if CHARGE >= LIMIT {
                debug!("Discharge #{CHARGE} due to limit {LIMIT}");
                msg::send_bytes(msg::source(), format!("Discharged: {CHARGE}"), 0).unwrap();
                DISCHARGE_HISTORY.push(CHARGE);
                CHARGE = 0;
            }
        }
    }
}
