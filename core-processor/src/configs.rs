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

use gear_core::memory::PageNumber;

const MAX_PAGES: u32 = 512;
const INIT_COST: u64 = 5000;
const ALLOC_COST: u64 = 10000;
const MEM_GROW_COST: u64 = 10000;
const LOAD_PAGE_COST: u64 = 3000;

#[derive(Clone, Copy, Debug, Encode, Decode, Default)]
pub struct BlockInfo {
    pub height: u32,
    pub timestamp: u64,
}

#[derive(Clone, Debug, Decode, Encode)]
pub struct AllocationsConfig {
    pub max_pages: PageNumber,
    pub init_cost: u64,
    pub alloc_cost: u64,
    pub mem_grow_cost: u64,
    pub load_page_cost: u64,
}

impl Default for AllocationsConfig {
    fn default() -> Self {
        Self {
            max_pages: MAX_PAGES.into(),
            init_cost: INIT_COST,
            alloc_cost: ALLOC_COST,
            mem_grow_cost: MEM_GROW_COST,
            load_page_cost: LOAD_PAGE_COST,
        }
    }
}

pub struct ExecutionSettings {
    pub block_info: BlockInfo,
    pub config: AllocationsConfig,
}

impl ExecutionSettings {
    pub fn new(block_info: BlockInfo) -> Self {
        Self {
            block_info,
            config: Default::default(),
        }
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
