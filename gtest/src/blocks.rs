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

use crate::BLOCK_DURATION_IN_MSECS;
use core_processor::configs::BlockInfo;
use once_cell::sync::Lazy;
use parking_lot::RwLock;
use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

type BlockInfoStorageInner = Arc<RwLock<Option<BlockInfo>>>;

static BLOCK_INFO_STORAGE: Lazy<BlockInfoStorageInner> = Lazy::new(|| Arc::new(RwLock::new(None)));

#[derive(Debug)]
pub(crate) struct BlocksManager {
    _unused: BlockInfoStorageInner,
}

impl BlocksManager {
    /// Create block info storage manager with a further initialization of the
    /// storage.
    pub(crate) fn new() -> Self {
        let mut bi = BLOCK_INFO_STORAGE.write();
        if bi.is_none() {
            let info = BlockInfo {
                height: 0,
                timestamp: now(),
            };

            *bi = Some(info);
        }

        Self {
            _unused: BLOCK_INFO_STORAGE.clone(),
        }
    }

    /// Get current block info.
    pub(crate) fn get(&self) -> BlockInfo {
        BLOCK_INFO_STORAGE
            .read()
            .clone()
            .expect("instance always initialized")
    }

    /// Move blocks by one.
    pub(crate) fn next_block(&self) -> BlockInfo {
        self.move_blocks_by(1)
    }

    /// Adjusts blocks info by moving blocks by `amount`.
    pub(crate) fn move_blocks_by(&self, amount: u32) -> BlockInfo {
        let mut bi_ref_mut = BLOCK_INFO_STORAGE.write();
        let Some(block_info) = bi_ref_mut.as_mut() else {
            panic!("instance always initialized");
        };
        block_info.height += amount;
        let duration = BLOCK_DURATION_IN_MSECS.saturating_mul(amount as u64);
        block_info.timestamp += duration;

        *block_info
    }
}

impl Default for BlocksManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for BlocksManager {
    fn drop(&mut self) {
        if Arc::strong_count(&BLOCK_INFO_STORAGE) == 2 {
            *BLOCK_INFO_STORAGE.write() = None;
        }
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_data_nullified_on_drop() {
        let first_instance = BlocksManager::new();
        let second_instance = BlocksManager::new();

        first_instance.next_block();
        first_instance.next_block();

        // Assert all instance use same data;
        assert_eq!(second_instance.get().height, 2);
        assert!(BLOCK_INFO_STORAGE.read().is_some());

        // Drop first instance and check whether data is removed.
        drop(first_instance);
        assert_eq!(second_instance.get().height, 2);

        second_instance.next_block();
        assert_eq!(second_instance.get().height, 3);
        assert!(BLOCK_INFO_STORAGE.read().is_some());

        drop(second_instance);
        assert!(BLOCK_INFO_STORAGE.read().is_none());
    }
}
