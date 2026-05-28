// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

#![no_std]
use parity_scale_codec::{Decode, Encode};

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

#[cfg(not(feature = "std"))]
mod wasm;

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
