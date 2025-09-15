// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

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

use crate as pallet_gear;
use crate::*;
use common::pallet_tests::MAX_BLOCK;
use frame_support::{
    PalletId, construct_runtime,
    pallet_prelude::*,
    parameter_types,
    traits::{ConstU64, FindAuthor, Get},
};
use frame_support_test::TestRandomness;
use frame_system::{self as system, limits::BlockWeights, mocking, pallet_prelude::BlockNumberFor};
use sp_core::{ConstU8, H256};
use sp_runtime::{
    BuildStorage,
    traits::{BlakeTwo256, IdentityLookup},
};
use sp_std::{
    cell::RefCell,
    convert::{TryFrom, TryInto},
};

type Block = mocking::MockBlock<Test>;
type AccountId = u64;
pub type BlockNumber = BlockNumberFor<Test>;
type Balance = u128;

type BlockWeightsOf<T> = <T as frame_system::Config>::BlockWeights;

pub(crate) const USER_1: AccountId = 1;
pub(crate) const USER_2: AccountId = 2;
pub(crate) const USER_3: AccountId = 3;
pub(crate) const LOW_BALANCE_USER: AccountId = 4;
pub(crate) const BLOCK_AUTHOR: AccountId = 255;
pub(crate) const RENT_POOL: AccountId = 256;

macro_rules! dry_run {
    (
        $weight:ident,
        $initial_weight:expr
    ) => {
        GasAllowanceOf::<Test>::put($initial_weight);

        let (builtins, _) = <Test as Config>::BuiltinDispatcherFactory::create();
        let mut ext_manager = ExtManager::<Test>::new(builtins);
        pallet_gear::Pallet::<Test>::process_tasks(&mut ext_manager);
        pallet_gear::Pallet::<Test>::process_queue(ext_manager);

        let $weight = $initial_weight.saturating_sub(GasAllowanceOf::<Test>::get());
    };
}

// Configure a mock runtime to test the pallet.
construct_runtime!(
    pub enum Test
    {
        System: system,
        GearProgram: pallet_gear_program,
        GearMessenger: pallet_gear_messenger,
        GearScheduler: pallet_gear_scheduler,
        GearBank: pallet_gear_bank,
        Gear: pallet_gear,
        GearGas: pallet_gear_gas,
        GearVoucher: pallet_gear_voucher,
        Balances: pallet_balances,
        Authorship: pallet_authorship,
        Timestamp: pallet_timestamp,
    }
);

common::impl_pallet_system!(Test);
pallet_gear_program::impl_config!(Test);
pallet_gear_messenger::impl_config!(Test, CurrentBlockNumber = Gear);
pallet_gear_scheduler::impl_config!(Test);
pallet_gear_bank::impl_config!(Test);
pallet_gear::impl_config!(Test, Schedule = DynamicSchedule, RentPoolId = ConstU64<RENT_POOL>);
pallet_gear_gas::impl_config!(Test);
common::impl_pallet_authorship!(Test);
common::impl_pallet_timestamp!(Test);
common::impl_pallet_balances!(Test);

parameter_types! {
    pub const BlockHashCount: BlockNumber = 250;
    pub const ExistentialDeposit: Balance = 500;
}

parameter_types! {
    // Match the default `max_block` set in frame_system::limits::BlockWeights::with_sensible_defaults()
    pub const BlockGasLimit: u64 = MAX_BLOCK;
    pub const OutgoingLimit: u32 = 1024;
    pub const OutgoingBytesLimit: u32 = 64 * 1024 * 1024;
    pub ReserveThreshold: BlockNumber = 1;
    pub RentFreePeriod: BlockNumber = 1_000;
    pub RentCostPerBlock: Balance = 11;
    pub ResumeMinimalPeriod: BlockNumber = 100;
    pub ResumeSessionDuration: BlockNumber = 1_000;
    pub const PerformanceMultiplier: u32 = 100;
}

thread_local! {
    static SCHEDULE: RefCell<Option<Schedule<Test>>> = const { RefCell::new(None) };
}

#[derive(Debug)]
pub struct DynamicSchedule;

impl DynamicSchedule {
    fn with<F, R>(f: F) -> R
    where
        F: FnOnce(&mut Schedule<Test>) -> R,
    {
        SCHEDULE.with_borrow_mut(|schedule| f(schedule.get_or_insert_with(Default::default)))
    }

    pub fn mutate<F>(f: F) -> DynamicScheduleReset
    where
        F: FnOnce(&mut Schedule<Test>),
    {
        Self::with(f);
        DynamicScheduleReset(())
    }

    pub fn get() -> Schedule<Test> {
        Self::with(|schedule| schedule.clone())
    }
}

impl Get<Schedule<Test>> for DynamicSchedule {
    fn get() -> Schedule<Test> {
        Self::get()
    }
}

#[must_use]
pub struct DynamicScheduleReset(());

impl Drop for DynamicScheduleReset {
    fn drop(&mut self) {
        SCHEDULE.with(|schedule| {
            *schedule.borrow_mut() = Some(Default::default());
        })
    }
}

parameter_types! {
    pub const BankPalletId: PalletId = PalletId(*b"py/gbank");
    pub const GasMultiplier: common::GasMultiplier<Balance, u64> = common::GasMultiplier::ValuePerGas(100);
    pub const MinVoucherDuration: BlockNumber = 5;
    pub const MaxVoucherDuration: BlockNumber = 100_000_000;
}

parameter_types! {
    pub const VoucherPalletId: PalletId = PalletId(*b"py/vouch");
}

impl pallet_gear_voucher::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type PalletId = VoucherPalletId;
    type WeightInfo = ();
    type CallsDispatcher = pallet_gear::PrepaidCallDispatcher<Test>;
    type Mailbox = MailboxOf<Self>;
    type MaxProgramsAmount = ConstU8<32>;
    type MaxDuration = MaxVoucherDuration;
    type MinDuration = MinVoucherDuration;
}

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
    let mut t = system::GenesisConfig::<Test>::default()
        .build_storage()
        .unwrap();

    pallet_balances::GenesisConfig::<Test> {
        balances: vec![
            (USER_1, 5_000_000_000_000_000_u128),
            (USER_2, 350_000_000_000_000_u128),
            (USER_3, 500_000_000_000_000_u128),
            (LOW_BALANCE_USER, 1_000_000_u128),
            (BLOCK_AUTHOR, 500_000_u128),
            (RENT_POOL, ExistentialDeposit::get()),
            (GearBank::bank_address(), ExistentialDeposit::get()),
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
        Weight::from_parts(weight, 0)
    })
}

pub fn get_weight_of_adding_task() -> Weight {
    let minimal_weight = get_min_weight();

    new_test_ext().execute_with(|| {
        let gas_allowance = GasAllowanceOf::<Test>::get();

        dry_run!(_weight, BlockGasLimitOf::<Test>::get());

        TaskPoolOf::<Test>::add(
            100,
            VaraScheduledTask::RemoveFromMailbox(USER_2, Default::default()),
        )
        .unwrap_or_else(|e| unreachable!("Scheduling logic invalidated! {:?}", e));

        Weight::from_parts(gas_allowance - GasAllowanceOf::<Test>::get(), 0)
    }) - minimal_weight
}

pub fn run_to_block(n: BlockNumber, remaining_weight: Option<u64>) {
    run_to_block_maybe_with_queue(n, remaining_weight, Some(true))
}

pub fn run_to_block_maybe_with_queue(
    n: BlockNumber,
    remaining_weight: Option<u64>,
    gear_run: Option<bool>,
) {
    while System::block_number() < n {
        System::on_finalize(System::block_number());
        GearBank::on_finalize(System::block_number());
        System::set_block_number(System::block_number() + 1);
        System::on_initialize(System::block_number());
        GearGas::on_initialize(System::block_number());
        GearMessenger::on_initialize(System::block_number());
        Gear::on_initialize(System::block_number());
        GearBank::on_initialize(System::block_number());

        if let Some(remaining_weight) = remaining_weight {
            GasAllowanceOf::<Test>::put(remaining_weight);
            let max_block_weight = <BlockWeightsOf<Test> as Get<BlockWeights>>::get().max_block;
            System::register_extra_weight_unchecked(
                max_block_weight.saturating_sub(Weight::from_parts(remaining_weight, 0)),
                DispatchClass::Normal,
            );
        }

        if let Some(process_messages) = gear_run {
            if !process_messages {
                QueueProcessingOf::<Test>::deny();
            }

            // Spend the maximum weight of the block to account for the weight of Gear::run() in the current block.
            let max_block_weight = <BlockWeightsOf<Test> as Get<BlockWeights>>::get().max_block;
            System::register_extra_weight_unchecked(max_block_weight, DispatchClass::Mandatory);

            Gear::run(frame_support::dispatch::RawOrigin::None.into(), None).unwrap();
        }

        Gear::on_finalize(System::block_number());
        GearBank::on_finalize(System::block_number());

        if gear_run.is_some() {
            assert!(!System::events().iter().any(|e| {
                matches!(
                    e.event,
                    RuntimeEvent::Gear(pallet_gear::Event::QueueNotProcessed)
                )
            }))
        }
    }
}

pub fn run_to_next_block(remaining_weight: Option<u64>) {
    run_for_blocks(1, remaining_weight)
}

pub fn run_for_blocks(block_count: BlockNumber, remaining_weight: Option<u64>) {
    run_to_block(System::block_number() + block_count, remaining_weight);
}
