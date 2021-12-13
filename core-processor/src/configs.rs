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

#[derive(Clone, Copy, Debug)]
pub struct BlockInfo {
    /// Current block height.
    pub height: u32,
    /// Current block timestamp in msecs since tne Unix epoch.
    pub timestamp: u64,
}

/// Runner configuration.
#[derive(Clone, Debug, Decode, Encode)]
pub struct AllocationsConfig {
    /// Total memory pages count.
    pub max_pages: PageNumber,
    /// Gas cost for init memory page.
    pub init_cost: u64,
    /// Gas cost for memory page allocation.
    pub alloc_cost: u64,
    /// Gas cost for memory grow
    pub mem_grow_cost: u64,
    /// Gas cost for loading memory page from program state.
    pub load_page_cost: u64,
}

impl AllocationsConfig {
    pub fn new() -> Self {
        Self {
            max_pages: MAX_PAGES.into(),
            init_cost: INIT_COST,
            alloc_cost: ALLOC_COST,
            mem_grow_cost: MEM_GROW_COST,
            load_page_cost: LOAD_PAGE_COST,
        }
    }
}

impl Default for AllocationsConfig {
    fn default() -> Self {
        Self::new()
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
