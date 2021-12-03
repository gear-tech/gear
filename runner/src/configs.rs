// This file is part of Gear.

// Copyright (C) 2021 Gear Technologies Inc.
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

use codec::{Decode, Encode};

use gear_core::{gas::GasCounter, memory::PageNumber};

use alloc::collections::BTreeSet;

const MAX_PAGES: u32 = 512;
const INIT_COST: u64 = 5000;
const ALLOC_COST: u64 = 10000;
const MEM_GROW_COST: u64 = 10000;
const LOAD_PAGE_COST: u64 = 3000;

#[derive(Clone, Copy, Debug)]
pub struct BlockInfo {
    /// Current block height.
    pub height: u32,
    /// Current block timestamp in msecs since tne Unix epoch.
    pub timestamp: u64,
    /// Current block gas limit.
    pub gas_limit: u64,
}

impl BlockInfo {
    pub fn new(height: u32, timestamp: u64, gas_limit: u64) -> Self {
        Self {
            height,
            timestamp,
            gas_limit,
        }
    }
}

/// Runner configuration.
#[derive(Clone, Debug, Decode, Encode)]
struct AllocationsConfig {
    /// Total memory pages count.
    max_pages: PageNumber,
    /// Gas cost for init memory page.
    init_cost: u64,
    /// Gas cost for memory page allocation.
    alloc_cost: u64,
    /// Gas cost for memory grow
    mem_grow_cost: u64,
    /// Gas cost for loading memory page from program state.
    load_page_cost: u64,
}

impl AllocationsConfig {
    fn new() -> Self {
        Self {
            max_pages: MAX_PAGES.into(),
            init_cost: INIT_COST,
            alloc_cost: ALLOC_COST,
            mem_grow_cost: MEM_GROW_COST,
            load_page_cost: LOAD_PAGE_COST,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum EntryPoint {
    Init,
    Handle,
    HandleReply,
}

impl From<EntryPoint> for &'static str {
    fn from(entry_point: EntryPoint) -> &'static str {
        match entry_point {
            EntryPoint::Init => "init",
            EntryPoint::Handle => "handle",
            EntryPoint::HandleReply => "handle_reply",
        }
    }
}

pub struct RunningContext {
    block_info: BlockInfo,
    gas_counter: GasCounter,
    config: AllocationsConfig,
    allocations: BTreeSet<PageNumber>,
}

impl RunningContext {
    pub fn new(block_info: BlockInfo, allocations: BTreeSet<PageNumber>) -> Self {
        Self {
            block_info,
            allocations,
            config: AllocationsConfig::new(),
            gas_counter: GasCounter::new(block_info.gas_limit),
        }
    }

    pub fn allocations(&self) -> BTreeSet<PageNumber> {
        self.allocations.clone()
    }

    pub fn block_info(&self) -> BlockInfo {
        self.block_info
    }

    pub fn gas_counter(&mut self) -> &mut GasCounter {
        &mut self.gas_counter
    }

    pub fn max_pages(&self) -> PageNumber {
        self.config.max_pages
    }

    pub fn init_cost(&self) -> u64 {
        self.config.init_cost
    }

    pub fn alloc_cost(&self) -> u64 {
        self.config.alloc_cost
    }

    pub fn mem_grow_cost(&self) -> u64 {
        self.config.mem_grow_cost
    }

    pub fn load_page_cost(&self) -> u64 {
        self.config.load_page_cost
    }
}
