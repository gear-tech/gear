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

//! Configurations.

use alloc::{collections::BTreeSet, vec::Vec};
use codec::{Decode, Encode};
use gear_core::{code, costs::HostFnWeights, memory::WasmPageNumber};
use gear_wasm_instrument::syscalls::SysCallName;

const INIT_COST: u64 = 5000;
const ALLOC_COST: u64 = 10000;
const MEM_GROW_COST: u64 = 10000;
const LOAD_PAGE_COST: u64 = 3000;

/// Contextual block information.
#[derive(Clone, Copy, Debug, Encode, Decode, Default)]
pub struct BlockInfo {
    /// Height.
    pub height: u32,
    /// Timestamp.
    pub timestamp: u64,
}

/// Memory/allocation config.
#[derive(Clone, Debug, Decode, Encode)]
pub struct AllocationsConfig {
    /// Max amount of pages.
    pub max_pages: WasmPageNumber,
    /// Cost of initial memory.
    pub init_cost: u64,
    /// Cost of allocating memory.
    pub alloc_cost: u64,
    /// Memory grow cost.
    pub mem_grow_cost: u64,
    /// Load page cost.
    pub load_page_cost: u64,
}

impl Default for AllocationsConfig {
    fn default() -> Self {
        Self {
            max_pages: WasmPageNumber(code::MAX_WASM_PAGE_COUNT),
            init_cost: INIT_COST,
            alloc_cost: ALLOC_COST,
            mem_grow_cost: MEM_GROW_COST,
            load_page_cost: LOAD_PAGE_COST,
        }
    }
}

/// Execution settings for handling messages.
pub struct ExecutionSettings {
    /// Contextual block information.
    pub block_info: BlockInfo,
    /// Allocation config.
    pub allocations_config: AllocationsConfig,
    /// Minimal amount of existence for account.
    pub existential_deposit: u128,
    /// Weights of host functions.
    pub host_fn_weights: HostFnWeights,
    /// Functions forbidden to be called.
    pub forbidden_funcs: BTreeSet<SysCallName>,
    /// Threshold for inserting into mailbox
    pub mailbox_threshold: u64,
    /// Cost for single block waitlist holding.
    pub waitlist_cost: u64,
    /// Reserve for parameter of scheduling.
    pub reserve_for: u32,
    /// Cost for reservation holding.
    pub reservation: u64,
    /// Most recently determined random seed, along with the time in the past since when it was determinable by chain observers.
    /// TODO: find a way to put a random seed inside block config.
    pub random_data: (Vec<u8>, u32),
}

impl ExecutionSettings {
    /// Max amount of pages.
    pub fn max_pages(&self) -> WasmPageNumber {
        self.allocations_config.max_pages
    }
}

/// Stable parameters for the whole block across processing runs.
#[derive(Clone)]
pub struct BlockConfig {
    /// Block info.
    pub block_info: BlockInfo,
    /// Allocations config.
    pub allocations_config: AllocationsConfig,
    /// Existential deposit.
    pub existential_deposit: u128,
    /// Outgoing limit.
    pub outgoing_limit: u32,
    /// Host function weights.
    pub host_fn_weights: HostFnWeights,
    /// Forbidden functions.
    pub forbidden_funcs: BTreeSet<SysCallName>,
    /// Mailbox threshold.
    pub mailbox_threshold: u64,
    /// Cost for single block waitlist holding.
    pub waitlist_cost: u64,
    /// Reserve for parameter of scheduling.
    pub reserve_for: u32,
    /// Cost for reservation holding.
    pub reservation: u64,
    /// One-time db-read cost.
    pub read_cost: u64,
    /// One-time db-write cost.
    pub write_cost: u64,
    /// Per written byte cost.
    pub write_per_byte_cost: u64,
    /// Per loaded byte cost.
    pub read_per_byte_cost: u64,
    /// WASM module instantiation byte cost.
    pub module_instantiation_byte_cost: u64,
    /// Amount of reservations can exist for 1 program.
    pub max_reservations: u64,
    /// WASM module instrumentation base cost.
    pub module_instrumentation_cost: u64,
    /// WASM module instrumentation byte cost.
    pub module_instrumentation_byte_cost: u64,
}
