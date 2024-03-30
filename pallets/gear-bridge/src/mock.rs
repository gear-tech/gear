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

//! Module with runtime mock for running tests.

use crate as pallet_gear_bridge;
use frame_support::{
    construct_runtime, parameter_types, traits::Hooks, weights::constants::RocksDbWeight,
};
use frame_system::{self as system, pallet_prelude::BlockNumberFor};
use primitive_types::H256;
use sp_runtime::{
    traits::{BlakeTwo256, IdentityLookup},
    BuildStorage,
};
use sp_std::convert::{TryFrom, TryInto};

type Block = frame_system::mocking::MockBlock<Test>;
type AccountId = u64;
pub type BlockNumber = BlockNumberFor<Test>;
type Balance = u128;

pub const USER: AccountId = 1;

// Configure a mock runtime to test the pallet.
construct_runtime!(
    pub enum Test
    {
        System: system,
        Balances: pallet_balances,
        GearBridge: pallet_gear_bridge,
    }
);

common::impl_pallet_system!(Test, DbWeight = RocksDbWeight, BlockWeights = ());
common::impl_pallet_balances!(Test);

parameter_types! {
    pub const BlockHashCount: BlockNumber = 250;
    pub const ExistentialDeposit: Balance = 1000;
    pub const QueueLimit: u32 = 128;
}

impl crate::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type QueueLimit = QueueLimit;
}

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
    let t = system::GenesisConfig::<Test>::default()
        .build_storage()
        .unwrap();

    let mut ext = sp_io::TestExternalities::new(t);
    ext.execute_with(|| System::set_block_number(1));
    ext
}

// Runs blocks to some specific number.
pub fn run_to_block(n: u64) {
    while System::block_number() < n {
        System::on_finalize(System::block_number());
        GearBridge::on_finalize(System::block_number());

        System::set_block_number(System::block_number() + 1);

        System::on_initialize(System::block_number());
        GearBridge::on_initialize(System::block_number());
    }
}

pub fn run_to_next_block() {
    run_to_block(System::block_number() + 1)
}

pub fn init_logger() {
    let _ = env_logger::Builder::from_default_env()
        .format_module_path(false)
        .format_level(true)
        .try_init();
}
