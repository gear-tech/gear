// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;
use parity_scale_codec::{Decode, Encode};

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

#[derive(Encode, Decode)]
pub enum HandleMessage {
    MillerLoop {
        message: Vec<u8>,
        signatures: Vec<Vec<u8>>,
    },
    Exp,
}

#[derive(Encode, Decode)]
pub struct InitMessage {
    pub g2_gen: Vec<u8>,
    pub pub_keys: Vec<Vec<u8>>,
}

#[cfg(not(feature = "std"))]
mod wasm;
