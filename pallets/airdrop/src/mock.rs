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

use crate as pallet_airdrop;
use frame_support::{
    construct_runtime, parameter_types,
    traits::{ConstU64, GenesisBuild},
};
use frame_support_test::TestRandomness;
use frame_system as system;
use sp_core::ConstU128;
use sp_runtime::{
    testing::Header,
    traits::{BlakeTwo256, IdentityLookup},
};
use sp_std::convert::{TryFrom, TryInto};

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;

pub const ALICE: u64 = 1;
pub const ROOT: u64 = 255;

// Configure a mock runtime to test the pallet.
construct_runtime!(
    pub enum Test where
        Block = Block,
        NodeBlock = Block,
        UncheckedExtrinsic = UncheckedExtrinsic,
    {
        System: system,
        Balances: pallet_balances,
        Sudo: pallet_sudo,
        Authorship: pallet_authorship,
        Timestamp: pallet_timestamp,
        GearProgram: pallet_gear_program,
        GearMessenger: pallet_gear_messenger,
        GearScheduler: pallet_gear_scheduler,
        GearGas: pallet_gear_gas,
        Gear: pallet_gear,
        Airdrop: pallet_airdrop,
    }
);

parameter_types! {
    pub const BlockHashCount: u64 = 250;
    pub const SS58Prefix: u8 = 42;
    pub const ExistentialDeposit: u64 = 1;
    pub const OutgoingLimit: u32 = 1024;
    pub GearSchedule: pallet_gear::Schedule<Test> = <pallet_gear::Schedule<Test>>::default();
}

impl system::Config for Test {
    type BaseCallFilter = frame_support::traits::Everything;
    type BlockWeights = ();
    type BlockLength = ();
    type DbWeight = ();
    type RuntimeOrigin = RuntimeOrigin;
    type RuntimeCall = RuntimeCall;
    type Index = u64;
    type BlockNumber = u64;
    type Hash = sp_core::H256;
    type Hashing = BlakeTwo256;
    type AccountId = u64;
    type Lookup = IdentityLookup<Self::AccountId>;
    type Header = Header;
    type RuntimeEvent = RuntimeEvent;
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

impl pallet_balances::Config for Test {
    type MaxLocks = ();
    type MaxReserves = ();
    type ReserveIdentifier = [u8; 8];
    type Balance = u128;
    type DustRemoval = ();
    type RuntimeEvent = RuntimeEvent;
    type ExistentialDeposit = ExistentialDeposit;
    type AccountStore = System;
    type WeightInfo = ();
}

impl pallet_sudo::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type RuntimeCall = RuntimeCall;
}

impl pallet_timestamp::Config for Test {
    type Moment = u64;
    type OnTimestampSet = ();
    type MinimumPeriod = ConstU64<500>;
    type WeightInfo = ();
}

impl pallet_authorship::Config for Test {
    type FindAuthor = ();

    type EventHandler = ();
}

impl pallet_gear_gas::Config for Test {
    type BlockGasLimit = ConstU64<100_000_000>;
}

impl pallet_gear_messenger::Config for Test {
    type BlockLimiter = GearGas;
    type CurrentBlockNumber = Gear;
}

impl pallet_gear_program::Config for Test {}

pub struct GasConverter;
impl common::GasPrice for GasConverter {
    type Balance = u128;
    type GasToBalanceMultiplier = ConstU128<1_000>;
}

impl pallet_gear::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type Randomness = TestRandomness<Self>;
    type Currency = Balances;
    type GasPrice = GasConverter;
    type WeightInfo = ();
    type Schedule = GearSchedule;
    type OutgoingLimit = OutgoingLimit;
    type DebugInfo = ();
    type ProgramStorage = GearProgram;
    type CodeStorage = GearProgram;
    type MailboxThreshold = ConstU64<3000>;
    type ReservationsLimit = ConstU64<256>;
    type Messenger = GearMessenger;
    type GasProvider = GearGas;
    type BlockLimiter = GearGas;
    type Scheduler = GearScheduler;
    type QueueRunner = Gear;
}

impl pallet_gear_scheduler::Config for Test {
    type BlockLimiter = GearGas;
    type ReserveThreshold = ConstU64<1>;
    type WaitlistCost = ConstU64<100>;
    type MailboxCost = ConstU64<100>;
    type ReservationCost = ConstU64<100>;
    type DispatchHoldCost = ConstU64<100>;
}

impl pallet_airdrop::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type WeightInfo = ();
}

pub type AirdropCall = pallet_airdrop::Call<Test>;

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
    let mut t = system::GenesisConfig::default()
        .build_storage::<Test>()
        .unwrap();

    pallet_balances::GenesisConfig::<Test> {
        balances: vec![(ROOT, 100_000_000_u128)],
    }
    .assimilate_storage(&mut t)
    .unwrap();

    pallet_sudo::GenesisConfig::<Test> { key: Some(ROOT) }
        .assimilate_storage(&mut t)
        .unwrap();
    t.into()
}
