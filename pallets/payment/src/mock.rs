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

use crate::{self as pallet_gear_payment, Config, DelegateFee};
use common::storage::Messenger;
use frame_support::{
    construct_runtime, parameter_types,
    traits::{
        ConstU128, ConstU32, ConstU8, Contains, Currency, FindAuthor, OnFinalize, OnInitialize,
        OnUnbalanced,
    },
    weights::{constants::WEIGHT_REF_TIME_PER_SECOND, ConstantMultiplier, Weight},
    PalletId,
};
use frame_support_test::TestRandomness;
use frame_system::{self as system, mocking, pallet_prelude::BlockNumberFor};
use pallet_gear_voucher::VoucherId;
#[allow(deprecated)]
use pallet_transaction_payment::CurrencyAdapter;
use primitive_types::H256;
use sp_runtime::{
    testing::TestXt,
    traits::{BlakeTwo256, ConstBool, ConstU64, IdentityLookup},
    BuildStorage,
};
use sp_std::{
    convert::{TryFrom, TryInto},
    prelude::*,
};

type Block = mocking::MockBlock<Test>;
type AccountId = u64;
pub type BlockNumber = BlockNumberFor<Test>;
type Balance = u128;

pub const ALICE: AccountId = 1;
pub const BOB: AccountId = 2;
pub const BLOCK_AUTHOR: AccountId = 255;
pub const FEE_PAYER: AccountId = 201;
pub(crate) type MailboxOf<T> = <<T as Config>::Messenger as Messenger>::Mailbox;

// Configure a mock runtime to test the pallet.
construct_runtime!(
    pub enum Test
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
        GearVoucher: pallet_gear_voucher,
        GearBank: pallet_gear_bank,
    }
);

common::impl_pallet_system!(Test, DbWeight = (), BlockWeights = RuntimeBlockWeights);
pallet_gear::impl_config!(Test, Schedule = GearSchedule);
pallet_gear_gas::impl_config!(Test);
common::impl_pallet_balances!(Test);
common::impl_pallet_authorship!(Test);
common::impl_pallet_timestamp!(Test);
pallet_gear_messenger::impl_config!(Test, CurrentBlockNumber = Gear);
pallet_gear_scheduler::impl_config!(Test);
pallet_gear_program::impl_config!(Test);
pallet_gear_bank::impl_config!(Test);

parameter_types! {
    pub const BlockHashCount: BlockNumber = 2_400;
    pub const ExistentialDeposit: Balance = 1;
    pub RuntimeBlockWeights: frame_system::limits::BlockWeights = frame_system::limits::BlockWeights::simple_max(
        Weight::from_parts(WEIGHT_REF_TIME_PER_SECOND / 2, u64::MAX)
    );
    pub ReserveThreshold: BlockNumber = 1;
}

parameter_types! {
    pub const TransactionByteFee: u128 = 1;
    pub const QueueLengthStep: u64 = 5;
}

#[allow(deprecated)]
impl pallet_transaction_payment::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type OnChargeTransaction = CurrencyAdapter<Balances, DealWithFees>;
    type OperationalFeeMultiplier = ConstU8<5>;
    type WeightToFee = ConstantMultiplier<u128, ConstU128<1_000>>;
    type LengthToFee = ConstantMultiplier<u128, ConstU128<1_000>>;
    type FeeMultiplierUpdate = pallet_gear_payment::GearFeeMultiplier<Test, QueueLengthStep>;
}

parameter_types! {
    pub const BlockGasLimit: u64 = 500_000;
    pub const OutgoingLimit: u32 = 1024;
    pub const OutgoingBytesLimit: u32 = 64 * 1024 * 1024;
    pub const PerformanceMultiplier: u32 = 100;
    pub GearSchedule: pallet_gear::Schedule<Test> = <pallet_gear::Schedule<Test>>::default();
    pub RentFreePeriod: BlockNumber = 1_000;
    pub RentCostPerBlock: Balance = 11;
    pub ResumeMinimalPeriod: BlockNumber = 100;
    pub ResumeSessionDuration: BlockNumber = 1_000;
    pub const BankPalletId: PalletId = PalletId(*b"py/gbank");
    pub const GasMultiplier: common::GasMultiplier<Balance, u64> = common::GasMultiplier::ValuePerGas(100);
}

type NegativeImbalance = <Balances as Currency<u64>>::NegativeImbalance;

pub struct DealWithFees;
impl OnUnbalanced<NegativeImbalance> for DealWithFees {
    fn on_unbalanceds(mut fees_then_tips: impl Iterator<Item = NegativeImbalance>) {
        if let Some(fees) = fees_then_tips.next()
            && let Some(author) = Authorship::author()
        {
            Balances::resolve_creating(&author, fees);

            if let Some(tips) = fees_then_tips.next() {
                Balances::resolve_creating(&author, tips);
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

pub struct DelegateFeeAccountBuilder;

// TODO: simplify it (#3640).
impl DelegateFee<RuntimeCall, AccountId> for DelegateFeeAccountBuilder {
    fn delegate_fee(call: &RuntimeCall, who: &AccountId) -> Option<AccountId> {
        match call {
            RuntimeCall::GearVoucher(voucher_call) => voucher_call.get_sponsor(*who),
            _ => None,
        }
    }
}

impl pallet_gear_payment::Config for Test {
    type ExtraFeeCallFilter = ExtraFeeFilter;
    type DelegateFee = DelegateFeeAccountBuilder;
    type Messenger = GearMessenger;
}

parameter_types! {
    pub const VoucherPalletId: PalletId = PalletId(*b"py/vouch");
    pub const MinVoucherDuration: BlockNumber = 5;
    pub const MaxVoucherDuration: BlockNumber = 100_000_000;
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
            (ALICE, 100_000_000_000u128),
            (BOB, 10_000u128),
            (BLOCK_AUTHOR, 1_000u128),
            (FEE_PAYER, 10_000_000u128),
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

pub fn get_last_voucher_id() -> VoucherId {
    System::events()
        .iter()
        .rev()
        .filter_map(|r| {
            if let RuntimeEvent::GearVoucher(e) = r.event.clone() {
                Some(e)
            } else {
                None
            }
        })
        .find_map(|e| match e {
            pallet_gear_voucher::Event::VoucherIssued { voucher_id, .. } => Some(voucher_id),
            _ => None,
        })
        .expect("can't find message send event")
}
