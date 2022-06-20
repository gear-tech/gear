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
#![no_std]

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

#[cfg(not(feature = "std"))]
mod wasm {
    include! {"./code.rs"}
}

use codec::{Decode, Encode};

/// Gas meter of pow.
pub struct GasMeter {
    ptr: u64,
    multiplier: u128,
}

impl GasMeter {
    /// New gas meter.
    pub fn new(ptr: u64, multiplier: u128) -> Self {
        Self { ptr, multiplier }
    }

    /// Update the gas avaiable and gas spent.
    pub fn spin(&mut self, ptr: u64) -> bool {
        let gas_spent = self.ptr - ptr;

        *self = Self {
            ptr,
            multiplier: self.multiplier,
        };

        if ptr as u128 > gas_spent as u128 * self.multiplier {
            true
        } else {
            false
        }
    }
}

#[derive(Debug, Encode, Decode)]
pub struct Package {
    /// Base number of pow.
    pub base: u128,
    /// Exponent of this calculation.
    pub exponent: u128,
    /// Current exponent.
    pub ptr: u128,
    /// The result of `pow(base, exponent)`.
    pub result: u128,
}

impl Package {
    pub fn calc(&mut self) {
        self.ptr += 1;
        self.result = self.base.saturating_mul(self.result);
    }

    pub fn finished(&self) -> bool {
        self.exponent == self.ptr
    }
}
