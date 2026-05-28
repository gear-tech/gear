// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

use parity_scale_codec::{Decode, Encode};

#[derive(Debug, Encode, Decode)]
pub enum InitAction {
    None,
    Panic,
}

#[cfg(not(feature = "std"))]
mod wasm;
