// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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
use crate::{mock::*, offchain::PayeeInfo, QueueOf, WaitlistOf};
use codec::Decode;
use common::{self, storage::*, GasTree, Origin as _};
use core::convert::TryInto;
use frame_support::{assert_ok, traits::ReservableCurrency, unsigned::TransactionSource};
use frame_system::Pallet as SystemPallet;
use gear_core::{
    ids::{MessageId, ProgramId},
    message::{DispatchKind, StoredDispatch, StoredMessage},
};
use pallet_gear::GasHandlerOf;
use pallet_gear_program::Pallet as ProgramPallet;
use sp_runtime::{
    offchain::{
        storage_lock::{StorageLock, Time},
        Duration,
    },
    traits::{UniqueSaturatedInto, ValidateUnsigned},
};

pub(crate) type BlockNumberOf<T> = <<T as Config>::Messenger as Messenger>::BlockNumber;

pub(crate) fn init_logger() {
    let _ = env_logger::Builder::from_default_env()
        .format_module_path(false)
        .format_level(true)
        .try_init();
}

// Emulates specified block numbers of insertion.
fn insert_waiting_message<T>(dispatch: StoredDispatch, block_number: u64)
where
    T: crate::Config,
    T::AccountId: common::Origin,
{
    let current_bn = SystemPallet::<T>::block_number();
    let block_number: u32 = block_number.try_into().unwrap();

    SystemPallet::<T>::set_block_number(T::BlockNumber::from(block_number));

    WaitlistOf::<T>::insert(dispatch).expect("Failed to insert message in wl");

    SystemPallet::<T>::set_block_number(current_bn);
}

fn populate_wait_list<T>(n: u64, bn: u32, num_users: u64, gas_limits: Vec<u64>)
where
    T: crate::Config,
    T::AccountId: common::Origin,
{
    for i in 0_u64..n {
        let prog_id = (i + 1).into();
        let msg_id = (100_u64 * n + i + 1).into();
        let blk_num = i % (bn as u64) + 1;
        let user_id = (i % num_users + 1).into();
        let gas_limit = gas_limits[i as usize];
        insert_waiting_message::<T>(
            StoredDispatch::new(
                DispatchKind::Handle,
                StoredMessage::new(msg_id, user_id, prog_id, Default::default(), 0, None),
                None,
            ),
            blk_num,
        );

        let origin = <u64 as common::Origin>::from_origin(user_id.into_origin());
        let _ =
            GasHandlerOf::<Test>::create(origin, msg_id, gas_limit);
    }
}

fn populate_wait_list_with_split<T, I>(n: u64, bn: u32, user_id: I, gas_limit: u64)
where
    T: crate::Config,
    T::AccountId: common::Origin,
    I: Into<ProgramId> + Copy,
{
    let mut last_msg_id: Option<MessageId> = None;
    let user_id = user_id.into();
    for i in 0_u64..n {
        let prog_id = (i + 1).into();
        let msg_id = (100_u64 * n + i + 1).into();
        let blk_num = i % (bn as u64) + 1;
        let message = StoredMessage::new(msg_id, user_id, prog_id, Default::default(), 0, None);
        insert_waiting_message::<T>(
            StoredDispatch::new(DispatchKind::Handle, message, None),
            blk_num,
        );
        if let Some(last_msg_id) = last_msg_id {
            let _ = GasHandlerOf::<Test>::split(last_msg_id, msg_id);
        } else {
            let origin = <u64 as common::Origin>::from_origin(user_id.into_origin());
            let _ = GasHandlerOf::<Test>::create(
                origin,
                msg_id,
                gas_limit,
            );
        }
        last_msg_id = Some(msg_id);
    }
}

fn wait_list_contents<T>() -> Vec<(StoredDispatch, u32)>
where
    T: crate::Config,
    T::AccountId: common::Origin,
    BlockNumberOf<T>: TryInto<u32>,
{
    WaitlistOf::<T>::iter()
        .map(|(d, bn)| (d, bn.unique_saturated_into()))
        .collect()
}

fn decode_validate_usage_call(encoded: &[u8]) -> crate::Call<Test> {
    let mut encoded = <&[u8]>::clone(&encoded);
    let extrinsic: Extrinsic = Decode::decode(&mut encoded).unwrap();

    let inner = match extrinsic.call {
        mock::Call::Usage(inner) => inner,
        _ => unreachable!(),
    };

    assert!(
        <Usage as ValidateUnsigned>::validate_unsigned(TransactionSource::Local, &inner,).is_ok()
    );

    inner
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

        // Populate wait list with `Test::MaxBatchSize` x `num_batches` messages
        let num_entries = <Test as Config>::MaxBatchSize::get()
            .saturating_mul(num_batches)
            .saturating_sub(1) as u64;
        assert_eq!(num_entries, 29);
        populate_wait_list::<Test>(num_entries, 10, 1, vec![10_000; num_entries as usize]);

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
            Some(Some(MessageId::from(2910)))
        );

        run_to_block_with_ocw(16);
        assert_eq!(pool.read().transactions.len(), 4);

        // Last seen key in the wait list should be that of the 20th message
        assert_eq!(
            get_offchain_storage_value(offchain::STORAGE_LAST_KEY),
            Some(Some(MessageId::from(2920)))
        );

        run_to_block_with_ocw(17);
        assert_eq!(pool.read().transactions.len(), 5);
        // The wait list should have been completely exhausted at this moment,
        // the last key points at the wait list storage prefix
        assert_eq!(
            get_offchain_storage_value(offchain::STORAGE_LAST_KEY),
            Some(Option::<MessageId>::None)
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
fn ocw_double_charge() {
    init_logger();
    let (mut ext, pool) = with_offchain_ext();
    ext.execute_with(|| {
        // Reserve some currency on users' accounts
        for i in 1..=10 {
            assert_ok!(<Balances as ReservableCurrency<_>>::reserve(&i, 20_000));
        }

        let block_author_balance = Balances::free_balance(&BLOCK_AUTHOR);

        // Assert the tx pool is empty
        assert_eq!(pool.read().transactions.len(), 0);

        // Pretend the network has been up for a while
        run_to_block(10);

        // Expected number of batches needed to scan the entire wait list
        let num_batches = 1u32;

        // Populate wait list with `Test::MaxBatchSize` x `num_batches` messages
        let num_entries = <Test as Config>::MaxBatchSize::get()
            .saturating_mul(num_batches)
            .saturating_sub(1) as u64;
        assert_eq!(num_entries, 9);
        populate_wait_list::<Test>(num_entries, 10, 1, vec![10_000; num_entries as usize]);

        run_to_block_with_ocw(14);

        assert_eq!(pool.read().transactions.len(), 1);

        let unsigned = decode_validate_usage_call(&pool.read().transactions[0]);
        let payees_list = match unsigned {
            crate::Call::collect_waitlist_rent { payees_list } => payees_list,
            _ => unreachable!(),
        };

        // emulate malicious validator: send one more unsigned extrinsic with the same argument
        assert_ok!(Usage::send_transaction(payees_list.clone()));

        assert_eq!(pool.read().transactions.len(), 2);

        run_to_block_with_ocw(15);

        assert_eq!(pool.read().transactions.len(), 2);

        // calling the unsigned version of the extrinsic
        assert_ok!(Usage::collect_waitlist_rent(
            Origin::none(),
            payees_list.clone()
        ));

        let expected_balance = block_author_balance + 9_000;
        assert_eq!(Balances::reserved_balance(&1), 11_000);
        assert_eq!(Balances::free_balance(&BLOCK_AUTHOR), expected_balance);

        // call the unsigned extrinsic second time
        assert_ok!(Usage::collect_waitlist_rent(Origin::none(), payees_list));

        // check that there is no double charging
        assert_eq!(Balances::reserved_balance(&1), 11_000);
        assert_eq!(Balances::free_balance(&BLOCK_AUTHOR), expected_balance);
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

        // Populate wait list with `Test::MaxBatchSize` x `num_batches` messages
        let batch_size = <Test as Config>::MaxBatchSize::get() as u64;
        let num_entries = batch_size.saturating_mul(num_batches as u64);
        assert_eq!(num_entries, 70);
        populate_wait_list::<Test>(num_entries, 10, 1, vec![10_000; num_entries as usize]);

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

        // New "invoicing" round has started
        assert_eq!(
            get_offchain_storage_value(offchain::STORAGE_ROUND_STARTED_AT),
            Some(15_u32)
        );

        // The last tx in the pool contains batch #1 of messages
        let latest_tx = decode_txs(pool.clone())[..]
            .last()
            .expect("Checked above")
            .clone();
        assert!(matches!(
            latest_tx.call,
            mock::Call::Usage(pallet::Call::collect_waitlist_rent { .. })
        ));
        if let mock::Call::Usage(pallet::Call::collect_waitlist_rent { payees_list }) =
            latest_tx.call
        {
            for (
                i,
                PayeeInfo {
                    program_id,
                    message_id,
                },
            ) in payees_list.into_iter().enumerate()
            {
                let i = i as u64;
                assert_eq!(
                    (program_id, message_id),
                    (
                        (i + 1).into_origin(),
                        (100_u64 * (num_entries as u64) + i + 1).into_origin()
                    )
                );
            }
        } else {
            unreachable!()
        }

        // Last seen key in the wait list should be that of the 10th message
        assert_eq!(
            get_offchain_storage_value(offchain::STORAGE_LAST_KEY),
            Some(Some(MessageId::from(7010)))
        );

        run_to_block_with_ocw(16);
        assert_eq!(pool.read().transactions.len(), 4);

        // The last tx in the pool now contains batch #2 of messages
        let latest_tx = decode_txs(pool.clone())[..]
            .last()
            .expect("Checked above")
            .clone();
        assert!(matches!(
            latest_tx.call,
            mock::Call::Usage(pallet::Call::collect_waitlist_rent { .. })
        ));
        if let mock::Call::Usage(pallet::Call::collect_waitlist_rent { payees_list }) =
            latest_tx.call
        {
            for (
                i,
                PayeeInfo {
                    program_id,
                    message_id,
                },
            ) in payees_list.into_iter().enumerate()
            {
                let i = i as u64;
                assert_eq!(
                    (program_id, message_id),
                    (
                        (batch_size + i + 1).into_origin(),
                        (100_u64 * (num_entries as u64) + batch_size + i + 1).into_origin()
                    )
                );
            }
        } else {
            unreachable!()
        }

        // Last seen key in the wait list should be that of the 20th message
        assert_eq!(
            get_offchain_storage_value(offchain::STORAGE_LAST_KEY),
            Some(Some(MessageId::from(7020)))
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
            Some(Some(MessageId::from(7070)))
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
            Some(Option::<MessageId>::None)
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
        populate_wait_list::<Test>(10, 10, 10, vec![10_000; 10]);

        let wl = wait_list_contents::<Test>()
            .into_iter()
            .map(|(d, n)| (d.into_parts().1, n))
            .collect::<Vec<_>>();

        assert_eq!(wl.len(), 10);
        assert_eq!(wl[0].0.id(), 1001.into());
        assert_eq!(wl[9].0.id(), 1010.into());

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
        let wl = wait_list_contents::<Test>()
            .into_iter()
            .map(|(d, n)| (d.into_parts().1, n))
            .collect::<Vec<_>>();

        // current block number
        assert_eq!(wl[0].1, 15);
        assert_eq!(wl[4].1, 15);
        // initial block number
        assert_eq!(wl[5].1, 6);
        assert_eq!(wl[9].1, 10);

        // Check that the collected rent adds up
        assert_eq!(Balances::free_balance(&BLOCK_AUTHOR), 6101);
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
        populate_wait_list::<Test>(2, 10, 2, vec![1_100, 500]);

        let wl = wait_list_contents::<Test>();
        assert_eq!(wl.len(), 2);

        // Insert respective programs to the program storage
        let _program_1 = crate::mock::set_program::<Test>(1.into(), [0; 32].into(), 10);
        let _program_2 = crate::mock::set_program::<Test>(2.into(), [0; 32].into(), 10);

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

        // The first message still was charged the amount in excess
        assert_eq!(Balances::reserved_balance(&1), 1_000);

        // The second message wasn't charged at all before emitting trap reply
        assert_eq!(Balances::reserved_balance(&2), 500);

        // Ensure there are two trap reply messages in the message queue
        let message = QueueOf::<Test>::dequeue()
            .expect("Storage corrupted")
            .expect("No messages in queue");

        assert_eq!(message.source(), 1.into());
        assert_eq!(message.destination(), 1.into());
        assert_eq!(
            message.reply(),
            Some((201.into(), core_processor::ERR_EXIT_CODE))
        );
        // Check that respective `ValueNode` have been created by splitting the parent node
        assert_eq!(
            GasHandlerOf::<Test>::get_limit(message.id())
                .unwrap()
                .map(|(g, _)| g)
                .unwrap(),
            1000
        );

        // Second trap reply message
        let message = QueueOf::<Test>::dequeue()
            .expect("Storage corrupted")
            .expect("No messages in queue");

        assert_eq!(message.source(), 2.into());
        assert_eq!(message.destination(), 2.into());
        assert_eq!(
            message.reply(),
            Some((202.into(), core_processor::ERR_EXIT_CODE))
        );

        assert_eq!(
            GasHandlerOf::<Test>::get_limit(message.id())
                .unwrap()
                .map(|(g, _)| g)
                .unwrap(),
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
        populate_wait_list::<Test>(5, 10, 5, vec![10_000; 10]);

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
        assert_eq!(Balances::free_balance(&BLOCK_AUTHOR), 5501);
    });
}

#[test]
fn dust_discarded_with_noop() {
    init_logger();
    new_test_ext().execute_with(|| {
        // Message sender has reserved just above `T::TrapReplyExistentialGasLimit`
        assert_ok!(<Balances as ReservableCurrency<_>>::reserve(&1, 1_100));

        run_to_block(10);

        // Populate wait list with 2 messages
        populate_wait_list::<Test>(1, 10, 1, vec![1_100]);

        let wl = wait_list_contents::<Test>();
        assert_eq!(wl.len(), 1);

        run_to_block(15);

        // Calling the unsigned version of the extrinsic
        assert_ok!(Usage::collect_waitlist_rent(
            Origin::signed(11), // AccountId without any balance
            vec![PayeeInfo {
                program_id: 1.into_origin(),
                message_id: 101.into_origin()
            },],
        ));

        // The amount destined to the tx submitter is below `ExistentialDeposit`
        // It should have been removed as dust, no change in the beneficiary free balance
        assert_eq!(Balances::free_balance(&11), 0);
    });
}

#[test]
fn gas_properly_handled_for_trap_replies() {
    init_logger();
    new_test_ext().execute_with(|| {
        // 3st user has just above `T::TrapReplyExistentialGasLimit` reserved
        assert_ok!(<Balances as ReservableCurrency<_>>::reserve(&3, 1_100));
        // 4nd user already has less than `T::TrapReplyExistentialGasLimit` reserved
        assert_ok!(<Balances as ReservableCurrency<_>>::reserve(&4, 500));

        run_to_block(10);

        // Populate wait list with 2 messages
        populate_wait_list_with_split::<Test, u64>(2, 10, 3, 1_100);

        let wl = wait_list_contents::<Test>();
        assert_eq!(wl.len(), 2);

        // Insert respective programs to the program storage
        let _program_1 = crate::mock::set_program::<Test>(1.into(), [0; 32].into(), 10);
        let _program_2 = crate::mock::set_program::<Test>(2.into(), [0; 32].into(), 10);

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

        // Both messages should have been removed from wait list
        assert_eq!(wait_list_contents::<Test>().len(), 0);

        assert!(!ProgramPallet::<Test>::program_exists(3.into()));
        assert!(!ProgramPallet::<Test>::program_exists(4.into()));

        // 100 gas spent for rent payment by 1st message, other gas unreserved, due to addition of message into mailbox.
        assert_eq!(GasHandlerOf::<Test>::total_supply(), 0);

        // Ensure the message sender has the funds unreserved
        assert_eq!(Balances::reserved_balance(&3), 0);
        assert_eq!(Balances::free_balance(&3), 999_900); // Initial 1_000_000 less 100 paid for rent
        assert_eq!(Balances::free_balance(&BLOCK_AUTHOR), 201); // Initial 101 + 100 charged for rent
    });
}
