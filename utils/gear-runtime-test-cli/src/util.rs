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

use frame_support::traits::{OnFinalize, OnIdle, OnInitialize};
use frame_system as system;
use gear_common::{storage::*, GasAllowance, Origin};
use gear_core::message::{StoredDispatch, StoredMessage};
use gear_runtime::{Gas, Gear, GearMessenger, Runtime, System};
use pallet_gear::{BlockGasLimitOf, GasAllowanceOf};
use pallet_gear_debug::DebugData;
use sp_runtime::{app_crypto::UncheckedFrom, AccountId32};

pub(crate) type QueueOf<T> = <<T as pallet_gear::Config>::Messenger as Messenger>::Queue;
pub(crate) type MailboxOf<T> = <<T as pallet_gear::Config>::Messenger as Messenger>::Mailbox;

pub fn get_dispatch_queue() -> Vec<StoredDispatch> {
    QueueOf::<Runtime>::iter()
        .map(|v| v.unwrap_or_else(|e| unreachable!("Message queue corrupted! {:?}", e)))
        .collect()
}

pub fn process_queue(snapshots: &mut Vec<DebugData>, mailbox: &mut Vec<StoredMessage>) {
    while !QueueOf::<Runtime>::is_empty() {
        run_to_block(System::block_number() + 1, None, false);
        // Parse data from events
        for event in System::events() {
            if let gear_runtime::Event::GearDebug(pallet_gear_debug::Event::DebugDataSnapshot(
                snapshot,
            )) = &event.event
            {
                snapshots.push(snapshot.clone());
            }

            if let gear_runtime::Event::Gear(pallet_gear::Event::UserMessageSent {
                message, ..
            }) = &event.event
            {
                mailbox.push(message.clone());
            }
        }
        System::reset_events();
    }
}

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
    let mut t = system::GenesisConfig::default()
        .build_storage::<Runtime>()
        .unwrap();

    pallet_balances::GenesisConfig::<Runtime> {
        balances: vec![(
            AccountId32::unchecked_from(1000001.into_origin()),
            (BlockGasLimitOf::<Runtime>::get() * 10) as u128,
        )],
    }
    .assimilate_storage(&mut t)
    .unwrap();

    let mut ext = sp_io::TestExternalities::new(t);
    ext.execute_with(|| System::set_block_number(1));
    ext
}

pub fn run_to_block(n: u32, remaining_weight: Option<u64>, skip_process_queue: bool) {
    while System::block_number() < n {
        System::on_finalize(System::block_number());
        System::set_block_number(System::block_number() + 1);
        System::on_initialize(System::block_number());
        Gas::on_initialize(System::block_number());
        GearMessenger::on_initialize(System::block_number());
        Gear::on_initialize(System::block_number());
        let remaining_weight = remaining_weight.unwrap_or_else(BlockGasLimitOf::<Runtime>::get);
        if skip_process_queue {
            GasAllowanceOf::<Runtime>::update(remaining_weight);
        } else {
            Gear::on_idle(System::block_number(), remaining_weight);
        }
    }
}
