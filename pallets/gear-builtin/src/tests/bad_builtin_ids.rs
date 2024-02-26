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
    BuiltinCache = GearBuiltin,
);

pub struct FirstBuiltinActor {}
impl BuiltinActor for FirstBuiltinActor {
    type Error = BuiltinActorError;

    const ID: u64 = 1_u64;

    fn handle(
        _dispatch: &StoredDispatch,
        _gas_limit: u64,
    ) -> (Result<Payload, BuiltinActorError>, u64) {
        let payload = b"Success".to_vec().try_into().expect("Small vector");

        (Ok(payload), 1_000_u64)
    }
}

pub struct SecondBuiltinActor {}
impl BuiltinActor for SecondBuiltinActor {
    type Error = BuiltinActorError;

    const ID: u64 = 2_u64;

    fn handle(
        _dispatch: &StoredDispatch,
        _gas_limit: u64,
    ) -> (Result<Payload, BuiltinActorError>, u64) {
        let payload = b"Success".to_vec().try_into().expect("Small vector");

        (Ok(payload), 1_000_u64)
    }
}

pub struct ThirdBuiltinActor {}
impl BuiltinActor for ThirdBuiltinActor {
    type Error = BuiltinActorError;

    const ID: u64 = 3_u64;

    fn handle(
        _dispatch: &StoredDispatch,
        _gas_limit: u64,
    ) -> (Result<Payload, BuiltinActorError>, u64) {
        let payload = b"Success".to_vec().try_into().expect("Small vector");

        (Ok(payload), 1_000_u64)
    }
}

// Duplicate builtin id: `BuiltinId(2)` already exists.
pub struct DuplicateBuiltinActor {}
impl BuiltinActor for DuplicateBuiltinActor {
    type Error = BuiltinActorError;

    const ID: u64 = 2_u64;

    fn handle(
        _dispatch: &StoredDispatch,
        _gas_limit: u64,
    ) -> (Result<Payload, BuiltinActorError>, u64) {
        let payload = b"Success".to_vec().try_into().expect("Small vector");

        (Ok(payload), 1_000_u64)
    }
}

impl pallet_gear_builtin::Config for Test {
    type Builtins = (
        FirstBuiltinActor,
        SecondBuiltinActor,
        ThirdBuiltinActor,
        DuplicateBuiltinActor,
    );
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

pub(crate) fn new_test_ext() -> sp_io::TestExternalities {
    let bank_address = <Test as pallet_gear_bank::Config>::BankAddress::get();

    ExtBuilder::default()
        .endowment(ENDOWMENT)
        .endowed_accounts(vec![bank_address, SIGNER, BLOCK_AUTHOR])
        .build()
}

pub(crate) fn init_logger() {
    let _ = env_logger::Builder::from_default_env()
        .format_module_path(false)
        .format_level(true)
        .try_init();
}

use crate::mock::{BLOCK_AUTHOR, ENDOWMENT, EXISTENTIAL_DEPOSIT, MILLISECS_PER_BLOCK, SIGNER};
use common::Origin;
use frame_support::assert_ok;

const ARBITRARY_ADDRESS: [u8; 32] =
    hex_literal::hex!("1f81dd2c95c0006c335530c3f1b32d8b1314e08bc940ea26afdbe2af88b0400d");

#[test]
#[should_panic(expected = "Duplicate builtin ids")]
fn queue_processing_panics_on_any_message() {
    init_logger();

    new_test_ext().execute_with(|| {
        let destination: ProgramId = H256::from(ARBITRARY_ADDRESS).cast();

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(SIGNER),
            destination,
            Default::default(),
            10_000_000_000,
            0,
            false,
        ));
        // Expecting panic
        run_to_block(2);
    });
}
