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

use crate as pallet_gear_debug;
use frame_support::{
    construct_runtime, parameter_types,
    traits::{FindAuthor, OnFinalize, OnIdle, OnInitialize},
};
use frame_system as system;
use primitive_types::H256;
use sp_runtime::{
    testing::Header,
    traits::{BlakeTwo256, IdentityLookup},
};
use sp_std::convert::{TryFrom, TryInto};

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;

pub const BLOCK_AUTHOR: u64 = 255;

impl pallet_balances::Config for Test {
    type MaxLocks = ();
    type MaxReserves = ();
    type ReserveIdentifier = [u8; 8];
    type Balance = u128;
    type DustRemoval = ();
    type Event = Event;
    type ExistentialDeposit = ExistentialDeposit;
    type AccountStore = System;
    type WeightInfo = ();
}

parameter_types! {
    pub const BlockHashCount: u64 = 250;
    pub const SS58Prefix: u8 = 42;
    pub const ExistentialDeposit: u64 = 1;
}

impl system::Config for Test {
    type BaseCallFilter = frame_support::traits::Everything;
    type BlockWeights = ();
    type BlockLength = ();
    type DbWeight = ();
    type Origin = Origin;
    type Call = Call;
    type Index = u64;
    type BlockNumber = u64;
    type Hash = H256;
    type Hashing = BlakeTwo256;
    type AccountId = u64;
    type Lookup = IdentityLookup<Self::AccountId>;
    type Header = Header;
    type Event = Event;
    type BlockHashCount = BlockHashCount;
    type Version = ();
    type PalletInfo = PalletInfo;
    type AccountData = pallet_balances::AccountData<u128>;
    type OnNewAccount = ();
    type OnKilledAccount = ();
    type SystemWeightInfo = ();
    type SS58Prefix = SS58Prefix;
    type OnSetCode = ();
    type MaxConsumers = frame_support::traits::ConstU32<16>;
}

impl pallet_gear_debug::Config for Test {
    type Event = Event;
    type WeightInfo = ();
    type CodeStorage = GearProgram;
    type Messenger = GearMessenger;
}

pub struct FixedBlockAuthor;

impl FindAuthor<u64> for FixedBlockAuthor {
    fn find_author<'a, I>(_digests: I) -> Option<u64>
    where
        I: 'a + IntoIterator<Item = (sp_runtime::ConsensusEngineId, &'a [u8])>,
    {
        Some(BLOCK_AUTHOR)
    }
}

impl pallet_authorship::Config for Test {
    type FindAuthor = FixedBlockAuthor;
    type UncleGenerations = ();
    type FilterUncle = ();
    type EventHandler = ();
}

parameter_types! {
    pub const MinimumPeriod: u64 = 500;
    pub const OutgoingLimit: u32 = 1024;
    pub const BlockGasLimit: u64 = 10_000_000_000;
}

impl pallet_timestamp::Config for Test {
    type Moment = u64;
    type OnTimestampSet = ();
    type MinimumPeriod = MinimumPeriod;
    type WeightInfo = ();
}

pub struct GasConverter;
impl common::GasPrice for GasConverter {
    type Balance = u128;
}

impl pallet_gear_program::Config for Test {
    type Event = Event;
    type WeightInfo = ();
    type Currency = Balances;
    type Messenger = GearMessenger;
}

impl pallet_gear::Config for Test {
    type Event = Event;
    type Currency = Balances;
    type GasPrice = GasConverter;
    type WeightInfo = ();
    type OutgoingLimit = OutgoingLimit;
    type DebugInfo = super::Pallet<Test>;
    type WaitListFeePerBlock = ();
    type Schedule = ();
    type CodeStorage = GearProgram;
    type Messenger = GearMessenger;
    type ValueTreeProvider = Gas;
}

impl pallet_gear_messenger::Config for Test {
    type Currency = Balances;
}

impl pallet_gas::Config for Test {
    type BlockGasLimit = BlockGasLimit;
}

// Configure a mock runtime to test the pallet.
construct_runtime!(
    pub enum Test where
        Block = Block,
        NodeBlock = Block,
        UncheckedExtrinsic = UncheckedExtrinsic,
    {
        System: system::{Pallet, Call, Config, Storage, Event<T>},
        GearDebug: pallet_gear_debug::{Pallet, Call, Storage, Event<T>},
        Balances: pallet_balances::{Pallet, Call, Storage, Config<T>, Event<T>},
        Authorship: pallet_authorship::{Pallet, Storage},
        Timestamp: pallet_timestamp::{Pallet, Storage},
        GearProgram: pallet_gear_program::{Pallet, Storage, Event<T>},
        GearMessenger: pallet_gear_messenger::{Pallet},
        Gear: pallet_gear::{Pallet, Call, Storage, Event<T>},
        Gas: pallet_gas,
    }
);

// Build genesis storage according to the mock runtime.
#[allow(unused)]
pub fn new_test_ext() -> sp_io::TestExternalities {
    let mut t = system::GenesisConfig::default()
        .build_storage::<Test>()
        .unwrap();

    pallet_balances::GenesisConfig::<Test> {
        balances: vec![
            (1, 100_000_000_000_u128),
            (2, 2_u128),
            (BLOCK_AUTHOR, 1_u128),
        ],
    }
    .assimilate_storage(&mut t)
    .unwrap();

    let mut ext = sp_io::TestExternalities::new(t);
    ext.execute_with(|| System::set_block_number(1));
    ext
}

pub fn run_to_block(n: u64, remaining_weight: Option<u64>) {
    while System::block_number() < n {
        System::on_finalize(System::block_number());
        System::set_block_number(System::block_number() + 1);
        System::on_initialize(System::block_number());
        Gas::on_initialize(System::block_number());
        GearMessenger::on_initialize(System::block_number());
        Gear::on_initialize(System::block_number());
        let remaining_weight =
            remaining_weight.unwrap_or(pallet_gear::BlockGasLimitOf::<Test>::get());
        Gear::on_idle(System::block_number(), remaining_weight);
    }
}
