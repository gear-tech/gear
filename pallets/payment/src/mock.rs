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

use crate as pallet_gear_payment;
use frame_support::{
    construct_runtime, parameter_types,
    traits::{
        ConstU128, ConstU8, Contains, Currency, FindAuthor, OnFinalize, OnInitialize, OnUnbalanced,
    },
    weights::{constants::WEIGHT_REF_TIME_PER_SECOND, ConstantMultiplier, Weight},
};
use frame_support_test::TestRandomness;
use frame_system as system;
use pallet_transaction_payment::CurrencyAdapter;
use primitive_types::H256;
use sp_runtime::{
    testing::{Header, TestXt},
    traits::{BlakeTwo256, ConstU64, IdentityLookup},
};
use sp_std::{
    convert::{TryFrom, TryInto},
    prelude::*,
};

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;

pub const ALICE: u64 = 1;
pub const BLOCK_AUTHOR: u64 = 255;

// Configure a mock runtime to test the pallet.
construct_runtime!(
    pub enum Test where
        Block = Block,
        NodeBlock = Block,
        UncheckedExtrinsic = UncheckedExtrinsic,
    {
        System: system,
        Gear: pallet_gear,
        GearGas: pallet_gear_gas,
        Balances: pallet_balances,
        Authorship: pallet_authorship,
        TransactionPayment: pallet_transaction_payment,
        Timestamp: pallet_timestamp,
        GearMessenger: pallet_gear_messenger,
        GearScheduler: pallet_gear_scheduler,
        GearPayment: pallet_gear_payment,
        GearProgram: pallet_gear_program,
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

parameter_types! {
    pub const BlockHashCount: u64 = 2400;
    pub const SS58Prefix: u8 = 42;
    pub const ExistentialDeposit: u64 = 1;
    pub RuntimeBlockWeights: frame_system::limits::BlockWeights = frame_system::limits::BlockWeights::simple_max(
        Weight::from_parts(WEIGHT_REF_TIME_PER_SECOND / 2, u64::MAX)
    );
}

impl system::Config for Test {
    type BaseCallFilter = frame_support::traits::Everything;
    type BlockWeights = RuntimeBlockWeights;
    type BlockLength = ();
    type DbWeight = ();
    type RuntimeOrigin = RuntimeOrigin;
    type RuntimeCall = RuntimeCall;
    type Index = u64;
    type BlockNumber = u64;
    type Hash = H256;
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

parameter_types! {
    pub const TransactionByteFee: u128 = 1;
    pub const QueueLengthStep: u64 = 5;
}

impl pallet_transaction_payment::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type OnChargeTransaction = CurrencyAdapter<Balances, DealWithFees>;
    type OperationalFeeMultiplier = ConstU8<5>;
    type WeightToFee = ConstantMultiplier<u128, ConstU128<1_000>>;
    type LengthToFee = ConstantMultiplier<u128, ConstU128<1_000>>;
    type FeeMultiplierUpdate = pallet_gear_payment::GearFeeMultiplier<Test, QueueLengthStep>;
}

pub struct GasConverter;
impl common::GasPrice for GasConverter {
    type Balance = u128;
    type GasToBalanceMultiplier = ConstU128<1_000>;
}

parameter_types! {
    pub const BlockGasLimit: u64 = 500_000;
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

impl pallet_gear_program::Config for Test {}

impl pallet_gear_gas::Config for Test {
    type BlockGasLimit = BlockGasLimit;
}

impl pallet_gear_scheduler::Config for Test {
    type BlockLimiter = GearGas;
    type ReserveThreshold = ConstU64<1>;
    type WaitlistCost = ConstU64<100>;
    type MailboxCost = ConstU64<100>;
    type ReservationCost = ConstU64<100>;
    type DispatchHoldCost = ConstU64<100>;
}

impl pallet_gear_messenger::Config for Test {
    type BlockLimiter = GearGas;
    type CurrentBlockNumber = Gear;
}

type NegativeImbalance = <Balances as Currency<u64>>::NegativeImbalance;

pub struct DealWithFees;
impl OnUnbalanced<NegativeImbalance> for DealWithFees {
    fn on_unbalanceds<B>(mut fees_then_tips: impl Iterator<Item = NegativeImbalance>) {
        if let Some(fees) = fees_then_tips.next() {
            if let Some(author) = Authorship::author() {
                Balances::resolve_creating(&author, fees);
            }
            if let Some(tips) = fees_then_tips.next() {
                if let Some(author) = Authorship::author() {
                    Balances::resolve_creating(&author, tips);
                }
            }
        }
    }
}

pub struct ExtraFeeFilter;
impl Contains<RuntimeCall> for ExtraFeeFilter {
    fn contains(call: &RuntimeCall) -> bool {
        // Calls that affect message queue and are subject to extra fee
        matches!(
            call,
            RuntimeCall::Gear(pallet_gear::Call::create_program { .. })
                | RuntimeCall::Gear(pallet_gear::Call::upload_program { .. })
                | RuntimeCall::Gear(pallet_gear::Call::send_message { .. })
                | RuntimeCall::Gear(pallet_gear::Call::send_reply { .. })
        )
    }
}

impl pallet_gear_payment::Config for Test {
    type ExtraFeeCallFilter = ExtraFeeFilter;
    type Messenger = GearMessenger;
}

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
    let mut t = system::GenesisConfig::default()
        .build_storage::<Test>()
        .unwrap();

    pallet_balances::GenesisConfig::<Test> {
        balances: vec![(ALICE, 100_000_000_000u128), (BLOCK_AUTHOR, 1_000u128)],
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

pub fn run_to_block(n: u64) {
    let now = System::block_number();
    for i in now + 1..=n {
        System::on_finalize(i - 1);
        System::set_block_number(i);
        System::on_initialize(i);
        TransactionPayment::on_finalize(i);
    }
}

impl common::ExtractCall<RuntimeCall> for TestXt<RuntimeCall, ()> {
    fn extract_call(&self) -> RuntimeCall {
        self.call.clone()
    }
}
