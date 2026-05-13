// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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
    costs::{ExtCosts, LazyPagesCosts, ProcessCosts},
    pages::WasmPagesAmount,
};

pub use gear_wasm_instrument::syscalls::SyscallName;
use parity_scale_codec::{Decode, Encode};

/// Contextual block information.
#[derive(Clone, Copy, Debug, Default, Encode, Decode)]
pub struct BlockInfo {
    /// Height.
    pub height: u32,
    /// Timestamp.
    pub timestamp: u64,
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
