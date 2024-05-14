// This file is part of Gear.

// Copyright (C) 2023-2024 Gear Technologies Inc.
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

#![allow(unused)]

use crate as pallet_gear_program;
use crate::*;
use common::pallet_tests::MAX_BLOCK;
use frame_support::{
    construct_runtime,
    pallet_prelude::*,
    parameter_types,
    traits::{ConstU32, ConstU64, FindAuthor},
    weights::RuntimeDbWeight,
};
use frame_system::{self as system, pallet_prelude::BlockNumberFor};
use sp_core::H256;
use sp_runtime::{
    traits::{BlakeTwo256, IdentityLookup},
    BuildStorage, Perbill,
};
use sp_std::convert::{TryFrom, TryInto};

type Block = frame_system::mocking::MockBlock<Test>;
type AccountId = u64;
pub type BlockNumber = BlockNumberFor<Test>;
type Balance = u128;

pub(crate) const USER_1: AccountId = 1;
pub(crate) const USER_2: AccountId = 2;
pub(crate) const USER_3: AccountId = 3;
pub(crate) const LOW_BALANCE_USER: AccountId = 4;
pub(crate) const BLOCK_AUTHOR: AccountId = 255;

// Configure a mock runtime to test the pallet.
construct_runtime!(
    pub enum Test
    {
        System: system,
        GearProgram: pallet_gear_program,
        GearScheduler: pallet_gear_scheduler,
        GearGas: pallet_gear_gas,
        Balances: pallet_balances,
        Authorship: pallet_authorship,
        Timestamp: pallet_timestamp,
    }
);

common::impl_pallet_system!(Test);
pallet_gear_program::impl_config!(Test);
pallet_gear_scheduler::impl_config!(Test);
pallet_gear_gas::impl_config!(Test);
common::impl_pallet_balances!(Test);
common::impl_pallet_authorship!(Test);
common::impl_pallet_timestamp!(Test);

parameter_types! {
    pub const BlockGasLimit: u64 = MAX_BLOCK;
    pub const BlockHashCount: BlockNumber = 250;
    pub const ExistentialDeposit: Balance = 500;
    pub ReserveThreshold: BlockNumber = 1;
}

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
    let mut t = system::GenesisConfig::<Test>::default()
        .build_storage()
        .unwrap();

    pallet_balances::GenesisConfig::<Test> {
        balances: vec![
            (USER_1, 5_000_000_000_000_000_u128),
            (USER_2, 200_000_000_000_000_u128),
            (USER_3, 500_000_000_000_000_u128),
            (LOW_BALANCE_USER, 1_000_000_u128),
            (BLOCK_AUTHOR, 500_000_u128),
        ],
    }
    .assimilate_storage(&mut t)
    .unwrap();

    let mut ext = sp_io::TestExternalities::new(t);
    ext.execute_with(|| {
        System::set_block_number(1);
    });
    ext
}
