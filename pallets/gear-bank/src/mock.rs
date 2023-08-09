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
    construct_runtime, parameter_types,
    traits::{Everything, FindAuthor},
    weights::constants::RocksDbWeight,
};
use frame_system::mocking::{MockBlock, MockUncheckedExtrinsic};
use pallet_balances::AccountData;
use primitive_types::H256;
use sp_io::TestExternalities;
use sp_runtime::{
    testing::Header,
    traits::{BlakeTwo256, ConstU32, IdentityLookup},
};

pub type AccountId = u8;
pub type Balance = u128;

mod consts {
    #![allow(unused)]

    use super::*;

    pub const ALICE: AccountId = 1;
    pub const ALICE_BALANCE: Balance = 100_000_000_000;

    pub const BOB: AccountId = 2;
    pub const BOB_BALANCE: Balance = 100_000_000;

    pub const BLOCK_AUTHOR: AccountId = 255;

    pub const BANK_ADDRESS: AccountId = 137;
    pub const EXISTENTIAL_DEPOSIT: Balance = 100_000;
}

pub use consts::*;

parameter_types! {
    pub const BankAddress: AccountId = BANK_ADDRESS;
    pub const BlockHashCount: u64 = 250;
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

impl frame_system::Config for Test {
    type BaseCallFilter = Everything;
    type BlockWeights = ();
    type BlockLength = ();
    type DbWeight = RocksDbWeight;
    type RuntimeOrigin = RuntimeOrigin;
    type RuntimeCall = RuntimeCall;
    type Index = u64;
    type BlockNumber = u64;
    type Hash = H256;
    type Hashing = BlakeTwo256;
    type AccountId = AccountId;
    type Lookup = IdentityLookup<Self::AccountId>;
    type Header = Header;
    type RuntimeEvent = RuntimeEvent;
    type BlockHashCount = BlockHashCount;
    type Version = ();
    type PalletInfo = PalletInfo;
    type AccountData = AccountData<u128>;
    type OnNewAccount = ();
    type OnKilledAccount = ();
    type SystemWeightInfo = ();
    type SS58Prefix = ();
    type OnSetCode = ();
    type MaxConsumers = ConstU32<16>;
}

pub struct FixedBlockAuthor;

impl FindAuthor<AccountId> for FixedBlockAuthor {
    fn find_author<'a, I: 'a>(_: I) -> Option<AccountId> {
        Some(BLOCK_AUTHOR)
    }
}

impl pallet_authorship::Config for Test {
    type FindAuthor = FixedBlockAuthor;
    type EventHandler = ();
}

impl pallet_balances::Config for Test {
    type MaxLocks = ();
    type MaxReserves = ();
    type ReserveIdentifier = [u8; 8];
    type Balance = Balance;
    type DustRemoval = ();
    type RuntimeEvent = RuntimeEvent;
    type ExistentialDeposit = ExistentialDeposit;
    type AccountStore = System;
    type WeightInfo = ();
}

impl pallet_gear_bank::Config for Test {
    type Currency = Balances;
    type BankAddress = BankAddress;
}

pub fn new_test_ext() -> TestExternalities {
    let mut storage = frame_system::GenesisConfig::default()
        .build_storage::<Test>()
        .unwrap();

    let balances = vec![
        (ALICE, ALICE_BALANCE),
        (BOB, BOB_BALANCE),
        (BANK_ADDRESS, EXISTENTIAL_DEPOSIT),
    ];

    pallet_balances::GenesisConfig::<Test> { balances }
        .assimilate_storage(&mut storage)
        .unwrap();

    let mut ext = TestExternalities::new(storage);
    ext.execute_with(|| System::set_block_number(1));
    ext
}
