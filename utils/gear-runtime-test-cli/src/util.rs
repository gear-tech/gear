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

use frame_support::traits::{OnFinalize, OnIdle, OnInitialize};
use frame_system as system;

use gear_common::{Dispatch, Message, Origin, STORAGE_MESSAGE_PREFIX};
use gear_runtime::{Gear, Runtime, System};

use pallet_gear_debug::DebugData;

use sp_core::H256;
use sp_runtime::{app_crypto::UncheckedFrom, AccountId32};

pub fn get_dispatch_queue() -> Vec<Dispatch> {
    #[derive(Decode, Encode)]
    struct Node {
        value: Dispatch,
        next: Option<H256>,
    }

    let mq_head_key = [STORAGE_MESSAGE_PREFIX, b"head"].concat();
    let mut dispatch_queue = vec![];

    if let Some(head) = sp_io::storage::get(&mq_head_key) {
        let mut next_id = H256::from_slice(&head[..]);
        loop {
            let next_node_key = [STORAGE_MESSAGE_PREFIX, next_id.as_bytes()].concat();
            if let Some(bytes) = sp_io::storage::get(&next_node_key) {
                let current_node = Node::decode(&mut &bytes[..]).unwrap();
                dispatch_queue.push(current_node.value);
                match current_node.next {
                    Some(h) => next_id = h,
                    None => break,
                }
            }
        }
    }

    dispatch_queue
}

pub fn process_queue(snapshots: &mut Vec<DebugData>, mailbox: &mut Vec<Message>) {
    while !gear_common::StorageQueue::<Dispatch>::get(STORAGE_MESSAGE_PREFIX).is_empty() {
        run_to_block(System::block_number() + 1, None);
        // Parse data from events
        for event in System::events() {
            if let gear_runtime::Event::GearDebug(pallet_gear_debug::Event::DebugDataSnapshot(
                snapshot,
            )) = &event.event
            {
                snapshots.push(snapshot.clone());
            }

            if let gear_runtime::Event::Gear(pallet_gear::Event::Log(msg)) = &event.event {
                mailbox.push(msg.clone());
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
            (<Runtime as pallet_gear::Config>::BlockGasLimit::get() * 10) as u128,
        )],
    }
    .assimilate_storage(&mut t)
    .unwrap();

    let mut ext = sp_io::TestExternalities::new(t);
    ext.execute_with(|| System::set_block_number(1));
    ext
}

pub fn run_to_block(n: u32, remaining_weight: Option<u64>) {
    while System::block_number() < n {
        System::on_finalize(System::block_number());
        System::set_block_number(System::block_number() + 1);
        System::on_initialize(System::block_number());
        Gear::on_initialize(System::block_number());
        let remaining_weight =
            remaining_weight.unwrap_or_else(<Runtime as pallet_gear::Config>::BlockGasLimit::get);
        Gear::on_idle(System::block_number(), remaining_weight);
    }
}
