// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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

//! Gear core.
//!
//! This library provides a runner for dealing with multiple little programs exchanging messages in a deterministic manner.
//! To be used primary in Gear Substrate node implementation, but it is not limited to that.
#![no_std]
#![warn(missing_docs)]
#![cfg_attr(feature = "strict", deny(warnings))]
#![doc(html_logo_url = "https://docs.gear.rs/logo.svg")]

/// Max memory size, which runtime can allocate at once.
/// Substrate allocator restrict allocations bigger then 512 wasm pages at once.
/// See more information about:
/// https://github.com/paritytech/substrate/blob/cc4d5cc8654d280f03a13421669ba03632e14aa7/client/allocator/src/freeing_bump.rs#L136-L149
/// https://github.com/paritytech/substrate/blob/cc4d5cc8654d280f03a13421669ba03632e14aa7/primitives/core/src/lib.rs#L385-L388
pub const RUNTIME_MAX_ALLOC_SIZE: usize = 512 * 0x10000;

extern crate alloc;

pub mod code;
pub mod costs;
pub mod env;
pub mod gas;
pub mod ids;
pub mod memory;
pub mod message;
pub mod program;
