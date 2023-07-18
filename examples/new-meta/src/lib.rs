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

#![no_std]

// Reexport of types.
pub use demo_meta_io::*;

// For wasm compilation.
#[cfg(not(feature = "std"))]
mod wasm;

// Empty exports for native usage as dependency in other crates for running clippy.
#[cfg(not(feature = "std"))]
mod exports {
    pub const WASM_BINARY: &[u8] = &[];
    pub const WASM_METADATA: &[u8] = &[];
    pub const META_EXPORTS_V1: &[&str] = &[];
    pub const META_WASM_V1: &[u8] = &[];
    pub const META_EXPORTS_V2: &[&str] = &[];
    pub const META_WASM_V2: &[u8] = &[];
    pub const META_EXPORTS_V3: &[&str] = &[];
    pub const META_WASM_V3: &[u8] = &[];
}

// Exports for native usage as dependency in other crates.
#[cfg(feature = "std")]
mod exports {
    mod code {
        include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
    }

    // Binary itself.
    pub use code::WASM_BINARY_OPT as WASM_BINARY;

    // Metadata of the binary, defining types and registry for JS.
    pub use code::WASM_METADATA;

    // First reading state functions implementation.
    pub use demo_meta_state_v1::{META_EXPORTS_V1, META_WASM_V1};

    // Second reading state functions implementation.
    pub use demo_meta_state_v2::{META_EXPORTS_V2, META_WASM_V2};

    // Third reading state functions implementation.
    pub use demo_meta_state_v3::{META_EXPORTS_V3, META_WASM_V3};
}

// Public exports.
pub use exports::*;
