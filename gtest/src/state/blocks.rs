// This file is part of Gear.

// Copyright (C) 2024-2025 Gear Technologies Inc.
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
use gear_common::{auxiliary::BlockNumber, storage::GetCallback};
use std::{
    cell::RefCell, rc::Rc, thread::LocalKey, time::{SystemTime, UNIX_EPOCH}
};

pub(super) type BlockInfoStorageInner = Rc<RefCell<Option<BlockInfo>>>;
thread_local! {
    /// Definition of the storage value storing block info (timestamp and height).
    pub(super) static BLOCK_INFO_STORAGE: BlockInfoStorageInner = Rc::new(RefCell::new(None));
}

fn storage() -> &'static LocalKey<BlockInfoStorageInner> {
    if super::overlay_enabled() {
        &super::BLOCK_INFO_STORAGE_OVERLAY
    } else {
        &BLOCK_INFO_STORAGE
    }
}

#[derive(Debug)]
pub(crate) struct BlocksManager {
    _unused: BlockInfoStorageInner,
}

impl BlocksManager {
    /// Create block info storage manager with a further initialization of the
    /// storage.
    pub(crate) fn new() -> Self {
        let unused = storage().with(|bi_rc| {
            let mut ref_mut = bi_rc.borrow_mut();
            if ref_mut.is_none() {
                let info = BlockInfo {
                    height: 0,
                    timestamp: now(),
                };

                *ref_mut = Some(info);
            }

            Rc::clone(bi_rc)
        });

        Self { _unused: unused }
    }

    /// Get current block info.
    pub(crate) fn get(&self) -> BlockInfo {
        storage().with(|bi_rc| {
            bi_rc
                .borrow()
                .as_ref()
                .copied()
                .expect("instance always initialized")
        })
    }

    /// Move blocks by one.
    pub(crate) fn next_block(&self) -> BlockInfo {
        self.move_blocks_by(1)
    }

    /// Adjusts blocks info by moving blocks by `amount`.
    pub(crate) fn move_blocks_by(&self, amount: u32) -> BlockInfo {
        storage().with(|bi_rc| {
            let mut bi_ref_mut = bi_rc.borrow_mut();
            let Some(block_info) = bi_ref_mut.as_mut() else {
                panic!("instance always initialized");
            };
            block_info.height += amount;
            let duration = BLOCK_DURATION_IN_MSECS.saturating_mul(amount as u64);
            block_info.timestamp += duration;

            *block_info
        })
    }
}

impl Default for BlocksManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for BlocksManager {
    fn drop(&mut self) {
        storage().with(|bi_rc| {
            if Rc::strong_count(bi_rc) == 2 {
                *bi_rc.borrow_mut() = None;
            }
        });
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_millis() as u64
}

/// Block number getter.
///
/// Used to get block number for auxiliary complex storage managers,
/// like auxiliary mailbox, waitlist and etc.
pub(crate) struct GetBlockNumberImpl;

impl GetCallback<BlockNumber> for GetBlockNumberImpl {
    fn call() -> BlockNumber {
        BlocksManager::new().get().height
    }
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
        BLOCK_INFO_STORAGE.with(|bi_rc| bi_rc.borrow().is_some());

        // Drop first instance and check whether data is removed.
        drop(first_instance);
        assert_eq!(second_instance.get().height, 2);

        second_instance.next_block();
        assert_eq!(second_instance.get().height, 3);
        assert!(BLOCK_INFO_STORAGE.with(|bi_rc| bi_rc.borrow().is_some()));

        drop(second_instance);
        assert!(BLOCK_INFO_STORAGE.with(|bi_rc| bi_rc.borrow().is_none()));
    }
}
