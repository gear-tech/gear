// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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

use crate as pallet_gear_bank;
use frame_support::{
    construct_runtime, parameter_types, traits::FindAuthor, weights::constants::RocksDbWeight,
};
use frame_system::mocking::{MockBlock, MockUncheckedExtrinsic};
use primitive_types::H256;
use sp_io::TestExternalities;
use sp_runtime::{
    generic,
    traits::{BlakeTwo256, IdentityLookup},
};

pub type AccountId = u8;
pub type Balance = u128;
type BlockNumber = u64;

mod consts {
    #![allow(unused)]

    use super::*;

    pub const ALICE: AccountId = 1;
    pub const ALICE_BALANCE: Balance = 100_000_000_000;

    pub const BOB: AccountId = 2;
    pub const BOB_BALANCE: Balance = 150_000_000;

    pub const BLOCK_AUTHOR: AccountId = 255;

    pub const BANK_ADDRESS: AccountId = 137;

    pub const CHARLIE: AccountId = 3;
    pub const EVE: AccountId = 4;

    pub const EXISTENTIAL_DEPOSIT: Balance = 100_000;

    pub const VALUE_PER_GAS: Balance = 25;
}

pub use consts::*;

parameter_types! {
    pub const BankAddress: AccountId = BANK_ADDRESS;
    pub const GasMultiplier: common::GasMultiplier<Balance, u64> = common::GasMultiplier::ValuePerGas(VALUE_PER_GAS);
    pub const BlockHashCount: BlockNumber = 250;
    pub const ExistentialDeposit: Balance = EXISTENTIAL_DEPOSIT;
}

construct_runtime!(
    pub enum Test where
        Block = MockBlock<Test>,
        NodeBlock = MockBlock<Test>,
        UncheckedExtrinsic = MockUncheckedExtrinsic<Test>,
    {
        System: frame_system,
        Authorship: pallet_authorship,
        Balances: pallet_balances,
        GearBank: pallet_gear_bank,
    }
);

common::impl_pallet_system!(Test, DbWeight = RocksDbWeight, BlockWeights = (),);
common::impl_pallet_authorship!(Test);
common::impl_pallet_balances!(Test);

impl pallet_gear_bank::Config for Test {
    type Currency = Balances;
    type BankAddress = BankAddress;
    type GasMultiplier = GasMultiplier;
}

pub fn new_test_ext() -> TestExternalities {
    let mut storage = frame_system::GenesisConfig::default()
        .build_storage::<Test>()
        .unwrap();

    let balances = vec![
        (ALICE, ALICE_BALANCE),
        (BOB, BOB_BALANCE),
        (BANK_ADDRESS, EXISTENTIAL_DEPOSIT),
        (BLOCK_AUTHOR, EXISTENTIAL_DEPOSIT),
        (CHARLIE, EXISTENTIAL_DEPOSIT),
        (EVE, EXISTENTIAL_DEPOSIT),
    ];

    pallet_balances::GenesisConfig::<Test> { balances }
        .assimilate_storage(&mut storage)
        .unwrap();

    let mut ext = TestExternalities::new(storage);
    ext.execute_with(|| System::set_block_number(1));
    ext
}
