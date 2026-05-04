// This file is part of Gear.

// Copyright (C) 2024-2025 Gear Technologies Inc.
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
