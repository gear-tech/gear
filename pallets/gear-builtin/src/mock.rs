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

use crate::{
    self as pallet_gear_builtin, BuiltinActor, BuiltinActorError, BuiltinActorId, BuiltinActorType,
    BuiltinContext, BuiltinReply, GasAllowanceOf, bls12_381, proxy,
};
use common::{GasProvider, GasTree, Origin, storage::Limiter};
use core::cell::RefCell;
use frame_support::{
    PalletId, construct_runtime,
    pallet_prelude::{DispatchClass, Weight},
    parameter_types,
    traits::{ConstU32, ConstU64, FindAuthor, Get, InstanceFilter, OnFinalize, OnInitialize},
};
use frame_support_test::TestRandomness;
use frame_system::{self as system, limits::BlockWeights, pallet_prelude::BlockNumberFor};
use gbuiltin_proxy::ProxyType as BuiltinProxyType;
use gear_core::{ids::ActorId, message::StoredDispatch};
use parity_scale_codec::{Decode, Encode, MaxEncodedLen};
use sp_core::H256;
use sp_runtime::{
    BuildStorage, Perbill, Permill, RuntimeDebug,
    traits::{BlakeTwo256, IdentityLookup},
};
use sp_std::convert::{TryFrom, TryInto};

type AccountId = u64;
type BlockNumber = u32;
type Balance = u128;
type Block = frame_system::mocking::MockBlockU32<Test>;
type BlockWeightsOf<T> = <T as frame_system::Config>::BlockWeights;

pub(crate) type QueueOf<T> = pallet_gear_messenger::Dispatches<T>;
pub(crate) type GasHandlerOf<T> = <<T as pallet_gear::Config>::GasProvider as GasProvider>::GasTree;
pub(crate) type GasTreeOf<T> = pallet_gear_gas::GasNodes<T>;

pub(crate) const SIGNER: AccountId = 1;
pub(crate) const VAL_1_STASH: AccountId = 10;
pub(crate) const VAL_2_STASH: AccountId = 20;
pub(crate) const VAL_3_STASH: AccountId = 30;
pub(crate) const BLOCK_AUTHOR: AccountId = VAL_1_STASH;

pub(crate) const EXISTENTIAL_DEPOSIT: u128 = 10 * UNITS;
pub(crate) const ENDOWMENT: u128 = 1_000 * UNITS;

pub(crate) const UNITS: u128 = 1_000_000_000_000; // 10^(-12) precision
pub(crate) const MILLISECS_PER_BLOCK: u64 = 2_400;

#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct ExecutionTraceFrame {
    pub destination: u64,
    pub source: ActorId,
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
        Staking: pallet_staking,
        Proxy: pallet_proxy,
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
    pub const BlockHashCount: BlockNumber = 250;
    pub const ExistentialDeposit: Balance = EXISTENTIAL_DEPOSIT;
}

#[derive(
    Default,
    Copy,
    Clone,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Encode,
    Decode,
    RuntimeDebug,
    MaxEncodedLen,
    scale_info::TypeInfo,
)]
pub enum ProxyType {
    #[default]
    Any,
    NonTransfer,
    Governance,
    Staking,
    IdentityJudgement,
    CancelProxy,
}

impl From<BuiltinProxyType> for ProxyType {
    fn from(proxy_type: BuiltinProxyType) -> Self {
        match proxy_type {
            BuiltinProxyType::Any => ProxyType::Any,
            BuiltinProxyType::NonTransfer => ProxyType::NonTransfer,
            BuiltinProxyType::Governance => ProxyType::Governance,
            BuiltinProxyType::Staking => ProxyType::Staking,
            BuiltinProxyType::IdentityJudgement => ProxyType::IdentityJudgement,
            BuiltinProxyType::CancelProxy => ProxyType::CancelProxy,
        }
    }
}

impl InstanceFilter<RuntimeCall> for ProxyType {
    fn filter(&self, c: &RuntimeCall) -> bool {
        match self {
            ProxyType::Any => true,
            ProxyType::NonTransfer => !matches!(c, RuntimeCall::Balances(..)),
            ProxyType::CancelProxy => {
                matches!(
                    c,
                    RuntimeCall::Proxy(pallet_proxy::Call::reject_announcement { .. })
                )
            }
            ProxyType::Staking => matches!(c, RuntimeCall::Staking(..)),
            ProxyType::Governance | ProxyType::IdentityJudgement => {
                unimplemented!("No pallets defined in test runtime")
            }
        }
    }
    fn is_superset(&self, o: &Self) -> bool {
        match (self, o) {
            (x, y) if x == y => true,
            (ProxyType::Any, _) => true,
            (_, ProxyType::Any) => false,
            (ProxyType::NonTransfer, _) => true,
            _ => false,
        }
    }
}

common::impl_pallet_system!(Test);
common::impl_pallet_balances!(Test);
common::impl_pallet_authorship!(Test);
common::impl_pallet_timestamp!(Test);
common::impl_pallet_staking!(Test);

parameter_types! {
    pub const ProxyDepositBase: Balance = 1;
    pub const ProxyDepositFactor: Balance = 1;
    pub const MaxProxies: u32 = 100;
    pub const MaxPending: u32 = 100;
    pub const AnnouncementDepositBase: Balance = 1;
    pub const AnnouncementDepositFactor: Balance = 1;
}

impl pallet_proxy::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type RuntimeCall = RuntimeCall;
    type Currency = Balances;
    type ProxyType = ProxyType;
    type ProxyDepositBase = ProxyDepositBase;
    type ProxyDepositFactor = ProxyDepositFactor;
    type MaxProxies = MaxProxies;
    type WeightInfo = ();
    type MaxPending = MaxPending;
    type CallHasher = BlakeTwo256;
    type AnnouncementDepositBase = AnnouncementDepositBase;
    type AnnouncementDepositFactor = AnnouncementDepositBase;
}

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
    pub const BankPalletId: PalletId = PalletId(*b"py/gbank");
    pub const GasMultiplier: common::GasMultiplier<Balance, u64> = common::GasMultiplier::ValuePerGas(100);
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
    const TYPE: BuiltinActorType =
        BuiltinActorType::Custom(BuiltinActorId::new(b"success-actor", 1));

    fn handle(
        dispatch: &StoredDispatch,
        context: &mut BuiltinContext,
    ) -> Result<BuiltinReply, BuiltinActorError> {
        if !in_transaction() {
            let builtin_id =
                GearBuiltin::builtin_id_into_actor_id(<Self as BuiltinActor>::TYPE.id()).cast();
            DEBUG_EXECUTION_TRACE.with(|d| {
                d.borrow_mut().push(ExecutionTraceFrame {
                    destination: builtin_id,
                    source: dispatch.source(),
                    input: dispatch.payload_bytes().to_vec(),
                    is_success: true,
                })
            });
        }

        // Build the reply message
        let payload = b"Success".to_vec().try_into().expect("Small vector");
        context.try_charge_gas(1_000_000_u64)?;

        Ok(BuiltinReply {
            payload,
            value: dispatch.value(),
        })
    }

    fn max_gas() -> u64 {
        Default::default()
    }
}

// A builtin actor that always returns an error.
pub struct ErrorBuiltinActor {}
impl BuiltinActor for ErrorBuiltinActor {
    const TYPE: BuiltinActorType = BuiltinActorType::Custom(BuiltinActorId::new(b"error-actor", 1));

    fn handle(
        dispatch: &StoredDispatch,
        context: &mut BuiltinContext,
    ) -> Result<BuiltinReply, BuiltinActorError> {
        if !in_transaction() {
            let builtin_id =
                GearBuiltin::builtin_id_into_actor_id(<Self as BuiltinActor>::TYPE.id()).cast();

            DEBUG_EXECUTION_TRACE.with(|d| {
                d.borrow_mut().push(ExecutionTraceFrame {
                    destination: builtin_id,
                    source: dispatch.source(),
                    input: dispatch.payload_bytes().to_vec(),
                    is_success: false,
                })
            });
        }
        context.try_charge_gas(100_000_u64)?;
        Err(BuiltinActorError::InsufficientGas)
    }

    fn max_gas() -> u64 {
        Default::default()
    }
}

// An honest bulitin actor that actually checks whether the gas is sufficient.
pub struct HonestBuiltinActor {}
impl BuiltinActor for HonestBuiltinActor {
    const TYPE: BuiltinActorType =
        BuiltinActorType::Custom(BuiltinActorId::new(b"honest-actor", 1));

    fn handle(
        dispatch: &StoredDispatch,
        context: &mut BuiltinContext,
    ) -> Result<BuiltinReply, BuiltinActorError> {
        let is_error = context.to_gas_amount().left() < 500_000_u64;

        if !in_transaction() {
            let builtin_id =
                GearBuiltin::builtin_id_into_actor_id(<Self as BuiltinActor>::TYPE.id()).cast();

            DEBUG_EXECUTION_TRACE.with(|d| {
                d.borrow_mut().push(ExecutionTraceFrame {
                    destination: builtin_id,
                    source: dispatch.source(),
                    input: dispatch.payload_bytes().to_vec(),
                    is_success: !is_error,
                })
            });
        }

        if is_error {
            context.try_charge_gas(100_000_u64)?;
            return Err(BuiltinActorError::InsufficientGas);
        }

        // Build the reply message
        let payload = b"Success".to_vec().try_into().expect("Small vector");
        context.try_charge_gas(500_000_u64)?;

        Ok(BuiltinReply {
            payload,
            value: dispatch.value(),
        })
    }

    fn max_gas() -> u64 {
        Default::default()
    }
}

impl pallet_gear_builtin::Config for Test {
    type RuntimeCall = RuntimeCall;
    type Builtins = (
        SuccessBuiltinActor,
        ErrorBuiltinActor,
        HonestBuiltinActor,
        bls12_381::Actor<Self>,
        proxy::Actor<Self>,
    );
    type BlockLimiter = GearGas;
    type WeightInfo = ();
}

// Build genesis storage according to the mock runtime.
#[derive(Default)]
pub struct ExtBuilder {
    initial_authorities: Vec<AccountId>,
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

        pallet_staking::GenesisConfig::<Test> {
            validator_count: self.initial_authorities.len() as u32,
            minimum_validator_count: self.initial_authorities.len() as u32,
            stakers: self
                .initial_authorities
                .iter()
                .map(|x| {
                    (
                        *x,
                        *x,
                        self.endowment,
                        pallet_staking::StakerStatus::<AccountId>::Validator,
                    )
                })
                .collect::<Vec<_>>(),
            invulnerables: self.initial_authorities.to_vec(),
            slash_reward_fraction: Perbill::from_percent(10),
            ..Default::default()
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
pub(crate) fn run_to_block(n: BlockNumber) {
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
    run_for_n_blocks(1, None)
}

pub(crate) fn run_for_n_blocks(n: BlockNumber, remaining_weight: Option<u64>) {
    let now = System::block_number();
    let until = now + n;
    for current_blk in now..until {
        if let Some(remaining_weight) = remaining_weight {
            GasAllowanceOf::<Test>::put(remaining_weight);
            let max_block_weight = <BlockWeightsOf<Test> as Get<BlockWeights>>::get().max_block;
            System::register_extra_weight_unchecked(
                max_block_weight.saturating_sub(Weight::from_parts(remaining_weight, 0)),
                DispatchClass::Normal,
            );
        }

        let max_block_weight = <BlockWeightsOf<Test> as Get<BlockWeights>>::get().max_block;
        System::register_extra_weight_unchecked(max_block_weight, DispatchClass::Mandatory);
        Gear::run(frame_support::dispatch::RawOrigin::None.into(), None).unwrap();

        on_finalize(current_blk);

        let new_block_number = current_blk + 1;
        System::set_block_number(new_block_number);
        on_initialize(new_block_number);
    }
}

// Run on_initialize hooks in order as they appear in AllPalletsWithSystem.
pub(crate) fn on_initialize(new_block_number: BlockNumberFor<Test>) {
    Timestamp::set_timestamp(u64::from(new_block_number).saturating_mul(MILLISECS_PER_BLOCK));
    Authorship::on_initialize(new_block_number);
    GearGas::on_initialize(new_block_number);
    GearMessenger::on_initialize(new_block_number);
    Gear::on_initialize(new_block_number);
    GearBank::on_initialize(new_block_number);
}

// Run on_finalize hooks (in pallets reverse order, as they appear in AllPalletsWithSystem)
pub(crate) fn on_finalize(current_blk: BlockNumberFor<Test>) {
    Authorship::on_finalize(current_blk);
    Gear::on_finalize(current_blk);
    GearBank::on_finalize(current_blk);
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
    let bank_address = GearBank::bank_address();

    let mut endowed_accounts = vec![bank_address, SIGNER, BLOCK_AUTHOR];
    endowed_accounts.extend(GearBuiltin::list_builtins());

    ExtBuilder::default()
        .endowment(ENDOWMENT)
        .endowed_accounts(endowed_accounts)
        .build()
}
