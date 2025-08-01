// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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
    PalletId, construct_runtime, parameter_types,
    traits::{ConstU32, FindAuthor},
    weights::constants::RocksDbWeight,
};
use primitive_types::H256;
use sp_io::TestExternalities;
use sp_runtime::{
    BuildStorage, Percent,
    traits::{BlakeTwo256, IdentityLookup},
};

pub type AccountId = u8;
type Block = frame_system::mocking::MockBlock<Test>;
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

    pub const CHARLIE: AccountId = 3;
    pub const EVE: AccountId = 4;

    pub const TREASURY: AccountId = 77;

    pub const EXISTENTIAL_DEPOSIT: Balance = 100_000;

    pub const VALUE_PER_GAS: Balance = 100;
}

pub use consts::*;

parameter_types! {
    pub const BankPalletId: PalletId = PalletId(*b"py/gbank");
    pub const GasMultiplier: common::GasMultiplier<Balance, u64> = common::GasMultiplier::ValuePerGas(VALUE_PER_GAS);
    pub const BlockHashCount: BlockNumber = 250;
    pub const ExistentialDeposit: Balance = EXISTENTIAL_DEPOSIT;
    pub const TreasuryAddress: AccountId = TREASURY;
    pub const TreasuryGasFeeShare: Percent = Percent::from_percent(50);
}

construct_runtime!(
    pub enum Test
    {
        System: frame_system,
        Authorship: pallet_authorship,
        Balances: pallet_balances,
        GearBank: pallet_gear_bank,
    }
);

common::impl_pallet_system!(Test, DbWeight = RocksDbWeight, BlockWeights = ());
common::impl_pallet_authorship!(Test);
common::impl_pallet_balances!(Test);
pallet_gear_bank::impl_config!(
    Test,
    TreasuryAddress = TreasuryAddress,
    TreasuryGasFeeShare = TreasuryGasFeeShare
);

pub fn new_test_ext() -> TestExternalities {
    let mut storage = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .unwrap();

    let balances = vec![
        (ALICE, ALICE_BALANCE),
        (BOB, BOB_BALANCE),
        (GearBank::bank_address(), EXISTENTIAL_DEPOSIT),
        (BLOCK_AUTHOR, EXISTENTIAL_DEPOSIT),
        (CHARLIE, EXISTENTIAL_DEPOSIT),
        (EVE, EXISTENTIAL_DEPOSIT),
        (TREASURY, EXISTENTIAL_DEPOSIT),
    ];

    pallet_balances::GenesisConfig::<Test> { balances }
        .assimilate_storage(&mut storage)
        .unwrap();

    pallet_gear_bank::GenesisConfig::<Test> {
        _config: Default::default(),
    }
    .assimilate_storage(&mut storage)
    .unwrap();

    let mut ext = TestExternalities::new(storage);
    ext.execute_with(|| System::set_block_number(1));
    ext
}
