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
use crate::{mock::*, offchain::PayeeInfo};
use codec::Decode;
use common::{self, DAGBasedLedger, Dispatch, Message, Origin as _};
use core::convert::TryInto;
use frame_support::{assert_ok, traits::ReservableCurrency};
use gear_core::message::DispatchKind;
use hex_literal::hex;
use sp_runtime::offchain::{
    storage_lock::{StorageLock, Time},
    Duration,
};

pub(crate) fn init_logger() {
    let _ = env_logger::Builder::from_default_env()
        .format_module_path(false)
        .format_level(true)
        .try_init();
}

fn populate_wait_list(n: u64, bn: u32, num_users: u64, gas_limits: Vec<u64>) {
    for i in 0_u64..n {
        let prog_id = (i + 1).into_origin();
        let msg_id = (100_u64 * n + i + 1).into_origin();
        let blk_num = i % (bn as u64) + 1;
        let user_id = i % num_users + 1;
        let gas_limit = gas_limits[i as usize];
        let message = Message {
            id: msg_id,
            source: user_id.into_origin(),
            dest: prog_id,
            payload: vec![],
            gas_limit: gas_limit,
            value: 0_u128,
            reply: None,
        };
        common::insert_waiting_message(
            prog_id.clone(),
            msg_id.clone(),
            Dispatch {
                kind: DispatchKind::Handle,
                message,
            },
            blk_num.try_into().unwrap(),
        );
        let _ = <Test as pallet_gear::Config>::GasHandler::create(
            user_id.into_origin(),
            msg_id,
            gas_limit,
        );
    }
}

fn wait_list_contents() -> Vec<(Dispatch, u32)> {
    frame_support::storage::PrefixIterator::<(Dispatch, u32)>::new(
        common::STORAGE_WAITLIST_PREFIX.to_vec(),
        common::STORAGE_WAITLIST_PREFIX.to_vec(),
        |_, mut value| {
            let decoded = <(Dispatch, u32)>::decode(&mut value)?;
            Ok(decoded)
        },
    )
    .collect()
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
        populate_wait_list(num_entries, 10, 1, vec![10_000; num_entries as usize]);

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
        // The wait list should have been completely exhausted at this moment,
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
        populate_wait_list(num_entries, 10, 1, vec![10_000; num_entries as usize]);

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

#[test]
fn rent_charge_works() {
    init_logger();
    new_test_ext().execute_with(|| {
        // Reserve some currency on users' accounts
        for i in 1_u64..=10 {
            assert_ok!(<Balances as ReservableCurrency<_>>::reserve(&i, 10_000));
        }

        run_to_block(10);

        // Populate wait list
        // We have 10 messages in the wait list submitted one at a time by different users
        populate_wait_list(10, 10, 10, vec![10_000; 10]);

        let wl = wait_list_contents()
            .into_iter()
            .map(|(d, n)| (d.message, n))
            .collect::<Vec<_>>();
        assert_eq!(wl.len(), 10);
        assert_eq!(wl[0].0.id, 1001.into_origin());
        assert_eq!(wl[9].0.id, 1010.into_origin());

        run_to_block(15);

        // Calling the unsigned version of the extrinsic
        assert_ok!(Usage::collect_waitlist_rent(
            Origin::none(),
            vec![
                PayeeInfo {
                    program_id: 1.into_origin(),
                    message_id: 1001.into_origin()
                },
                PayeeInfo {
                    program_id: 2.into_origin(),
                    message_id: 1002.into_origin()
                },
                PayeeInfo {
                    program_id: 3.into_origin(),
                    message_id: 1003.into_origin()
                },
                PayeeInfo {
                    program_id: 4.into_origin(),
                    message_id: 1004.into_origin()
                },
                PayeeInfo {
                    program_id: 5.into_origin(),
                    message_id: 1005.into_origin()
                },
            ],
        ));
        // The i-th message was placed in the wait list at i-th block. Therefore at block 15
        // the 1st message has stayed in the wait list for 14 blocks whereas the 5th message -
        // for 10 blocks. The rent is 100 units of gas per block. Expect the sender of the
        // 1st message to have paid 1400 gas (converted to currency units in 1:1 ratio
        // through to the sender of the 5th message who should have paid 1000 gas only.
        assert_eq!(Balances::reserved_balance(&1), 8_600);
        assert_eq!(Balances::reserved_balance(&2), 8_700);
        assert_eq!(Balances::reserved_balance(&3), 8_800);
        assert_eq!(Balances::reserved_balance(&4), 8_900);
        assert_eq!(Balances::reserved_balance(&5), 9_000);

        // The insertion block number has been reset for the first 5 messages
        let wl = wait_list_contents()
            .into_iter()
            .map(|(d, n)| (d.message, n))
            .collect::<Vec<_>>();
        // current block number
        assert_eq!(wl[0].1, 15);
        assert_eq!(wl[4].1, 15);
        // initial block number
        assert_eq!(wl[5].1, 6);
        assert_eq!(wl[9].1, 10);

        // Check that the collected rent adds up
        assert_eq!(Balances::free_balance(&BLOCK_AUTHOR), 6001);
    });
}

#[test]
fn trap_reply_message_is_sent() {
    init_logger();
    new_test_ext().execute_with(|| {
        // 1st user has just above `T::TrapReplyExistentialGasLimit` reserved
        assert_ok!(<Balances as ReservableCurrency<_>>::reserve(&1, 1_100));
        // 2nd user already has less than `T::TrapReplyExistentialGasLimit` reserved
        assert_ok!(<Balances as ReservableCurrency<_>>::reserve(&2, 500));

        run_to_block(10);

        // Populate wait list with 2 messages
        populate_wait_list(2, 10, 2, vec![1_100, 500]);

        let wl = wait_list_contents()
            .into_iter()
            .map(|(d, n)| (d.message, n))
            .collect::<Vec<_>>();
        assert_eq!(wl.len(), 2);
        assert_eq!(wl[0].0.gas_limit, 1_100_u64);
        assert_eq!(wl[1].0.gas_limit, 500_u64);

        // Insert respective programs to the program storage
        let program_1 = gear_core::program::Program::new(
            1.into(),
            hex!("0061736d01000000020f0103656e76066d656d6f7279020001").to_vec(),
        )
        .unwrap();
        common::native::set_program(program_1);
        let program_2 = gear_core::program::Program::new(
            2.into(),
            hex!["0061736d01000000020f0103656e76066d656d6f7279020001"].to_vec(),
        )
        .unwrap();
        common::native::set_program(program_2);

        run_to_block(15);

        // Calling the unsigned version of the extrinsic
        assert_ok!(Usage::collect_waitlist_rent(
            Origin::none(),
            vec![
                PayeeInfo {
                    program_id: 1.into_origin(),
                    message_id: 201.into_origin()
                },
                PayeeInfo {
                    program_id: 2.into_origin(),
                    message_id: 202.into_origin()
                },
            ],
        ));

        // The first message still was charge the amount in excess
        assert_eq!(Balances::reserved_balance(&1), 1_000);

        // The second message wasn't charged at all before emitting trap reply
        assert_eq!(Balances::reserved_balance(&2), 500);

        // Ensure there are two trap reply messages in the message queue
        let Dispatch { message, .. } = common::dequeue_dispatch().unwrap();
        assert_eq!(message.source, 1.into_origin());
        assert_eq!(message.dest, 1.into_origin());
        assert_eq!(message.gas_limit, 1000);
        assert_eq!(
            message.reply,
            Some((201.into_origin(), core_processor::ERR_EXIT_CODE))
        );
        // Check that respective `ValueNode` have been created by splitting the parent node
        assert_eq!(
            <Test as pallet_gear::Config>::GasHandler::get(message.id)
                .unwrap()
                .0,
            1000
        );

        // Second trap reply message
        let Dispatch { message, .. } = common::dequeue_dispatch().unwrap();
        assert_eq!(message.source, 2.into_origin());
        assert_eq!(message.dest, 2.into_origin());
        assert_eq!(message.gas_limit, 500);
        assert_eq!(
            message.reply,
            Some((202.into_origin(), core_processor::ERR_EXIT_CODE))
        );

        assert_eq!(
            <Test as pallet_gear::Config>::GasHandler::get(message.id)
                .unwrap()
                .0,
            500
        );
    });
}

#[test]
fn external_submitter_gets_rewarded() {
    init_logger();
    new_test_ext().execute_with(|| {
        // Reserve some currency on users' accounts
        for i in 1_u64..=5 {
            assert_ok!(<Balances as ReservableCurrency<_>>::reserve(&i, 10_000));
        }

        run_to_block(10);

        // Populate wait list
        populate_wait_list(5, 10, 5, vec![10_000; 10]);

        run_to_block(15);

        // Calling the signed extrinsic
        assert_ok!(Usage::collect_waitlist_rent(
            Origin::signed(10),
            vec![
                PayeeInfo {
                    program_id: 1.into_origin(),
                    message_id: 501.into_origin()
                },
                PayeeInfo {
                    program_id: 2.into_origin(),
                    message_id: 502.into_origin()
                },
                PayeeInfo {
                    program_id: 3.into_origin(),
                    message_id: 503.into_origin()
                },
                PayeeInfo {
                    program_id: 4.into_origin(),
                    message_id: 504.into_origin()
                },
                PayeeInfo {
                    program_id: 5.into_origin(),
                    message_id: 505.into_origin()
                },
            ],
        ));

        // Check that the collected rent adds up:
        // 10% goes to the external user, the rest - to a validator
        assert_eq!(Balances::free_balance(&10), 1_000_600);
        assert_eq!(Balances::free_balance(&BLOCK_AUTHOR), 5401);
    });
}
