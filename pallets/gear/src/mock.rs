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

use crate as pallet_gear;
use crate::*;
use frame_support::{
    construct_runtime,
    pallet_prelude::*,
    parameter_types,
    traits::{ConstU64, FindAuthor, Get},
    weights::RuntimeDbWeight,
};
use frame_support_test::TestRandomness;
use frame_system::{self as system, limits::BlockWeights};
use sp_core::{ConstU128, H256};
use sp_runtime::{
    testing::Header,
    traits::{BlakeTwo256, IdentityLookup},
    Perbill,
};
use sp_std::convert::{TryFrom, TryInto};

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;
type AccountId = u64;

type BlockWeightsOf<T> = <T as frame_system::Config>::BlockWeights;

pub(crate) const USER_1: AccountId = 1;
pub(crate) const USER_2: AccountId = 2;
pub(crate) const USER_3: AccountId = 3;
pub(crate) const LOW_BALANCE_USER: AccountId = 4;
pub(crate) const BLOCK_AUTHOR: AccountId = 255;
const NORMAL_DISPATCH_RATIO: Perbill = Perbill::from_percent(75);
const MAX_BLOCK: u64 = 100_000_000_000;

macro_rules! dry_run {
    (
        $weight:ident,
        $initial_weight:expr
    ) => {
        GasAllowanceOf::<Test>::put($initial_weight);

        let mut ext_manager = Default::default();
        pallet_gear::Pallet::<Test>::process_tasks(&mut ext_manager);
        pallet_gear::Pallet::<Test>::process_queue(ext_manager);

        let $weight = $initial_weight.saturating_sub(GasAllowanceOf::<Test>::get());
    };
}

// Configure a mock runtime to test the pallet.
construct_runtime!(
    pub enum Test where
        Block = Block,
        NodeBlock = Block,
        UncheckedExtrinsic = UncheckedExtrinsic,
    {
        System: system,
        GearProgram: pallet_gear_program,
        GearMessenger: pallet_gear_messenger,
        GearScheduler: pallet_gear_scheduler,
        Gear: pallet_gear,
        GearGas: pallet_gear_gas,
        Balances: pallet_balances,
        Authorship: pallet_authorship,
        Timestamp: pallet_timestamp,
    }
);

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

parameter_types! {
    pub const BlockHashCount: u64 = 250;
    pub RuntimeBlockWeights: BlockWeights = BlockWeights::with_sensible_defaults(
        Weight::from_parts(MAX_BLOCK, u64::MAX),
        NORMAL_DISPATCH_RATIO,
    );
    pub const SS58Prefix: u8 = 42;
    pub const ExistentialDeposit: u64 = 500;
    pub const DbWeight: RuntimeDbWeight = RuntimeDbWeight { read: 1110, write: 2300 };
}

impl system::Config for Test {
    type BaseCallFilter = frame_support::traits::Everything;
    type BlockWeights = RuntimeBlockWeights;
    type BlockLength = ();
    type DbWeight = DbWeight;
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
    type AccountData = pallet_balances::AccountData<u128>;
    type OnNewAccount = ();
    type OnKilledAccount = ();
    type SystemWeightInfo = ();
    type SS58Prefix = SS58Prefix;
    type OnSetCode = ();
    type MaxConsumers = frame_support::traits::ConstU32<16>;
}

pub struct GasConverter;
impl common::GasPrice for GasConverter {
    type Balance = u128;
    type GasToBalanceMultiplier = ConstU128<1_000>;
}

impl pallet_gear_program::Config for Test {}

parameter_types! {
    // Match the default `max_block` set in frame_system::limits::BlockWeights::with_sensible_defaults()
    pub const BlockGasLimit: u64 = MAX_BLOCK;
    pub const OutgoingLimit: u32 = 1024;
    pub GearSchedule: pallet_gear::Schedule<Test> = <pallet_gear::Schedule<Test>>::default();
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
    type CodeStorage = GearProgram;
    type ProgramStorage = GearProgram;
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

impl pallet_gear_gas::Config for Test {
    type BlockGasLimit = BlockGasLimit;
}

impl pallet_gear_messenger::Config for Test {
    type BlockLimiter = GearGas;
    type CurrentBlockNumber = Gear;
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

    type EventHandler = ();
}

parameter_types! {
    pub const MinimumPeriod: u64 = 500;
}

impl pallet_timestamp::Config for Test {
    type Moment = u64;
    type OnTimestampSet = ();
    type MinimumPeriod = MinimumPeriod;
    type WeightInfo = ();
}

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
    let mut t = system::GenesisConfig::default()
        .build_storage::<Test>()
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
        Gear::on_initialize(System::block_number());
    });
    ext
}

pub fn get_min_weight() -> Weight {
    new_test_ext().execute_with(|| {
        dry_run!(weight, BlockGasLimitOf::<Test>::get());
        Weight::from_ref_time(weight)
    })
}

pub fn get_weight_of_adding_task() -> Weight {
    let minimal_weight = get_min_weight();

    new_test_ext().execute_with(|| {
        let gas_allowance = GasAllowanceOf::<Test>::get();

        dry_run!(_weight, BlockGasLimitOf::<Test>::get());

        TaskPoolOf::<Test>::add(
            100,
            ScheduledTask::RemoveFromMailbox(USER_2, Default::default()),
        )
        .unwrap_or_else(|e| unreachable!("Scheduling logic invalidated! {:?}", e));

        Weight::from_ref_time(gas_allowance - GasAllowanceOf::<Test>::get())
    }) - minimal_weight
}

pub fn run_to_block(n: u64, remaining_weight: Option<u64>) {
    while System::block_number() < n {
        System::on_finalize(System::block_number());
        System::set_block_number(System::block_number() + 1);
        System::on_initialize(System::block_number());
        GearGas::on_initialize(System::block_number());
        GearMessenger::on_initialize(System::block_number());
        Gear::on_initialize(System::block_number());

        if let Some(remaining_weight) = remaining_weight {
            GasAllowanceOf::<Test>::put(remaining_weight);
            let max_block_weight = <BlockWeightsOf<Test> as Get<BlockWeights>>::get().max_block;
            System::register_extra_weight_unchecked(
                max_block_weight.saturating_sub(Weight::from_ref_time(remaining_weight)),
                DispatchClass::Normal,
            );
        }

        Gear::run(frame_support::dispatch::RawOrigin::None.into()).unwrap();
        Gear::on_finalize(System::block_number());

        assert!(!System::events().iter().any(|e| {
            matches!(
                e.event,
                RuntimeEvent::Gear(pallet_gear::Event::QueueProcessingReverted)
            )
        }))
    }
}

pub fn run_to_next_block(remaining_weight: Option<u64>) {
    run_to_block(System::block_number() + 1, remaining_weight);
}
