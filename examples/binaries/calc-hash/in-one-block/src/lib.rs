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
use codec::{Decode, Encode};

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

/// Package with expected
#[derive(Encode, Decode)]
pub struct Package {
    /// Expected calculation times.
    pub expected: u128,
    /// Calculation package.
    pub package: shared::Package,
}

impl Package {
    /// New package with expected.
    pub fn new(expected: u128, src: [u8; 32]) -> Self {
        Self {
            expected,
            package: shared::Package::new(src),
        }
    }

    /// Deref `Package::calc`
    pub fn calc(&mut self) {
        self.package.calc();
    }

    /// Deref `Package::finished`
    ///
    /// Check if calculation is finished.
    pub fn finished(&self) -> bool {
        self.package.finished(self.expected)
    }

    /// The result of calculation.
    pub fn result(&self) -> [u8; 32] {
        self.package.result
    }
}
