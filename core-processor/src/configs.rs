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
    message::ContextSettings,
    pages::WasmPagesAmount,
};

pub use gear_wasm_instrument::syscalls::SyscallName;

/// Contextual block information.
#[derive(Clone, Copy, Debug, Default)]
pub struct BlockInfo {
    /// Height.
    pub height: u32,
    /// Timestamp.
    pub timestamp: u64,
}

/// Execution settings for handling messages.
pub struct ExecutionSettings {
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

impl ExecutionSettings {
    /// Creates execution settings from block configuration.
    pub fn from_block_config(block_config: &BlockConfig, random_data: (Vec<u8>, u32)) -> Self {
        Self {
            block_info: block_config.block_info,
            performance_multiplier: block_config.performance_multiplier,
            existential_deposit: block_config.existential_deposit,
            mailbox_threshold: block_config.mailbox_threshold,
            max_pages: block_config.max_pages,
            ext_costs: block_config.costs.ext.clone(),
            lazy_pages_costs: block_config.costs.lazy_pages.clone(),
            forbidden_funcs: block_config.forbidden_funcs.clone(),
            reserve_for: block_config.reserve_for,
            random_data,
            gas_multiplier: block_config.gas_multiplier,
        }
    }
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

impl BlockConfig {
    /// Creates message context settings from block configuration.
    ///
    /// Fee calculations:
    /// - Sending fee: double write cost for addition and removal from queue
    /// - Scheduled sending fee: double write cost for queue + double write cost for dispatch stash
    /// - Waiting fee: triple write cost for waitlist operations and reply handling
    /// - Waking fee: double write cost for waitlist removal and enqueueing
    /// - Reservation fee: double write cost for reservation operations
    pub fn context_settings(&self) -> ContextSettings {
        ContextSettings {
            sending_fee: self.costs.db.write.cost_for(2.into()),
            scheduled_sending_fee: self.costs.db.write.cost_for(4.into()),
            waiting_fee: self.costs.db.write.cost_for(3.into()),
            waking_fee: self.costs.db.write.cost_for(2.into()),
            reservation_fee: self.costs.db.write.cost_for(2.into()),
            outgoing_limit: self.outgoing_limit,
            outgoing_bytes_limit: self.outgoing_bytes_limit,
        }
    }
}
