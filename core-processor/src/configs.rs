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
use gear_backend_common::lazy_pages::LazyPagesWeights;
use gear_core::{
    costs::{CostPerPage, HostFnWeights},
    memory::{GranularityPage, WasmPage},
};
use gear_wasm_instrument::syscalls::SysCallName;

/// +_+_+
pub const TEST_MAX_PAGES_NUMBER: u16 = 512;

/// Contextual block information.
#[derive(Clone, Copy, Debug, Encode, Decode, Default)]
pub struct BlockInfo {
    /// Height.
    pub height: u32,
    /// Timestamp.
    pub timestamp: u64,
}

/// Memory/allocation config.
#[derive(Clone, Debug, Decode, Encode, Default)]
pub struct PageCosts {
    /// Cost per one [GranularityPage] `read` processing in lazy-pages,
    /// it does not include cost for loading page data from storage.
    pub lazy_pages_read: CostPerPage<GranularityPage>,
    /// Cost per one [GranularityPage] `write` processing in lazy-pages,
    /// it does not include cost for loading page data from storage.
    pub lazy_pages_write: CostPerPage<GranularityPage>,
    /// Cost per one [GranularityPage] `write after read` processing in lazy-pages,
    /// it does not include cost for loading page data from storage.
    pub lazy_pages_write_after_read: CostPerPage<GranularityPage>,
    /// Cost per one [GranularityPage] data loading from storage
    /// and moving it in program memory.
    pub load_page_data: CostPerPage<GranularityPage>,
    /// Cost per one [GranularityPage] processing write accesses after execution.
    /// Does not include cost for uploading page to storage.
    pub write_access_page: CostPerPage<GranularityPage>,
    /// Cost per one [GranularityPage] uploading data to storage.
    pub upload_page_data: CostPerPage<GranularityPage>,
    /// Cost per one [WasmPage] static page. Static pages can have static data,
    /// and executor must to move this data to static pages before execution.
    pub static_page: CostPerPage<WasmPage>,
    /// Cost per one [WasmPage] for memory growing.
    pub mem_grow: CostPerPage<WasmPage>,
}

impl PageCosts {
    /// +_+_+
    pub fn lazy_pages_weights(&self) -> LazyPagesWeights {
        LazyPagesWeights {
            read: self.lazy_pages_read.into(),
            write: self.lazy_pages_write.add(self.upload_page_data).into(),
            write_after_read: self
                .lazy_pages_write_after_read
                .add(self.upload_page_data)
                .into(),
            load_page_storage_data: self.load_page_data.into(),
        }
    }
    /// +_+_+
    pub fn new_for_tests() -> Self {
        let a = 1000.into();
        let b = 4000.into();
        Self {
            lazy_pages_read: a,
            lazy_pages_write: a,
            lazy_pages_write_after_read: a,
            load_page_data: a,
            write_access_page: a,
            upload_page_data: a,
            static_page: b,
            mem_grow: b,
        }
    }
}

/// Execution settings for handling messages.
pub struct ExecutionSettings {
    /// Contextual block information.
    pub block_info: BlockInfo,
    /// Max amount of pages in program memory during execution.
    pub max_pages: WasmPage,
    /// Pages costs.
    pub page_costs: PageCosts,
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
    // TODO: find a way to put a random seed inside block config.
    pub random_data: (Vec<u8>, u32),
}

/// Stable parameters for the whole block across processing runs.
#[derive(Clone)]
pub struct BlockConfig {
    /// Block info.
    pub block_info: BlockInfo,
    /// +_+_+.
    pub max_pages: WasmPage,
    /// Allocations config.
    pub page_costs: PageCosts,
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
    /// WASM code instrumentation base cost.
    pub code_instrumentation_cost: u64,
    /// WASM code instrumentation per-byte cost.
    pub code_instrumentation_byte_cost: u64,
}
