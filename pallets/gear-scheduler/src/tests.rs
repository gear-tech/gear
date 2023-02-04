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

//! Unit tests module.

extern crate alloc;

use crate::{mock::*, *};
use alloc::string::ToString;
use common::{scheduler::*, storage::*, GasPrice as _, GasTree, Origin};
use frame_support::traits::ReservableCurrency;
use gear_core::{ids::*, message::*};
use gear_core_errors::{SimpleCodec, SimpleReplyError};
use pallet_gear::{GasAllowanceOf, GasHandlerOf};
use sp_core::H256;

type GasPrice = <Test as pallet_gear::Config>::GasPrice;
type WaitlistOf<T> = <<T as pallet_gear::Config>::Messenger as Messenger>::Waitlist;
type TaskPoolOf<T> = <<T as pallet_gear::Config>::Scheduler as Scheduler>::TaskPool;

pub(crate) fn init_logger() {
    let _ = env_logger::Builder::from_default_env()
        .format_module_path(false)
        .format_level(true)
        .try_init();
}

const DEFAULT_GAS: u64 = 1_000_000;

fn wl_cost_for(amount_of_blocks: u64) -> u128 {
    GasPrice::gas_price(<Pallet<Test> as Scheduler>::CostsPerBlock::waitlist() * amount_of_blocks)
}

fn dispatch_from(src: impl Into<ProgramId>) -> StoredDispatch {
    StoredDispatch::new(
        DispatchKind::Handle,
        StoredMessage::new(
            MessageId::from_origin(H256::random().into_origin()),
            src.into(),
            ProgramId::from_origin(H256::random().into_origin()),
            Default::default(),
            0,
            None,
        ),
        None,
    )
}

fn populate_wl_from(
    src: <Test as frame_system::Config>::AccountId,
    bn: <Test as frame_system::Config>::BlockNumber,
) -> (MessageId, ProgramId) {
    let dispatch = dispatch_from(src);
    let mid = dispatch.id();
    let pid = dispatch.destination();

    TaskPoolOf::<Test>::add(bn, ScheduledTask::RemoveFromWaitlist(pid, mid))
        .expect("Failed to insert task");
    WaitlistOf::<Test>::insert(dispatch, u64::MAX).expect("Failed to insert to waitlist");
    Balances::reserve(&src, GasPrice::gas_price(DEFAULT_GAS)).expect("Cannot reserve gas");
    GasHandlerOf::<Test>::create(src, mid, DEFAULT_GAS).expect("Failed to create gas handler");

    (mid, pid)
}

fn task_and_wl_message_exist(
    mid: impl Into<MessageId>,
    pid: impl Into<ProgramId>,
    bn: <Test as frame_system::Config>::BlockNumber,
) -> bool {
    let mid = mid.into();
    let pid = pid.into();

    let ts = TaskPoolOf::<Test>::contains(&bn, &ScheduledTask::RemoveFromWaitlist(pid, mid));
    let wl = WaitlistOf::<Test>::contains(&pid, &mid);

    if ts != wl {
        panic!("Logic invalidated");
    }

    ts
}

fn out_of_rent_reply_exists(
    user_id: <Test as frame_system::Config>::AccountId,
    mid: impl Into<MessageId>,
    pid: impl Into<ProgramId>,
) -> bool {
    let src = ProgramId::from_origin(user_id.into_origin());
    let mid = mid.into();
    let pid = pid.into();

    System::events().into_iter().any(|e| {
        if let mock::RuntimeEvent::Gear(pallet_gear::Event::UserMessageSent {
            message: msg,
            expiration: None,
        }) = &e.event
        {
            let err = SimpleReplyError::OutOfRent;
            msg.destination() == src
                && msg.source() == pid
                && msg.reply() == Some(ReplyDetails::new(mid, err.into_status_code()))
                && msg.payload() == err.to_string().as_bytes()
        } else {
            false
        }
    })
}

fn db_r_w(reads: u64, writes: u64) -> Weight {
    <Test as frame_system::Config>::DbWeight::get().reads_writes(reads, writes)
}

// Don't worry if this test fails in your PR.
// It's due to gas allowance checking in most cases.
// Read and understand what's going on here,
// updating gas allowance changes afterward.
#[test]
fn gear_handles_tasks() {
    init_logger();
    new_test_ext().execute_with(|| {
        // We start from block 2 for confidence.
        let initial_block = 2;
        run_to_block(initial_block, Some(u64::MAX));
        // Read of missed blocks.
        assert_eq!(
            GasAllowanceOf::<Test>::get(),
            u64::MAX - db_r_w(1, 0).ref_time()
        );

        // Block producer initial balance.
        let block_author_balance = Balances::free_balance(BLOCK_AUTHOR);
        assert_eq!(Balances::reserved_balance(BLOCK_AUTHOR), 0);

        // USER_1 initial balance.
        let user1_balance = Balances::free_balance(USER_1);
        assert_eq!(Balances::reserved_balance(USER_1), 0);

        // Appending task and message to wl.
        let bn = 5;
        let (mid, pid) = populate_wl_from(USER_1, bn);
        assert!(task_and_wl_message_exist(mid, pid, bn));
        assert!(!out_of_rent_reply_exists(USER_1, mid, pid));

        // Balance checking.
        assert_eq!(Balances::free_balance(BLOCK_AUTHOR), block_author_balance);
        assert_eq!(
            Balances::free_balance(USER_1),
            user1_balance - GasPrice::gas_price(DEFAULT_GAS)
        );
        assert_eq!(
            Balances::reserved_balance(USER_1),
            GasPrice::gas_price(DEFAULT_GAS)
        );

        // Check if task and message exist before start of block `bn`.
        run_to_block(bn - 1, Some(u64::MAX));
        // Read of missed blocks.
        assert_eq!(
            GasAllowanceOf::<Test>::get(),
            u64::MAX - db_r_w(1, 0).ref_time()
        );

        // Storages checking.
        assert!(task_and_wl_message_exist(mid, pid, bn));
        assert!(!out_of_rent_reply_exists(USER_1, mid, pid));

        // Balance checking.
        assert_eq!(Balances::free_balance(BLOCK_AUTHOR), block_author_balance);
        assert_eq!(
            Balances::free_balance(USER_1),
            user1_balance - GasPrice::gas_price(DEFAULT_GAS)
        );
        assert_eq!(
            Balances::reserved_balance(USER_1),
            GasPrice::gas_price(DEFAULT_GAS)
        );

        // Check if task and message got processed in block `bn`.
        run_to_block(bn, Some(u64::MAX));
        // Read of missed blocks and write for removal of task.
        assert_eq!(
            GasAllowanceOf::<Test>::get(),
            u64::MAX - db_r_w(1, 1).ref_time()
        );

        // Storages checking.
        assert!(!task_and_wl_message_exist(mid, pid, bn));
        assert!(out_of_rent_reply_exists(USER_1, mid, pid));

        // Balance checking.
        let cost = wl_cost_for(bn - initial_block); // Diff of blocks of insertion and removal.
        assert_eq!(
            Balances::free_balance(BLOCK_AUTHOR),
            block_author_balance + cost
        );
        assert_eq!(Balances::free_balance(USER_1), user1_balance - cost);
        assert_eq!(Balances::reserved_balance(USER_1), 0);
    });
}

// Don't worry if this test fails in your PR.
// It's due to gas allowance checking in most cases.
// Read and understand what's going on here,
// updating gas allowance changes afterward.
#[test]
fn gear_handles_outdated_tasks() {
    init_logger();
    new_test_ext().execute_with(|| {
        // We start from block 2 for confidence.
        let initial_block = 2;
        run_to_block(initial_block, Some(u64::MAX));
        // Read of missed blocks.
        assert_eq!(
            GasAllowanceOf::<Test>::get(),
            u64::MAX - db_r_w(1, 0).ref_time()
        );

        // Block producer initial balance.
        let block_author_balance = Balances::free_balance(BLOCK_AUTHOR);
        assert_eq!(Balances::reserved_balance(BLOCK_AUTHOR), 0);

        // USER_1 initial balance.
        let user1_balance = Balances::free_balance(USER_1);
        assert_eq!(Balances::reserved_balance(USER_1), 0);

        // USER_2 initial balance.
        let user2_balance = Balances::free_balance(USER_2);
        assert_eq!(Balances::reserved_balance(USER_2), 0);

        // Appending twice task and message to wl.
        let bn = 5;
        let (mid1, pid1) = populate_wl_from(USER_1, bn);
        let (mid2, pid2) = populate_wl_from(USER_2, bn);
        assert!(task_and_wl_message_exist(mid1, pid1, bn));
        assert!(task_and_wl_message_exist(mid2, pid2, bn));
        assert!(!out_of_rent_reply_exists(USER_1, mid1, pid1));
        assert!(!out_of_rent_reply_exists(USER_2, mid2, pid2));

        // Balance checking.
        assert_eq!(Balances::free_balance(BLOCK_AUTHOR), block_author_balance);
        assert_eq!(
            Balances::free_balance(USER_1),
            user1_balance - GasPrice::gas_price(DEFAULT_GAS)
        );
        assert_eq!(
            Balances::reserved_balance(USER_1),
            GasPrice::gas_price(DEFAULT_GAS)
        );
        assert_eq!(
            Balances::free_balance(USER_2),
            user2_balance - GasPrice::gas_price(DEFAULT_GAS)
        );
        assert_eq!(
            Balances::reserved_balance(USER_2),
            GasPrice::gas_price(DEFAULT_GAS)
        );

        // Check if tasks and messages exist before start of block `bn`.
        run_to_block(bn - 1, Some(u64::MAX));
        assert_eq!(
            GasAllowanceOf::<Test>::get(),
            u64::MAX - db_r_w(1, 0).ref_time()
        );

        // Storages checking.
        assert!(task_and_wl_message_exist(mid1, pid1, bn));
        assert!(task_and_wl_message_exist(mid2, pid2, bn));
        assert!(!out_of_rent_reply_exists(USER_1, mid1, pid1));
        assert!(!out_of_rent_reply_exists(USER_2, mid2, pid2));

        // Balance checking.
        assert_eq!(Balances::free_balance(BLOCK_AUTHOR), block_author_balance);
        assert_eq!(
            Balances::free_balance(USER_1),
            user1_balance - GasPrice::gas_price(DEFAULT_GAS)
        );
        assert_eq!(
            Balances::reserved_balance(USER_1),
            GasPrice::gas_price(DEFAULT_GAS)
        );
        assert_eq!(
            Balances::free_balance(USER_2),
            user2_balance - GasPrice::gas_price(DEFAULT_GAS)
        );
        assert_eq!(
            Balances::reserved_balance(USER_2),
            GasPrice::gas_price(DEFAULT_GAS)
        );

        // Check if task and message got processed before start of block `bn`.
        // But due to the low gas allowance, we may process the only first task.
        run_to_block(bn, Some(db_r_w(1, 2).ref_time() + 1));
        // Read of missed blocks, write to it afterwards + single task processing.
        assert_eq!(GasAllowanceOf::<Test>::get(), 1);

        let cost1 = wl_cost_for(bn - initial_block);

        // Storages checking (order isn't guaranteed).
        if task_and_wl_message_exist(mid1, pid1, bn) {
            assert!(!task_and_wl_message_exist(mid2, pid2, bn));
            assert!(!out_of_rent_reply_exists(USER_1, mid1, pid1));
            assert!(out_of_rent_reply_exists(USER_2, mid2, pid2));
            assert_eq!(Balances::free_balance(USER_2), user2_balance - cost1);
            assert_eq!(Balances::reserved_balance(USER_2), 0);
        } else {
            assert!(task_and_wl_message_exist(mid2, pid2, bn));
            assert!(out_of_rent_reply_exists(USER_1, mid1, pid1));
            assert!(!out_of_rent_reply_exists(USER_2, mid2, pid2));
            assert_eq!(Balances::free_balance(USER_1), user1_balance - cost1);
            assert_eq!(Balances::reserved_balance(USER_1), 0);
        }

        assert_eq!(
            Balances::free_balance(BLOCK_AUTHOR),
            block_author_balance + cost1
        );

        // Check if missed task and message got processed in block `bn`.
        run_to_block(bn + 1, Some(u64::MAX));
        // Delete of missed blocks + single task processing.
        assert_eq!(
            GasAllowanceOf::<Test>::get(),
            u64::MAX - db_r_w(0, 2).ref_time()
        );

        let cost2 = wl_cost_for(bn + 1 - initial_block);

        // Storages checking.
        assert!(!task_and_wl_message_exist(mid1, pid1, bn));
        assert!(!task_and_wl_message_exist(mid2, pid2, bn));
        assert!(out_of_rent_reply_exists(USER_1, mid1, pid1));
        assert!(out_of_rent_reply_exists(USER_2, mid2, pid2));

        assert_eq!(
            Balances::free_balance(BLOCK_AUTHOR),
            block_author_balance + cost1 + cost2
        );
        assert_eq!(
            Balances::free_balance(USER_1) + Balances::free_balance(USER_2),
            user1_balance + user2_balance - cost1 - cost2
        );
        assert_eq!(Balances::reserved_balance(USER_1), 0);
        assert_eq!(Balances::reserved_balance(USER_2), 0);
    });
}
