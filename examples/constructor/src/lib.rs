// This file is part of Gear.

// Copyright (C) 2021-2024 Gear Technologies Inc.
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

#[cfg(not(feature = "wasm-wrapper"))]
mod wasm;

#[cfg(not(feature = "wasm-wrapper"))]
pub(crate) use wasm::DATA;

#[cfg(feature = "wasm-wrapper")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "wasm-wrapper")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

mod arg;
mod builder;
mod call;
mod scheme;

pub use arg::Arg;
pub use builder::Calls;
pub use call::Call;
pub use scheme::*;
