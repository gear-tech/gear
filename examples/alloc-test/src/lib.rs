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

//! A simple test program that triggers WASM page allocation growth.
//!
//! This program allocates memory on each handle call, forcing WASM memory
//! to grow beyond initial static pages. Used for testing that allocation
//! updates are properly tracked and propagated in sequential execution.

#![no_std]

extern crate alloc;

use parity_scale_codec::{Decode, Encode};

/// Action to perform.
#[derive(Debug, Encode, Decode)]
pub enum Action {
    /// Allocate `size` bytes and keep them allocated.
    Alloc(u32),
    /// Get current total allocated size.
    GetAllocatedSize,
}

/// Event emitted by the program.
#[derive(Debug, Encode, Decode, PartialEq, Eq)]
pub enum Event {
    /// Memory allocated successfully. Returns new total size.
    Allocated(u32),
    /// Current total allocated size.
    AllocatedSize(u32),
}

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

#[cfg(not(feature = "std"))]
mod wasm;
