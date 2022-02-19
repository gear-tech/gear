// This file is part of Gear.

// Copyright (C) 2021 Gear Technologies Inc.
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
// Every call to the auto_composer contract incremets the internal `ITER` counter.
// As soon as the counter reaches the `MAX_ITER`, the recursion stops.
// Effectively, this procedure executes a composition of `MAX_ITER` contracts `f`
// where the output of the previous call is fed to the input of the next call.

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "std")]
mod native {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use native::{WASM_BINARY, WASM_BINARY_BLOATY};

#[cfg(not(feature = "std"))]
mod wasm {
    extern crate alloc;

    use codec::Encode;
    use gstd::{debug, exec, msg};

    #[no_mangle]
    pub unsafe extern "C" fn handle() {
        let (x, y): (u64, u64) = msg::load().expect("Expected a pair of u64 numbers");
        let z: u64 = x.checked_mul(y).expect("Multiplication overflow");
        debug!(
            "[unchecked-multiplier::handle] Calculated {} x {} == {}",
            x, y, z
        );

        debug!(
            "[unchzecked-multiplier::handle] Before sending reply message, gas_available = {}",
            exec::gas_available()
        );
        msg::reply(z, exec::gas_available() - 50_000_000, 0);
    }

    #[no_mangle]
    pub unsafe extern "C" fn init() {
        msg::reply((), 0, 0);
        debug!("Program initialized");
    }
}
