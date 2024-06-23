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

//! Configurations.

use alloc::{collections::BTreeSet, vec::Vec};
use gear_core::{
    costs::{BlocksAmount, BytesAmount, CallsAmount, CostOf, SyscallCosts},
    pages::WasmPagesAmount,
};
use gear_lazy_pages_common::LazyPagesCosts;
use gear_wasm_instrument::syscalls::SyscallName;

/// Number of max pages number to use it in tests.
pub const TESTS_MAX_PAGES_NUMBER: u16 = 512;

/// Contextual block information.
#[derive(Clone, Copy, Debug, Default)]
pub struct BlockInfo {
    /// Height.
    pub height: u32,
    /// Timestamp.
    pub timestamp: u64,
}

/// Holding in storages rent costs.
#[derive(Debug, Default, Clone)]
pub struct RentCosts {
    /// Holding message in waitlist cost per block.
    pub waitlist: CostOf<BlocksAmount>,
    /// Holding message in dispatch stash cost per block.
    pub dispatch_stash: CostOf<BlocksAmount>,
    /// Holding reservation cost per block.
    pub reservation: CostOf<BlocksAmount>,
}

/// Execution externalities costs.
#[derive(Debug, Default, Clone)]
pub struct ExtCosts {
    /// Syscalls costs.
    pub syscalls: SyscallCosts,
    /// Rent costs.
    pub rent: RentCosts,
    /// Memory grow cost.
    pub mem_grow: CostOf<CallsAmount>,
    /// Memory grow per page cost.
    pub mem_grow_per_page: CostOf<WasmPagesAmount>,
}

/// Module instantiation costs.
#[derive(Debug, Default, Clone)]
pub struct InstantiationCosts {
    /// WASM module code section instantiation per byte cost.
    pub code_section_per_byte: CostOf<BytesAmount>,
    /// WASM module data section instantiation per byte cost.
    pub data_section_per_byte: CostOf<BytesAmount>,
    /// WASM module global section instantiation per byte cost.
    pub global_section_per_byte: CostOf<BytesAmount>,
    /// WASM module table section instantiation per byte cost.
    pub table_section_per_byte: CostOf<BytesAmount>,
    /// WASM module element section instantiation per byte cost.
    pub element_section_per_byte: CostOf<BytesAmount>,
    /// WASM module type section instantiation per byte cost.
    pub type_section_per_byte: CostOf<BytesAmount>,
}

/// Costs for message processing
#[derive(Clone, Debug, Default)]
pub struct ProcessCosts {
    /// Execution externalities costs.
    pub ext: ExtCosts,
    /// Lazy pages costs.
    pub lazy_pages: LazyPagesCosts,
    /// Storage read cost.
    pub read: CostOf<CallsAmount>,
    /// Storage read per byte cost.
    pub read_per_byte: CostOf<BytesAmount>,
    /// Storage write cost.
    pub write: CostOf<CallsAmount>,
    /// Code instrumentation cost.
    pub instrumentation: CostOf<CallsAmount>,
    /// Code instrumentation per byte cost.
    pub instrumentation_per_byte: CostOf<BytesAmount>,
    /// Module instantiation costs.
    pub instantiation_costs: InstantiationCosts,
}

/// Execution settings for handling messages.
pub(crate) struct ExecutionSettings {
    /// Contextual block information.
    pub block_info: BlockInfo,
    /// Performance multiplier.
    pub performance_multiplier: gsys::Percent,
    /// Execution externalities costs.
    pub ext_costs: ExtCosts,
    /// Lazy pages costs.
    pub lazy_pages_costs: LazyPagesCosts,
    /// Existential deposit.
    pub existential_deposit: u128,
    /// Mailbox threshold.
    pub mailbox_threshold: u64,
    /// Max allowed memory size.
    pub max_pages: WasmPagesAmount,
    /// Forbidden functions.
    pub forbidden_funcs: BTreeSet<SyscallName>,
    /// Reserve for parameter of scheduling.
    pub reserve_for: u32,
    /// Most recently determined random seed, along with the time in the past since when it was determinable by chain observers.
    // TODO: find a way to put a random seed inside block config.
    pub random_data: (Vec<u8>, u32),
    /// Gas multiplier.
    pub gas_multiplier: gsys::GasMultiplier,
}

/// Stable parameters for the whole block across processing runs.
#[derive(Clone)]
pub struct BlockConfig {
    /// Block info.
    pub block_info: BlockInfo,
    /// Performance multiplier.
    pub performance_multiplier: gsys::Percent,
    /// Forbidden functions.
    pub forbidden_funcs: BTreeSet<SyscallName>,
    /// Reserve for parameter of scheduling.
    pub reserve_for: u32,
    /// Gas multiplier.
    pub gas_multiplier: gsys::GasMultiplier,
    /// Program processing costs.
    pub costs: ProcessCosts,
    /// Existential deposit.
    pub existential_deposit: u128,
    /// Mailbox threshold.
    pub mailbox_threshold: u64,
    /// Amount of reservations can exist for 1 program.
    pub max_reservations: u64,
    /// Max allowed page numbers for wasm program.
    pub max_pages: WasmPagesAmount,
    /// Outgoing limit.
    pub outgoing_limit: u32,
    /// Outgoing bytes limit.
    pub outgoing_bytes_limit: u32,
}
