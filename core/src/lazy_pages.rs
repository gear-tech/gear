// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
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

//! Core logic for usage both in runtime and in lazy-pages native part.

use core::fmt::Debug;

use codec::{Decode, Encode};

/// Memory access error.
#[derive(Debug, Clone, Copy, Decode, Encode, derive_more::Display)]
pub enum AccessError {
    /// Given access addr + size overflows u32.
    #[display(fmt = "Access interval addr {_0:#x} + size {_1:#x} overflows u32::MAX")]
    AddrPlusSizeOverflow(u32, u32),
    /// Access is out of wasm memory.
    #[display(fmt = "Access is out of wasm wasm memory")]
    OutOfWasmMemoryAccess,
}
