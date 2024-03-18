// This file is part of Gear.

// Copyright (C) 2021-2024 Gear Technologies Inc.
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

use crate::{self as pallet_gear_builtin, BuiltinActor, BuiltinActorError};
use common::{GasProvider, GasTree};
use core::cell::RefCell;
use frame_support::{
    construct_runtime, parameter_types,
    traits::{ConstBool, ConstU64, FindAuthor, OnFinalize, OnInitialize},
};
use frame_support_test::TestRandomness;
use frame_system::{self as system, pallet_prelude::BlockNumberFor};
use gear_core::{
    ids::ProgramId,
    message::{Payload, StoredDispatch},
};
use sp_core::H256;
use sp_runtime::{
    traits::{BlakeTwo256, IdentityLookup},
    BuildStorage,
};
use sp_std::convert::{TryFrom, TryInto};

type AccountId = u64;
type BlockNumber = u64;
type Balance = u128;
type Block = frame_system::mocking::MockBlock<Test>;

pub(crate) type QueueOf<T> = pallet_gear_messenger::Dispatches<T>;
pub(crate) type GasHandlerOf<T> = <<T as pallet_gear::Config>::GasProvider as GasProvider>::GasTree;
pub(crate) type GasTreeOf<T> = pallet_gear_gas::GasNodes<T>;

pub(crate) const SIGNER: AccountId = 1;
pub(crate) const BLOCK_AUTHOR: AccountId = 10;

pub(crate) const EXISTENTIAL_DEPOSIT: u128 = 10 * UNITS;
pub(crate) const ENDOWMENT: u128 = 1_000 * UNITS;

pub(crate) const UNITS: u128 = 1_000_000_000_000; // 10^(-12) precision
pub(crate) const MILLISECS_PER_BLOCK: u64 = 2_400;

#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct ExecutionTraceFrame {
    pub destination: u64,
    pub source: ProgramId,
    pub input: Vec<u8>,
    pub is_success: bool,
}

thread_local! {
    static DEBUG_EXECUTION_TRACE: RefCell<Vec<ExecutionTraceFrame>> = const { RefCell::new(Vec::new()) };
    static IN_TRANSACTION: RefCell<bool> = const { RefCell::new(false) };
}

// Configure a mock runtime to test the pallet.
construct_runtime!(
    pub enum Test
    {
        System: system,
        Balances: pallet_balances,
        Authorship: pallet_authorship,
        Timestamp: pallet_timestamp,
        GearProgram: pallet_gear_program,
        GearMessenger: pallet_gear_messenger,
        GearScheduler: pallet_gear_scheduler,
        GearBank: pallet_gear_bank,
        Gear: pallet_gear,
        GearGas: pallet_gear_gas,
        GearBuiltin: pallet_gear_builtin,
    }
);

parameter_types! {
    pub const BlockHashCount: u64 = 250;
    pub const ExistentialDeposit: Balance = EXISTENTIAL_DEPOSIT;
}

common::impl_pallet_system!(Test);
common::impl_pallet_balances!(Test);
common::impl_pallet_authorship!(Test);
common::impl_pallet_timestamp!(Test);

parameter_types! {
    pub const BlockGasLimit: u64 = 100_000_000_000;
    pub const OutgoingLimit: u32 = 1024;
    pub const OutgoingBytesLimit: u32 = 64 * 1024 * 1024;
    pub ReserveThreshold: BlockNumber = 1;
    pub GearSchedule: pallet_gear::Schedule<Test> = <pallet_gear::Schedule<Test>>::default();
    pub RentFreePeriod: BlockNumber = 12_000;
    pub RentCostPerBlock: Balance = 11;
    pub ResumeMinimalPeriod: BlockNumber = 100;
    pub ResumeSessionDuration: BlockNumber = 1_000;
    pub const PerformanceMultiplier: u32 = 100;
    pub const BankAddress: AccountId = 15082001;
    pub const GasMultiplier: common::GasMultiplier<Balance, u64> = common::GasMultiplier::ValuePerGas(25);
}

pallet_gear_bank::impl_config!(Test);
pallet_gear_gas::impl_config!(Test);
pallet_gear_scheduler::impl_config!(Test);
pallet_gear_program::impl_config!(Test);
pallet_gear_messenger::impl_config!(Test, CurrentBlockNumber = Gear);
pallet_gear::impl_config!(
    Test,
    Schedule = GearSchedule,
    BuiltinDispatcherFactory = GearBuiltin,
);

// A builtin actor who always returns success (even if not enough gas is provided).
pub struct SuccessBuiltinActor {}
impl BuiltinActor for SuccessBuiltinActor {
    type Error = BuiltinActorError;

    const ID: u64 = u64::from_le_bytes(*b"bltn/suc");

    fn handle(
        dispatch: &StoredDispatch,
        _gas_limit: u64,
    ) -> (Result<Payload, BuiltinActorError>, u64) {
        if !in_transaction() {
            DEBUG_EXECUTION_TRACE.with(|d| {
                d.borrow_mut().push(ExecutionTraceFrame {
                    destination: <Self as BuiltinActor>::ID,
                    source: dispatch.source(),
                    input: dispatch.payload_bytes().to_vec(),
                    is_success: true,
                })
            });
        }

        // Build the reply message
        let payload = b"Success".to_vec().try_into().expect("Small vector");

        (Ok(payload), 1_000_000_u64)
    }
}

// A builtin actor that always returns an error.
pub struct ErrorBuiltinActor {}
impl BuiltinActor for ErrorBuiltinActor {
    type Error = BuiltinActorError;

    const ID: u64 = u64::from_le_bytes(*b"bltn/err");

    fn handle(
        dispatch: &StoredDispatch,
        _gas_limit: u64,
    ) -> (Result<Payload, BuiltinActorError>, u64) {
        if !in_transaction() {
            DEBUG_EXECUTION_TRACE.with(|d| {
                d.borrow_mut().push(ExecutionTraceFrame {
                    destination: <Self as BuiltinActor>::ID,
                    source: dispatch.source(),
                    input: dispatch.payload_bytes().to_vec(),
                    is_success: false,
                })
            });
        }
        (Err(BuiltinActorError::InsufficientGas), 100_000_u64)
    }
}

// An honest bulitin actor that actually checks whether the gas is sufficient.
pub struct HonestBuiltinActor {}
impl BuiltinActor for HonestBuiltinActor {
    type Error = BuiltinActorError;

    const ID: u64 = u64::from_le_bytes(*b"bltn/hon");

    fn handle(
        dispatch: &StoredDispatch,
        gas_limit: u64,
    ) -> (Result<Payload, BuiltinActorError>, u64) {
        let is_error = gas_limit < 500_000_u64;

        if !in_transaction() {
            DEBUG_EXECUTION_TRACE.with(|d| {
                d.borrow_mut().push(ExecutionTraceFrame {
                    destination: <Self as BuiltinActor>::ID,
                    source: dispatch.source(),
                    input: dispatch.payload_bytes().to_vec(),
                    is_success: !is_error,
                })
            });
        }

        if is_error {
            return (Err(BuiltinActorError::InsufficientGas), 100_000_u64);
        }

        // Build the reply message
        let payload = b"Success".to_vec().try_into().expect("Small vector");

        (Ok(payload), 500_000_u64)
    }
}

impl pallet_gear_builtin::Config for Test {
    type Builtins = (SuccessBuiltinActor, ErrorBuiltinActor, HonestBuiltinActor);
    type WeightInfo = ();
}

// Build genesis storage according to the mock runtime.
#[derive(Default)]
pub struct ExtBuilder {
    endowed_accounts: Vec<AccountId>,
    endowment: Balance,
}

impl ExtBuilder {
    pub fn endowment(mut self, e: Balance) -> Self {
        self.endowment = e;
        self
    }

    pub fn endowed_accounts(mut self, accounts: Vec<AccountId>) -> Self {
        self.endowed_accounts = accounts;
        self
    }

    pub fn build(self) -> sp_io::TestExternalities {
        let mut storage = system::GenesisConfig::<Test>::default()
            .build_storage()
            .unwrap();

        pallet_balances::GenesisConfig::<Test> {
            balances: self
                .endowed_accounts
                .iter()
                .map(|k| (*k, self.endowment))
                .collect(),
        }
        .assimilate_storage(&mut storage)
        .unwrap();

        let mut ext: sp_io::TestExternalities = storage.into();

        ext.execute_with(|| {
            let new_blk = 1;
            System::set_block_number(new_blk);
            on_initialize(new_blk);
        });
        ext
    }
}

#[allow(unused)]
pub(crate) fn run_to_block(n: u64) {
    while System::block_number() < n {
        let current_blk = System::block_number();

        Gear::run(frame_support::dispatch::RawOrigin::None.into(), None).unwrap();
        on_finalize(current_blk);

        let new_block_number = current_blk + 1;
        System::set_block_number(new_block_number);
        on_initialize(new_block_number);
    }
}

pub(crate) fn run_to_next_block() {
    run_for_n_blocks(1)
}

pub(crate) fn run_for_n_blocks(n: u64) {
    let now = System::block_number();
    let until = now + n;
    for current_blk in now..until {
        Gear::run(frame_support::dispatch::RawOrigin::None.into(), None).unwrap();
        on_finalize(current_blk);

        let new_block_number = current_blk + 1;
        System::set_block_number(new_block_number);
        on_initialize(new_block_number);
    }
}

// Run on_initialize hooks in order as they appear in AllPalletsWithSystem.
pub(crate) fn on_initialize(new_block_number: BlockNumberFor<Test>) {
    Timestamp::set_timestamp(new_block_number.saturating_mul(MILLISECS_PER_BLOCK));
    Authorship::on_initialize(new_block_number);
    GearGas::on_initialize(new_block_number);
    GearMessenger::on_initialize(new_block_number);
    Gear::on_initialize(new_block_number);
}

// Run on_finalize hooks (in pallets reverse order, as they appear in AllPalletsWithSystem)
pub(crate) fn on_finalize(current_blk: BlockNumberFor<Test>) {
    Authorship::on_finalize(current_blk);
    Gear::on_finalize(current_blk);
    assert!(!System::events().iter().any(|e| {
        matches!(
            e.event,
            RuntimeEvent::Gear(pallet_gear::Event::QueueNotProcessed)
        )
    }))
}

pub(crate) fn gas_price(gas: u64) -> u128 {
    <Test as pallet_gear_bank::Config>::GasMultiplier::get().gas_to_value(gas)
}

pub(crate) fn start_transaction() {
    sp_externalities::with_externalities(|ext| ext.storage_start_transaction())
        .expect("externalities should exists");

    set_transaction_flag(true);
}

pub(crate) fn rollback_transaction() {
    sp_externalities::with_externalities(|ext| {
        ext.storage_rollback_transaction()
            .expect("ongoing transaction must be there");
    })
    .expect("externalities should be set");

    set_transaction_flag(false);
}

pub(crate) fn current_stack() -> Vec<ExecutionTraceFrame> {
    DEBUG_EXECUTION_TRACE.with(|stack| stack.borrow().clone())
}

pub(crate) fn in_transaction() -> bool {
    IN_TRANSACTION.with(|value| *value.borrow())
}

pub(crate) fn set_transaction_flag(new_val: bool) {
    IN_TRANSACTION.with(|value| *value.borrow_mut() = new_val)
}

pub(crate) fn message_queue_empty() -> bool {
    QueueOf::<Test>::iter_keys().next().is_none()
}

pub(crate) fn gas_tree_empty() -> bool {
    GasTreeOf::<Test>::iter_keys().next().is_none()
        && <GasHandlerOf<Test> as GasTree>::total_supply() == 0
}

pub(crate) fn new_test_ext() -> sp_io::TestExternalities {
    let bank_address = <Test as pallet_gear_bank::Config>::BankAddress::get();

    ExtBuilder::default()
        .endowment(ENDOWMENT)
        .endowed_accounts(vec![bank_address, SIGNER, BLOCK_AUTHOR])
        .build()
}
