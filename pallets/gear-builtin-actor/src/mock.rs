// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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

use crate as pallet_gear_builtin_actor;
use core::cell::RefCell;
use core_processor::common::{DispatchOutcome, JournalNote};
use frame_election_provider_support::{onchain, SequentialPhragmen, VoteWeight};
use frame_support::{
    construct_runtime, parameter_types,
    traits::{
        ConstBool, ConstU32, ConstU64, Currency, FindAuthor, GenesisBuild, OnFinalize,
        OnInitialize, U128CurrencyToVote,
    },
    PalletId,
};
use frame_support_test::TestRandomness;
use frame_system::{self as system, pallet_prelude::BlockNumberFor, EnsureRoot};
use gear_core::{
    ids::{BuiltinId, MessageId, ProgramId},
    message::{ReplyMessage, ReplyPacket, StoredDispatch},
};
use pallet_gear::{BuiltinActor, RegisteredBuiltinActor};
use pallet_session::historical::{self as pallet_session_historical};
use sp_core::{crypto::key_types, H256};
use sp_runtime::{
    generic,
    testing::UintAuthorityId,
    traits::{BlakeTwo256, IdentityLookup, OpaqueKeys},
    DispatchError, KeyTypeId, Perbill, Percent,
};
use sp_std::convert::{TryFrom, TryInto};

type AccountId = u64;
type BlockNumber = u64;
type Balance = u128;
type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;

pub(crate) const SIGNER: AccountId = 1;
pub(crate) const REWARD_PAYEE: AccountId = 2;
pub(crate) const VAL_1_STASH: AccountId = 10;
pub(crate) const VAL_1_CONTROLLER: AccountId = 11;
pub(crate) const VAL_1_AUTH_ID: UintAuthorityId = UintAuthorityId(12);
pub(crate) const VAL_2_STASH: AccountId = 20;
pub(crate) const VAL_2_CONTROLLER: AccountId = 21;
pub(crate) const VAL_2_AUTH_ID: UintAuthorityId = UintAuthorityId(22);
pub(crate) const VAL_3_STASH: AccountId = 30;
pub(crate) const VAL_3_CONTROLLER: AccountId = 31;
pub(crate) const VAL_3_AUTH_ID: UintAuthorityId = UintAuthorityId(32);
pub(crate) const BLOCK_AUTHOR: AccountId = VAL_1_STASH;

pub(crate) const EXISTENTIAL_DEPOSIT: u128 = 10 * UNITS;
pub(crate) const VALIDATOR_STAKE: u128 = 100 * UNITS;
pub(crate) const ENDOWMENT: u128 = 1_000 * UNITS;

pub(crate) const UNITS: u128 = 1_000_000_000_000; // 10^(-12) precision
const MILLISECS_PER_BLOCK: u64 = 2_400;
pub(crate) const SESSION_DURATION: u64 = 250;

#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct ExecutionTraceFrame {
    pub message_id: MessageId,
    pub source: ProgramId,
    pub actor_id: ProgramId,
    pub builtin_id: BuiltinId,
    pub input: Vec<u8>,
    pub is_success: bool,
}

thread_local! {
    static DEBUG_EXECUTION_TRACE: RefCell<Vec<ExecutionTraceFrame>> = RefCell::new(Vec::new());
    static IN_TRANSACTION: RefCell<bool> = RefCell::new(false);
}

// Configure a mock runtime to test the pallet.
construct_runtime!(
    pub enum Test where
        Block = Block,
        NodeBlock = Block,
        UncheckedExtrinsic = UncheckedExtrinsic,
    {
        System: system::{Pallet, Call, Config, Storage, Event<T>},
        Balances: pallet_balances::{Pallet, Call, Storage, Config<T>, Event<T>},
        Authorship: pallet_authorship::{Pallet, Storage},
        Timestamp: pallet_timestamp::{Pallet, Call, Storage, Inherent},
        Staking: pallet_staking::{Pallet, Call, Storage, Config<T>, Event<T>},
        Session: pallet_session::{Pallet, Call, Storage, Config<T>, Event},
        Historical: pallet_session_historical::{Pallet, Storage},
        BagsList: pallet_bags_list::<Instance1>::{Pallet, Event<T>},
        GearProgram: pallet_gear_program,
        GearMessenger: pallet_gear_messenger,
        GearScheduler: pallet_gear_scheduler,
        GearBank: pallet_gear_bank,
        Gear: pallet_gear,
        GearGas: pallet_gear_gas,
        GearBuiltinActor: pallet_gear_builtin_actor,
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
    pub const Period: u64 = SESSION_DURATION;
    pub const Offset: u64 = SESSION_DURATION + 1;
}

impl pallet_session::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type ValidatorId = AccountId;
    type ValidatorIdOf = pallet_staking::StashOf<Self>;
    type ShouldEndSession = pallet_session::PeriodicSessions<Period, Offset>;
    type NextSessionRotation = pallet_session::PeriodicSessions<Period, Offset>;
    type SessionManager = pallet_session_historical::NoteHistoricalRoot<Self, Staking>;
    type SessionHandler = TestSessionHandler;
    type Keys = UintAuthorityId;
    type WeightInfo = ();
}

impl pallet_session_historical::Config for Test {
    type FullIdentification = pallet_staking::Exposure<AccountId, u128>;
    type FullIdentificationOf = pallet_staking::ExposureOf<Test>;
}

type AuthorityId = AccountId;
pub struct TestSessionHandler;
impl pallet_session::SessionHandler<AuthorityId> for TestSessionHandler {
    const KEY_TYPE_IDS: &'static [KeyTypeId] = &[key_types::DUMMY];

    fn on_new_session<Ks: OpaqueKeys>(
        _changed: bool,
        _validators: &[(AuthorityId, Ks)],
        _queued_validators: &[(AuthorityId, Ks)],
    ) {
    }

    fn on_disabled(_validator_index: u32) {}

    fn on_genesis_session<Ks: OpaqueKeys>(_validators: &[(AuthorityId, Ks)]) {}
}

pub struct DummyEraPayout;
impl pallet_staking::EraPayout<u128> for DummyEraPayout {
    fn era_payout(
        _total_staked: u128,
        total_issuance: u128,
        _era_duration_millis: u64,
    ) -> (u128, u128) {
        // At each era have 1% `total_issuance` increase
        (Percent::from_percent(1) * total_issuance, 0)
    }
}
parameter_types! {
    // 2 sessions in an era
    pub const SessionsPerEra: u32 = 2;
    // 4 eras for unbonding
    pub const BondingDuration: u32 = 4;
    pub const SlashDeferDuration: u32 = 3;
    pub const MaxNominatorRewardedPerValidator: u32 = 256;
    pub const OffendingValidatorsThreshold: Perbill = Perbill::from_percent(17);
    pub const MaxActiveValidators: u32 = 100;
    pub const OffchainRepeat: u64 = 5;
    pub const HistoryDepth: u32 = 84;
    pub const MaxNominations: u32 = 16;
    pub const MaxElectingVoters: u32 = 40_000;
    pub const MaxElectableTargets: u16 = 10_000;
    pub const MaxOnChainElectingVoters: u32 = 500;
    pub const MaxOnChainElectableTargets: u16 = 100;
}

pub struct OnChainSeqPhragmen;
impl onchain::Config for OnChainSeqPhragmen {
    type System = Test;
    type Solver = SequentialPhragmen<AccountId, Perbill>;
    type DataProvider = Staking;
    type WeightInfo = ();
    type MaxWinners = MaxActiveValidators;
    type VotersBound = MaxOnChainElectingVoters;
    type TargetsBound = MaxOnChainElectableTargets;
}

impl pallet_staking::Config for Test {
    type MaxNominations = MaxNominations;
    type Currency = Balances;
    type UnixTime = Timestamp;
    type CurrencyBalance = u128;
    type CurrencyToVote = U128CurrencyToVote;
    type ElectionProvider = onchain::OnChainExecution<OnChainSeqPhragmen>;
    type GenesisElectionProvider = onchain::OnChainExecution<OnChainSeqPhragmen>;
    type RewardRemainder = ();
    type RuntimeEvent = RuntimeEvent;
    type Slash = ();
    type Reward = ();
    type SessionsPerEra = SessionsPerEra;
    type BondingDuration = BondingDuration;
    type SlashDeferDuration = SlashDeferDuration;
    type AdminOrigin = EnsureRoot<AccountId>;
    type SessionInterface = Self;
    type EraPayout = DummyEraPayout;
    type NextNewSession = Session;
    type MaxNominatorRewardedPerValidator = MaxNominatorRewardedPerValidator;
    type OffendingValidatorsThreshold = OffendingValidatorsThreshold;
    type VoterList = BagsList;
    type TargetList = pallet_staking::UseValidatorsMap<Self>;
    type MaxUnlockingChunks = ConstU32<32>;
    type HistoryDepth = HistoryDepth;
    type OnStakerSlash = ();
    type WeightInfo = ();
    type BenchmarkingConfig = pallet_staking::TestBenchmarkingConfig;
}

pub const THRESHOLDS: [u64; 32] = [
    10,
    20,
    40,
    80,
    160,
    320,
    640,
    1_280,
    2_560,
    5_120,
    10_240,
    20_480,
    40_960,
    81_920,
    163_840,
    327_680,
    1_310_720,
    2_621_440,
    5_242_880,
    10_485_760,
    20_971_520,
    41_943_040,
    83_886_080,
    167_772_160,
    335_544_320,
    671_088_640,
    1_342_177_280,
    2_684_354_560,
    5_368_709_120,
    10_737_418_240,
    21_474_836_480,
    42_949_672_960,
];

parameter_types! {
    pub const BagThresholds: &'static [u64] = &THRESHOLDS;
}

impl pallet_bags_list::Config<pallet_bags_list::Instance1> for Test {
    type RuntimeEvent = RuntimeEvent;
    type ScoreProvider = Staking;
    type BagThresholds = BagThresholds;
    type Score = VoteWeight;
    type WeightInfo = ();
}

parameter_types! {
    pub const BlockGasLimit: u64 = 100_000_000_000;
    pub const OutgoingLimit: u32 = 1024;
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

pub struct SuccessBuiltinActor {}
impl BuiltinActor<StoredDispatch, JournalNote> for SuccessBuiltinActor {
    fn handle(
        _builtin_id: BuiltinId,
        dispatch: StoredDispatch,
        _gas_limit: u64,
    ) -> Result<Vec<JournalNote>, DispatchError> {
        let message_id = dispatch.id();
        let origin = dispatch.source();
        let actor_id = dispatch.destination();

        if !in_transaction() {
            DEBUG_EXECUTION_TRACE.with(|d| {
                d.borrow_mut().push(ExecutionTraceFrame {
                    message_id,
                    source: origin,
                    actor_id,
                    builtin_id: <Self as RegisteredBuiltinActor<_, _>>::ID,
                    input: dispatch.message().payload_bytes().to_vec(),
                    is_success: true,
                })
            });
        }

        let mut journal = vec![];

        journal.push(JournalNote::GasBurned {
            message_id,
            amount: 1_000_000,
        });

        // Build the reply message
        let payload = b"Success".to_vec().try_into().expect("Should fit");
        let reply_id = MessageId::generate_reply(message_id);
        let packet = ReplyPacket::new(payload, 0);
        let dispatch =
            ReplyMessage::from_packet(reply_id, packet).into_dispatch(actor_id, origin, message_id);

        journal.push(JournalNote::SendDispatch {
            message_id,
            dispatch,
            delay: 0,
            reservation: None,
        });

        let outcome = DispatchOutcome::Success;
        journal.push(JournalNote::MessageDispatched {
            message_id,
            source: origin,
            outcome,
        });

        journal.push(JournalNote::MessageConsumed(message_id));

        Ok(journal)
    }
}
impl RegisteredBuiltinActor<StoredDispatch, JournalNote> for SuccessBuiltinActor {
    const ID: BuiltinId = BuiltinId(*b"bltn/suc");
}

pub struct ErrorBuiltinActor {}
impl BuiltinActor<StoredDispatch, JournalNote> for ErrorBuiltinActor {
    fn handle(
        _builtin_id: BuiltinId,
        dispatch: StoredDispatch,
        _gas_limit: u64,
    ) -> Result<Vec<JournalNote>, DispatchError> {
        let message_id = dispatch.id();
        let origin = dispatch.source();
        let actor_id = dispatch.destination();

        if !in_transaction() {
            DEBUG_EXECUTION_TRACE.with(|d| {
                d.borrow_mut().push(ExecutionTraceFrame {
                    message_id,
                    source: origin,
                    actor_id,
                    builtin_id: <Self as RegisteredBuiltinActor<_, _>>::ID,
                    input: dispatch.message().payload_bytes().to_vec(),
                    is_success: false,
                })
            });
        }
        Err(DispatchError::Unavailable)
    }
}
impl RegisteredBuiltinActor<StoredDispatch, JournalNote> for ErrorBuiltinActor {
    const ID: BuiltinId = BuiltinId(*b"bltn/err");
}

pallet_gear::impl_config!(
    Test,
    Schedule = GearSchedule,
    BuiltinRegistry = GearBuiltinActor,
    BuiltinActor = (SuccessBuiltinActor, ErrorBuiltinActor),
);

parameter_types! {
    pub const BuiltinActorPalletId: PalletId = PalletId(*b"py/biact");
}

impl pallet_gear_builtin_actor::Config for Test {
    type PalletId = BuiltinActorPalletId;
}

impl<C> frame_system::offchain::SendTransactionTypes<C> for Test
where
    RuntimeCall: From<C>,
{
    type OverarchingCall = RuntimeCall;
    type Extrinsic = UncheckedExtrinsic;
}

pub type ValidatorAccountId = (
    AccountId,       // stash
    AccountId,       // controller
    UintAuthorityId, // authority discovery ID
);

// Build genesis storage according to the mock runtime.
#[derive(Default)]
pub struct ExtBuilder {
    initial_authorities: Vec<ValidatorAccountId>,
    stash: Balance,
    endowed_accounts: Vec<AccountId>,
    endowment: Balance,
    total_supply: Balance,
}

impl ExtBuilder {
    pub fn stash(mut self, s: Balance) -> Self {
        self.stash = s;
        self
    }

    pub fn endowment(mut self, e: Balance) -> Self {
        self.endowment = e;
        self
    }

    pub fn initial_authorities(mut self, authorities: Vec<ValidatorAccountId>) -> Self {
        self.initial_authorities = authorities;
        self
    }

    pub fn endowed_accounts(mut self, accounts: Vec<AccountId>) -> Self {
        self.endowed_accounts = accounts;
        self
    }

    pub fn build(self) -> sp_io::TestExternalities {
        let mut storage = system::GenesisConfig::default()
            .build_storage::<Test>()
            .unwrap();

        let balances: Vec<(AccountId, u128)> = self
            .initial_authorities
            .iter()
            .map(|x| (x.0, self.stash))
            .chain(self.endowed_accounts.iter().map(|k| (*k, self.endowment)))
            .collect();

        pallet_balances::GenesisConfig::<Test> { balances }
            .assimilate_storage(&mut storage)
            .unwrap();

        SessionConfig {
            keys: self
                .initial_authorities
                .iter()
                .map(|x| (x.0, x.0, x.2.clone()))
                .collect(),
        }
        .assimilate_storage(&mut storage)
        .unwrap();

        StakingConfig {
            validator_count: self.initial_authorities.len() as u32,
            minimum_validator_count: self.initial_authorities.len() as u32,
            stakers: self
                .initial_authorities
                .iter()
                .map(|x| {
                    (
                        x.0,
                        x.1,
                        self.stash,
                        pallet_staking::StakerStatus::<AccountId>::Validator,
                    )
                })
                .collect::<Vec<_>>(),
            invulnerables: self.initial_authorities.iter().map(|x| x.0).collect(),
            slash_reward_fraction: Perbill::from_percent(10),
            ..Default::default()
        }
        .assimilate_storage(&mut storage)
        .unwrap();

        GearBuiltinActorConfig {
            builtin_ids: vec![
                <SuccessBuiltinActor as RegisteredBuiltinActor<_, _>>::ID,
                <ErrorBuiltinActor as RegisteredBuiltinActor<_, _>>::ID,
            ],
            ..Default::default()
        }
        .assimilate_storage(&mut storage)
        .unwrap();

        let mut ext: sp_io::TestExternalities = storage.into();

        ext.execute_with(|| {
            let new_blk = 1;
            System::set_block_number(new_blk);
            on_initialize(new_blk);

            // ensure total supply is as expected
            let total_supply = Balances::total_issuance();
            if total_supply < self.total_supply {
                // Mint the difference to SIGNER user
                let diff = self.total_supply.saturating_sub(total_supply);
                let _ = <Balances as Currency<_>>::deposit_creating(&SIGNER, diff);
            }
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
    Session::on_initialize(new_block_number);
    GearGas::on_initialize(new_block_number);
    GearMessenger::on_initialize(new_block_number);
    Gear::on_initialize(new_block_number);
}

// Run on_finalize hooks (in pallets reverse order, as they appear in AllPalletsWithSystem)
pub(crate) fn on_finalize(current_blk: BlockNumberFor<Test>) {
    Staking::on_finalize(current_blk);
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

pub(crate) fn new_test_ext() -> sp_io::TestExternalities {
    let bank_address = <Test as pallet_gear_bank::Config>::BankAddress::get();

    ExtBuilder::default()
        .initial_authorities(vec![
            (VAL_1_STASH, VAL_1_CONTROLLER, VAL_1_AUTH_ID),
            (VAL_2_STASH, VAL_2_CONTROLLER, VAL_2_AUTH_ID),
            (VAL_3_STASH, VAL_3_CONTROLLER, VAL_3_AUTH_ID),
        ])
        .stash(VALIDATOR_STAKE)
        .endowment(ENDOWMENT)
        .endowed_accounts(vec![bank_address, SIGNER, REWARD_PAYEE])
        .build()
}
