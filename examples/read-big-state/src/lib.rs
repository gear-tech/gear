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
mod wasm {
    use super::*;
    use gstd::{msg, prelude::*};

    static mut STATE: Option<State> = None;

    fn state_mut() -> &'static mut State {
        unsafe { STATE.get_or_insert_with(State::new) }
    }

    #[no_mangle]
    extern "C" fn handle() {
        let strings = msg::load().expect("Failed to load state");
        state_mut().insert(strings);
    }

    #[no_mangle]
    extern "C" fn state() {
        msg::reply(state_mut(), 0).expect("Error in reply of state");
    }
}
