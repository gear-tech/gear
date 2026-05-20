// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate as pallet_gear_gas;
use frame_support::{
    construct_runtime, parameter_types, traits::ConstU32, weights::constants::RocksDbWeight,
};
use frame_system::{self as system, pallet_prelude::BlockNumberFor};
use primitive_types::H256;
use sp_runtime::{
    BuildStorage,
    traits::{BlakeTwo256, IdentityLookup},
};
use sp_std::convert::{TryFrom, TryInto};

type Block = frame_system::mocking::MockBlock<Test>;
type AccountId = u64;
pub type BlockNumber = BlockNumberFor<Test>;
type Balance = u128;
type BlockGasLimit = ();

pub const ALICE: AccountId = 1;
pub const BOB: AccountId = 2;
pub const BLOCK_AUTHOR: AccountId = 255;

// Configure a mock runtime to test the pallet.
construct_runtime!(
    pub enum Test
    {
        System: system,
        GearMessenger: pallet_gear_messenger,
        GearGas: pallet_gear_gas,
        Balances: pallet_balances,
    }
);

common::impl_pallet_system!(Test, DbWeight = RocksDbWeight, BlockWeights = ());
pallet_gear_messenger::impl_config!(Test, CurrentBlockNumber = GearBlockNumber);
pallet_gear_gas::impl_config!(Test);
common::impl_pallet_balances!(Test);

parameter_types! {
    pub const BlockHashCount: BlockNumber = 250;
    pub const ExistentialDeposit: Balance = 1;
    pub const GearBlockNumber: BlockNumber = 100;
}

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
    let mut t = system::GenesisConfig::<Test>::default()
        .build_storage()
        .unwrap();

    pallet_balances::GenesisConfig::<Test> {
        balances: vec![
            (ALICE, 100_000_000_u128),
            (BOB, 2_u128),
            (BLOCK_AUTHOR, 1_u128),
        ],
    }
    .assimilate_storage(&mut t)
    .unwrap();

    let mut ext = sp_io::TestExternalities::new(t);
    ext.execute_with(|| System::set_block_number(1));
    ext
}
