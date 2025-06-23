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

use crate::{
    constants::BlockNumber, state::WithOverlay, BLOCK_DURATION_IN_MSECS, EPOCH_DURATION_IN_BLOCKS,
    INITIAL_RANDOM_SEED,
};
use core_processor::configs::BlockInfo;
use gear_common::storage::GetCallback;
use rand::{rngs::StdRng, RngCore, SeedableRng};
use std::{
    thread::LocalKey,
    time::{SystemTime, UNIX_EPOCH},
};

thread_local! {
    /// Definition of the storage value storing block info (timestamp and height).
    pub(super) static BLOCK_INFO_STORAGE: WithOverlay<BlockInfo> = WithOverlay::new(BlockInfo {
        height: 0,
        timestamp: now(),
    });
    pub(super) static CURRENT_EPOCH_RANDOM: WithOverlay<Vec<u8>> = WithOverlay::new(epoch_random(INITIAL_RANDOM_SEED));
}

fn block_info_storage() -> &'static LocalKey<WithOverlay<BlockInfo>> {
    &BLOCK_INFO_STORAGE
}

fn current_epoch_random_storage() -> &'static LocalKey<WithOverlay<Vec<u8>>> {
    &CURRENT_EPOCH_RANDOM
}

#[derive(Debug)]
pub(crate) struct BlocksManager;

impl BlocksManager {
    /// Get current block info.
    pub(crate) fn get(&self) -> BlockInfo {
        block_info_storage().with(|bi| {
            log::warn!("{bi:?}");
            *bi.data()
        })
    }

    /// Move blocks by one.
    pub(crate) fn next_block(&self) -> BlockInfo {
        let bi = self.move_blocks_by(1);

        let block_height = self.get().height;
        if block_height % EPOCH_DURATION_IN_BLOCKS == 0 {
            let seed = INITIAL_RANDOM_SEED + (block_height / EPOCH_DURATION_IN_BLOCKS) as u64;
            update_epoch_random(seed);
        }

        bi
    }

    /// Adjusts blocks info by moving blocks by `amount`.
    pub(crate) fn move_blocks_by(&self, amount: u32) -> BlockInfo {
        block_info_storage().with(|bi| {
            bi.data_mut().height += amount;
            let duration = BLOCK_DURATION_IN_MSECS.saturating_mul(amount as u64);
            bi.data_mut().timestamp += duration;

            *bi.data()
        })
    }

    pub(crate) fn reset(&self) {
        block_info_storage().with(|bi_rc| {
            *bi_rc.data_mut() = BlockInfo {
                height: 0,
                timestamp: now(),
            };
        });
    }
}

impl Default for BlocksManager {
    fn default() -> Self {
        Self
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
        BlocksManager.get().height
    }
}

pub(crate) fn current_epoch_random() -> Vec<u8> {
    current_epoch_random_storage().with(|random| random.data().clone())
}

pub(super) fn update_epoch_random(seed: u64) {
    current_epoch_random_storage().with(|random| {
        *random.data_mut() = epoch_random(seed);
    });
}

fn epoch_random(seed: u64) -> Vec<u8> {
    let mut rng = StdRng::seed_from_u64(seed);
    let mut random = [0u8; 32];
    rng.fill_bytes(&mut random);

    random.to_vec()
}
