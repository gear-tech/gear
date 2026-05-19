// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

#![no_std]

use parity_scale_codec::{Decode, Encode};
use shared::PackageId;

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

#[cfg(not(feature = "std"))]
mod wasm;

/// Program methods.
#[derive(Debug, Encode, Decode)]
pub enum Method {
    Start {
        expected: u128,
        id: PackageId,
        src: [u8; 32],
    },
    Refuel(PackageId),
    Calculate(PackageId),
}
