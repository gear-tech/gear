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

/// Lazy-pages status.
/// By default in program initialization status is set as `Normal`.
/// If nothing bad happens in lazy-pages, then status remains to be `Normal`.
/// If gas limit exceed, then status is set as `GasLimitExceeded`, and lazy-pages
/// starts to skips all signals processing until the end of execution.
/// The same is for gas allowance exceed, except it sets status as `GasAllowanceExceed`.
/// In the end of execution this status is checked and if it's not `Normal` then
/// termination reason sets as `gas limit exceeded` or `gas allowance exceeded`, depending on status.
/// NOTE: `repr(i64)` is important to be able add additional fields, without old runtimes separate support logic.
#[derive(Debug, Clone, Copy, Encode, Decode)]
#[repr(i64)]
pub enum Status {
    /// Lazy-pages works in normal mode.
    Normal = 0_i64,
    /// Skips signals processing until the end of execution, set termination reason as `gas limit exceeded`.
    GasLimitExceeded,
    /// Skips signals processing until the end of execution, set termination reason as `gas allowance exceeded`.
    GasAllowanceExceeded,
}

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
