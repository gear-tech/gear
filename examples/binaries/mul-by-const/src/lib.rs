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

// This contract recursively composes itself with another contract (the other contract
// being applied to the input data first): `c(f) = (c(f) . f) x`.
// Every call to the auto_composer contract increments the internal `ITER` counter.
// As soon as the counter reaches the `MAX_ITER`, the recursion stops.
// Effectively, this procedure executes a composition of `MAX_ITER` contracts `f`
// where the output of the previous call is fed to the input of the next call.

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

    use gstd::{debug, exec, msg, String};

    static mut DEBUG: DebugInfo = DebugInfo { me: String::new() };
    static mut STATE: State = State { intrinsic: 0 };

    struct DebugInfo {
        me: String,
    }

    struct State {
        intrinsic: u64,
    }

    impl State {
        fn new(value: u64) -> Self {
            Self { intrinsic: value }
        }

        unsafe fn unchecked_mul(&mut self, other: u64) -> u64 {
            let z: u64 = self
                .intrinsic
                .checked_mul(other)
                .expect("Multiplication overflow");
            debug!(
                "[0x{} mul_by_const::unchecked_mul] Calculated {} x {} == {}",
                DEBUG.me, self.intrinsic, other, z
            );
            z
        }
    }

    #[no_mangle]
    extern "C" fn handle() {
        let x: u64 = msg::load().expect("Expecting a u64 number");

        msg::reply(unsafe { STATE.unchecked_mul(x) }, 0).unwrap();
    }

    #[no_mangle]
    extern "C" fn init() {
        let val: u64 = msg::load().expect("Expecting a u64 number");
        unsafe {
            STATE = State::new(val);
            DEBUG = DebugInfo {
                me: hex::encode(exec::program_id()),
            };
        }
        msg::reply_bytes([], 0).unwrap();
        debug!(
            "[0x{} mul_by_const::init] Program initialized with input {}",
            unsafe { &DEBUG.me },
            val
        );
    }
}
