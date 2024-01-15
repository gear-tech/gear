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

//! Provide sp-sandbox support.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod env;
pub mod error;
mod funcs;
mod log;
pub mod memory;
#[cfg(any(feature = "mock", test))]
pub mod mock;
mod runtime;
mod state;

use gear_core::{
    env::Externalities,
    gas::{CountersOwner, GasAmount},
    memory::MemoryInterval,
};
use gear_lazy_pages_common::ProcessAccessError;

/// Extended externalities that can manage gas counters.
pub trait BackendExternalities: Externalities + CountersOwner {
    fn gas_amount(&self) -> GasAmount;

    /// Pre-process memory access if need.
    fn pre_process_memory_accesses(
        reads: &[MemoryInterval],
        writes: &[MemoryInterval],
        gas_counter: &mut u64,
    ) -> Result<(), ProcessAccessError>;
}

#[cfg(test)]
mod tests;
