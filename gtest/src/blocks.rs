// This file is part of Gear.

// Copyright (C) 2024 Gear Technologies Inc.
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

//! Block timestamp and height management.

use core_processor::configs::BlockInfo;
use std::{
    cell::RefCell,
    time::{SystemTime, UNIX_EPOCH},
};

use crate::BLOCK_DURATION_IN_MSECS;

thread_local! {
    /// Definition of the storage value storing block info (timestamp and height).
    static BLOCK_INFO_STORAGE: RefCell<Option<BlockInfo>> = const { RefCell::new(None) };
}

/// Block info storage manager.
#[derive(Debug, Default)]
pub(crate) struct BlocksManager(());

impl BlocksManager {
    /// Create block info storage manager with a further initialization of the
    /// storage.
    pub(crate) fn new() -> Self {
        BLOCK_INFO_STORAGE.with_borrow_mut(|block_info| {
            let info = BlockInfo {
                height: 0,
                timestamp: now(),
            };

            block_info.replace(info);
        });

        Self(())
    }

    /// Get current block info.
    pub(crate) fn get(&self) -> BlockInfo {
        BLOCK_INFO_STORAGE.with_borrow(|cell| {
            cell.as_ref()
                .copied()
                .expect("must be initialized in a `BlocksManager::new`")
        })
    }

    /// Move blocks by one.
    pub(crate) fn next_block(&self) -> BlockInfo {
        self.move_blocks_by(1)
    }

    /// Adjusts blocks info by moving blocks by `amount`.
    pub(crate) fn move_blocks_by(&self, amount: u32) -> BlockInfo {
        BLOCK_INFO_STORAGE.with_borrow_mut(|block_info| {
            let Some(block_info) = block_info.as_mut() else {
                panic!("must initialized in a `BlocksManager::new`");
            };
            block_info.height += amount;
            let duration = BLOCK_DURATION_IN_MSECS.saturating_mul(amount as u64);
            block_info.timestamp += duration;

            *block_info
        })
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_millis() as u64
}
