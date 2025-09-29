// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! gearexe common types and traits.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

mod crypto;
pub mod db;
pub mod events;
pub mod gear;
mod primitives;
pub mod tx_pool;
mod utils;

pub use crypto::*;
pub use gear_core;
pub use gprimitives;
pub use k256;
pub use primitives::*;
pub use sha3;
pub use utils::*;
