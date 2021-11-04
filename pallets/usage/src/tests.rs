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

use super::*;
use crate::mock::*;
use common::{self, Message, Origin as _};
use core::convert::TryInto;
use frame_support::{assert_noop, assert_ok};
use hex_literal::hex;
use sp_runtime::{
    offchain::{
        storage_lock::{StorageLock, Time},
        Duration,
    },
    traits::SaturatedConversion,
};

pub(crate) fn init_logger() {
    let _ = env_logger::Builder::from_default_env()
        .format_module_path(false)
        .format_level(true)
        .try_init();
}

fn populate_wait_list(n: u64, bn: u32) {
    for i in 1_u64..=n {
        let prog_id = i.into_origin();
        let msg_id = (100_u64 * n + i).into_origin();
        let blk_num = (i - 1) % (bn as u64) + 1;
        common::insert_waiting_message(
            prog_id.clone(),
            msg_id.clone(),
            Message {
                id: msg_id,
                source: 0_u64.into_origin(),
                dest: prog_id,
                payload: vec![],
                gas_limit: 10_000_000_u64,
                value: 0_u128,
                reply: None,
            },
            blk_num.try_into().unwrap(),
        );
    }
}

#[test]
fn ocw_interval_maintained() {
    init_logger();
    let (mut ext, pool) = with_offchain_ext();
    ext.execute_with(|| {
        // Assert the tx pool is empty
        assert_eq!(pool.read().transactions.len(), 0);

        // Pretend the network has been up for a while
        run_to_block_with_ocw(10);

        // Expected number of batches needed to scan the entire wait list
        let num_batches = 3_u32;

        // Populate wait list with `Test::MaxBatchSize` x `num_bathces` messages
        let num_entries = <Test as Config>::MaxBatchSize::get()
            .saturating_mul(num_batches)
            .saturating_sub(1) as u64;
        assert_eq!(num_entries, 29);
        populate_wait_list(num_entries, 10_u32);

        // Assert the tx pool has exactly 2 extrinsics (one in each 5 blocks)
        assert_eq!(pool.read().transactions.len(), 2);

        run_to_block_with_ocw(14);

        // Next OCW will not run until block 15, hence num tx in pool remains unchanged
        assert_eq!(pool.read().transactions.len(), 2);
        // Ensure that the current "invoicing" round started at block 10
        assert_eq!(
            get_offchain_storage_value(offchain::STORAGE_ROUND_STARTED_AT),
            Some(10_u32)
        );

        // From block 15 on we expect to have a new transaction every block
        run_to_block_with_ocw(15);
        assert_eq!(pool.read().transactions.len(), 3);
        // New "invoicing" round has started, as well
        assert_eq!(
            get_offchain_storage_value(offchain::STORAGE_ROUND_STARTED_AT),
            Some(15_u32)
        );

        // Last seen key in the wait list should be that of the 10th message
        assert_eq!(
            get_offchain_storage_value(offchain::STORAGE_LAST_KEY),
            Some(common::wait_key(
                10_u64.into_origin(),
                2910_u64.into_origin()
            ))
        );

        run_to_block_with_ocw(16);
        assert_eq!(pool.read().transactions.len(), 4);

        // Last seen key in the wait list should be that of the 20th message
        assert_eq!(
            get_offchain_storage_value(offchain::STORAGE_LAST_KEY),
            Some(common::wait_key(
                20_u64.into_origin(),
                2920_u64.into_origin()
            ))
        );

        run_to_block_with_ocw(17);
        assert_eq!(pool.read().transactions.len(), 5);
        // The wait list should have been completely exhauseted at this moment,
        // the last key points at the wait list storage prefix
        assert_eq!(
            get_offchain_storage_value(offchain::STORAGE_LAST_KEY),
            Some(common::STORAGE_WAITLIST_PREFIX.to_vec())
        );

        // Expect to idle for 2 blocks
        run_to_block_with_ocw(19);
        assert_eq!(
            get_offchain_storage_value(offchain::STORAGE_ROUND_STARTED_AT),
            Some(15_u32)
        );
        assert_eq!(pool.read().transactions.len(), 5);

        // The whole cycle is starting anew at block 20
        run_to_block_with_ocw(20);
        // New invoicing round has started
        assert_eq!(
            get_offchain_storage_value(offchain::STORAGE_ROUND_STARTED_AT),
            Some(20_u32)
        );
        // Another transaction added to the pool
        assert_eq!(pool.read().transactions.len(), 6);
    })
}

#[test]
fn ocw_overlapping_prevented() {
    init_logger();
    let (mut ext, pool) = with_offchain_ext();
    ext.execute_with(|| {
        assert_eq!(pool.read().transactions.len(), 0);

        // Acquire the lock, as if another thread is mid-way
        let mut lock = StorageLock::<'_, Time>::with_deadline(
            offchain::STORAGE_OCW_LOCK,
            Duration::from_millis(10_000),
        );
        let _guard = lock.lock();

        // The OCW at block 5 will block until the lock expires after 10 seconds
        run_to_block_with_ocw(5);
        assert_eq!(pool.read().transactions.len(), 1);

        // The OCW at block 5 has run but it had to wait for the lock expiration
        let current_time = get_current_offchain_time();
        let elapsed_at_least = 10_000_u64;
        assert!(
            current_time > elapsed_at_least,
            "current_time = {}, elapsed_at_least = {}",
            current_time,
            elapsed_at_least
        );
    })
}

#[test]
fn ocw_interval_stretches_for_large_wait_list() {
    init_logger();
    let (mut ext, pool) = with_offchain_ext();
    ext.execute_with(|| {
        // Pretend the network has been up for a while
        run_to_block_with_ocw(10);

        // Expected number of batches needed to scan the entire wait list
        let num_batches = 7_u32;

        // Populate wait list with `Test::MaxBatchSize` x `num_bathces` messages
        let num_entries = <Test as Config>::MaxBatchSize::get().saturating_mul(num_batches) as u64;
        assert_eq!(num_entries, 70);
        populate_wait_list(num_entries, 10_u32);

        // Assert the tx pool has exactly 2 extrinsics (after each 5 blocks)
        assert_eq!(pool.read().transactions.len(), 2);

        run_to_block_with_ocw(14);

        // Now OCW will not run until block 15, hence num tx in pool remains unchanged
        assert_eq!(pool.read().transactions.len(), 2);
        // Ensure that the current "invoicing" round started at block 10
        assert_eq!(
            get_offchain_storage_value(offchain::STORAGE_ROUND_STARTED_AT),
            Some(10_u32)
        );

        // From block 15 on we expect to have a new transaction every block
        run_to_block_with_ocw(15);
        assert_eq!(pool.read().transactions.len(), 3);
        // New "invoicing" round has started, as well
        assert_eq!(
            get_offchain_storage_value(offchain::STORAGE_ROUND_STARTED_AT),
            Some(15_u32)
        );

        // Last seen key in the wait list should be that of the 10th message
        assert_eq!(
            get_offchain_storage_value(offchain::STORAGE_LAST_KEY),
            Some(common::wait_key(
                10_u64.into_origin(),
                7010_u64.into_origin()
            ))
        );

        run_to_block_with_ocw(16);
        assert_eq!(pool.read().transactions.len(), 4);

        // Last seen key in the wait list should be that of the 20th message
        assert_eq!(
            get_offchain_storage_value(offchain::STORAGE_LAST_KEY),
            Some(common::wait_key(
                20_u64.into_origin(),
                7020_u64.into_origin()
            ))
        );

        run_to_block_with_ocw(17);
        assert_eq!(pool.read().transactions.len(), 5);
        run_to_block_with_ocw(18);
        assert_eq!(pool.read().transactions.len(), 6);
        run_to_block_with_ocw(19);
        assert_eq!(pool.read().transactions.len(), 7);
        run_to_block_with_ocw(20);
        assert_eq!(pool.read().transactions.len(), 8);
        run_to_block_with_ocw(21);
        assert_eq!(pool.read().transactions.len(), 9);

        // The wait list should have been completely exhausted at this moment,
        // however, the last key should still be that of the last message (#70)
        assert_eq!(
            get_offchain_storage_value(offchain::STORAGE_LAST_KEY),
            Some(common::wait_key(
                70_u64.into_origin(),
                7070_u64.into_origin()
            ))
        );

        run_to_block_with_ocw(22);
        // We don't expect the current round counter to be reset as long as the
        // last key refers to some message (not the wait list prefix)
        assert_eq!(
            get_offchain_storage_value(offchain::STORAGE_ROUND_STARTED_AT),
            Some(15_u32)
        );
        // A new transactions still added, although the payload should be empty
        assert_eq!(pool.read().transactions.len(), 10);
        // The last key should now point at the wait list storage prefix
        assert_eq!(
            get_offchain_storage_value(offchain::STORAGE_LAST_KEY),
            Some(common::STORAGE_WAITLIST_PREFIX.to_vec())
        );

        // The whole cycle is starting anew (without any idling between the rounds)
        run_to_block_with_ocw(23);
        // New invoicing round has started
        assert_eq!(
            get_offchain_storage_value(offchain::STORAGE_ROUND_STARTED_AT),
            Some(23_u32)
        );
        // Another transaction added to the pool
        assert_eq!(pool.read().transactions.len(), 11);
    })
}
