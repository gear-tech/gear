// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::{collections::BTreeMap, string::String, vec, vec::Vec};
use parity_scale_codec::{Decode, Encode};

#[cfg(feature = "wasm-wrapper")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "wasm-wrapper")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

#[derive(Encode, Decode, Default, Debug, Clone)]
pub struct Strings(pub Vec<String>);

impl Strings {
    pub const LEN: usize = 16;

    pub fn new(string: String) -> Self {
        Self(vec![string; Self::LEN])
    }
}

#[derive(Encode, Decode, Default, Debug, Clone)]
pub struct State(pub Vec<BTreeMap<u64, Strings>>);

impl State {
    pub const LEN: usize = 16;

    pub fn new() -> Self {
        Self(vec![Default::default(); Self::LEN])
    }

    pub fn insert(&mut self, strings: Strings) {
        for map in &mut self.0 {
            map.insert(map.keys().count() as u64, strings.clone());
        }
    }
}

#[cfg(not(feature = "wasm-wrapper"))]
mod wasm;
