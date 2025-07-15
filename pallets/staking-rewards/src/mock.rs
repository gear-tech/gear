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

use crate::{self as pallet_gear_staking_rewards, CurrencyOf};
use core::marker::PhantomData;
use frame_election_provider_support::{
    bounds::ElectionBoundsBuilder, onchain, ElectionDataProvider, SequentialPhragmen,
};
use frame_support::{
    construct_runtime, parameter_types,
    traits::{
        tokens::{PayFromAccount, UnityAssetBalanceConversion},
        ConstU32, ConstU64, Contains, Currency, FindAuthor, Hooks, NeverEnsureOrigin,
    },
    weights::{constants::RocksDbWeight, Weight},
    PalletId,
};
use frame_system::{self as system, pallet_prelude::BlockNumberFor, EnsureRoot};
use pallet_election_provider_multi_phase::{self as multi_phase};
use pallet_session::historical::{self as pallet_session_historical};
use sp_core::{crypto::key_types, H256};
use sp_runtime::{
    generic::UncheckedExtrinsic,
    testing::{Block as TestBlock, UintAuthorityId},
    traits::{BlakeTwo256, IdentityLookup, One, OpaqueKeys, Scale},
    BuildStorage, KeyTypeId, Perbill, Percent, Permill, Perquintill,
};
use sp_std::convert::{TryFrom, TryInto};

pub(crate) type TxExtension = (pallet_gear_staking_rewards::StakingBlackList<Test>,);
type TestXt = sp_runtime::testing::TestXt<RuntimeCall, TxExtension>;
type Block = TestBlock<TestXt>;
type AccountId = u64;
pub type BlockNumber = BlockNumberFor<Test>;
type Balance = u128;

pub(crate) const SIGNER: AccountId = 1;
pub(crate) const VAL_1_STASH: AccountId = 10;
pub(crate) const BLOCK_AUTHOR: AccountId = VAL_1_STASH;
pub(crate) const VAL_1_AUTH_ID: UintAuthorityId = UintAuthorityId(12);
pub(crate) const VAL_2_STASH: AccountId = 20;
pub(crate) const VAL_2_AUTH_ID: UintAuthorityId = UintAuthorityId(22);
pub(crate) const VAL_3_STASH: AccountId = 30;
pub(crate) const VAL_3_AUTH_ID: UintAuthorityId = UintAuthorityId(32);
pub(crate) const NOM_1_STASH: AccountId = 40;
pub(crate) const ROOT: AccountId = 101;

pub(crate) const INITIAL_TOTAL_TOKEN_SUPPLY: u128 = 1_000_000 * UNITS;
pub(crate) const EXISTENTIAL_DEPOSIT: u128 = 10 * UNITS / 100_000; // 10
pub(crate) const VALIDATOR_STAKE: u128 = 100 * UNITS; // 10
pub(crate) const ENDOWMENT: u128 = 100 * UNITS;

pub(crate) const UNITS: u128 = 100_000; // 10^(-5) precision
pub(crate) const MILLISECONDS_PER_YEAR: u64 = 1_000 * 3_600 * 24 * 36_525 / 100;
pub(crate) const MILLISECS_PER_BLOCK: u64 = 2_400;
pub(crate) const SESSION_DURATION: u64 = 1000;

// Configure a mock runtime to test the pallet.
construct_runtime!(
    pub enum Test
    {
        System: system,
        Timestamp: pallet_timestamp,
        Authorship: pallet_authorship,
        Balances: pallet_balances,
        Staking: pallet_staking,
        Session: pallet_session,
        Historical: pallet_session_historical,
        Treasury: pallet_treasury,
        Sudo: pallet_sudo,
        Utility: pallet_utility,
        ElectionProviderMultiPhase: multi_phase,
        StakingRewards: pallet_gear_staking_rewards,
    }
);

common::impl_pallet_system!(Test, DbWeight = RocksDbWeight, BlockWeights = ());
common::impl_pallet_balances!(Test);
common::impl_pallet_authorship!(Test, EventHandler = Staking);
common::impl_pallet_timestamp!(Test);
common::impl_pallet_staking!(
    Test,
    EraPayout = StakingRewards,
    Slash = Treasury,
    Reward = StakingRewards,
    NextNewSession = Session,
    ElectionProvider = onchain::OnChainExecution<OnChainSeqPhragmen<Test>>,
    GenesisElectionProvider = onchain::OnChainExecution<OnChainSeqPhragmen<Test>>,
);

parameter_types! {
    pub const BlockHashCount: BlockNumber = 250;
    pub const ExistentialDeposit: Balance = EXISTENTIAL_DEPOSIT;
}

impl pallet_sudo::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type RuntimeCall = RuntimeCall;
    type WeightInfo = ();
}

impl pallet_utility::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type RuntimeCall = RuntimeCall;
    type WeightInfo = ();
    type PalletsOrigin = OriginCaller;
}

// Filter that matches `pallet_staking::Pallet<T>::bond()` call
pub struct BondCallFilter;
impl Contains<RuntimeCall> for BondCallFilter {
    fn contains(call: &RuntimeCall) -> bool {
        match call {
            RuntimeCall::Staking(pallet_staking::Call::bond { .. }) => true,
            RuntimeCall::Utility(utility_call) => {
                match utility_call {
                    pallet_utility::Call::batch { calls }
                    | pallet_utility::Call::batch_all { calls }
                    | pallet_utility::Call::force_batch { calls } => {
                        for c in calls {
                            if Self::contains(c) {
                                return true;
                            }
                        }
                    }
                    pallet_utility::Call::as_derivative { call, .. }
                    | pallet_utility::Call::dispatch_as { call, .. }
                    | pallet_utility::Call::with_weight { call, .. } => {
                        return Self::contains(call);
                    }
                    _ => (),
                }
                false
            }
            _ => false,
        }
    }
}

// Filter that matches accounts for which staking is disabled
pub struct NonStakingAccountsFilter;
impl Contains<AccountId> for NonStakingAccountsFilter {
    fn contains(account: &AccountId) -> bool {
        StakingRewards::filtered_accounts().contains(account)
    }
}

parameter_types! {
    pub const StakingRewardsPalletId: PalletId = PalletId(*b"py/strwd");
    pub const MinInflation: Perquintill = Perquintill::from_percent(1);
    pub const MaxROI: Perquintill = Perquintill::from_percent(30);
    pub const Falloff: Perquintill = Perquintill::from_percent(2);
    pub const MillisecondsPerYear: u64 = 1000 * 3600 * 24 * 36525 / 100;

}
impl pallet_gear_staking_rewards::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type BondCallFilter = BondCallFilter;
    type AccountFilter = NonStakingAccountsFilter;
    type PalletId = StakingRewardsPalletId;
    type RefillOrigin = EnsureRoot<AccountId>;
    type WithdrawOrigin = EnsureRoot<AccountId>;
    type MillisecondsPerYear = MillisecondsPerYear;
    type MinInflation = MinInflation;
    type MaxROI = MaxROI;
    type Falloff = Falloff;
    type WeightInfo = ();
}

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

parameter_types! {
    pub const MaxActiveValidators: u32 = 100;
    pub const OffchainRepeat: u64 = 5;
    pub const MaxElectingVoters: u32 = 40_000;
    pub const MaxElectableTargets: u16 = 10_000;
    pub const MaxOnChainElectingVoters: u32 = 500;
    pub const MaxOnChainElectableTargets: u16 = 100;
    pub ElectionBounds: frame_election_provider_support::bounds::ElectionBounds =
        ElectionBoundsBuilder::default().voters_count(MaxElectingVoters::get().into()).build();
}

frame_election_provider_support::generate_solution_type!(
    #[compact]
    pub struct TestNposSolution::<
        VoterIndex = u32,
        TargetIndex = u16,
        Accuracy = sp_runtime::PerU16,
        MaxVoters = ConstU32::<2_000>,
    >(16)
);

pub struct OnChainSeqPhragmen<T: frame_system::Config + pallet_staking::Config>(PhantomData<T>);
impl<T: frame_system::Config + pallet_staking::Config> onchain::Config for OnChainSeqPhragmen<T> {
    type System = T;
    type Solver = SequentialPhragmen<<T as frame_system::Config>::AccountId, Perbill>;
    type DataProvider = pallet_staking::Pallet<T>;
    type WeightInfo = ();
    type MaxWinners = MaxActiveValidators;
    type Bounds = ElectionBounds;
}

parameter_types! {
    pub const ProposalBond: Permill = Permill::from_percent(5);
    pub const ProposalBondMinimum: u128 = UNITS;
    pub const SpendPeriod: u32 = 100;
    pub const Burn: Permill = Permill::from_percent(50);
    pub const TreasuryPalletId: PalletId = PalletId(*b"py/trsry");
    pub TreasuryAccount: AccountId = Treasury::account_id();
    pub const MaxApprovals: u32 = 100;
}

impl pallet_treasury::Config for Test {
    type PalletId = TreasuryPalletId;
    type Currency = Balances;
    type RejectOrigin = EnsureRoot<AccountId>;
    type RuntimeEvent = RuntimeEvent;
    type SpendPeriod = SpendPeriod;
    type Burn = Burn;
    type BurnDestination = ();
    type SpendFunds = ();
    type WeightInfo = ();
    type MaxApprovals = MaxApprovals;
    type SpendOrigin = NeverEnsureOrigin<u128>;
    type AssetKind = ();
    type Beneficiary = Self::AccountId;
    type BeneficiaryLookup = IdentityLookup<Self::Beneficiary>;
    type Paymaster = PayFromAccount<Balances, TreasuryAccount>;
    type BalanceConverter = UnityAssetBalanceConversion;
    type PayoutPeriod = ConstU64<10>;
    type BlockNumberProvider = System;
}

parameter_types! {
    // phase durations. 1/4 of the last session for each.
    pub static SignedPhase: u64 = SESSION_DURATION / 4;
    pub static UnsignedPhase: u64 = SESSION_DURATION / 4;

    // signed config
    pub static SignedRewardBase: Balance = 50 * UNITS;
    pub static SignedDepositBase: Balance = 50 * UNITS;
    pub static SignedDepositByte: Balance = 0;
    pub static SignedMaxSubmissions: u32 = 5;
    pub static SignedMaxRefunds: u32 = 2;
    pub BetterUnsignedThreshold: Perbill = Perbill::zero();
    pub SignedMaxWeight: Weight = Weight::from_parts(u64::MAX, u64::MAX);

    pub static MaxVotesPerVoter: u32 = 16;
    pub static SignedFixedDeposit: Balance = 1;
    pub static SignedDepositIncreaseFactor: Percent = Percent::from_percent(10);

    // miner configs
    pub static MinerTxPriority: u64 = 100;
    pub static MinerMaxWeight: Weight = Weight::from_parts(u64::MAX, u64::MAX);
    pub static MinerMaxLength: u32 = 256;
}

impl multi_phase::MinerConfig for Test {
    type AccountId = AccountId;
    type MaxLength = MinerMaxLength;
    type MaxWeight = MinerMaxWeight;
    type MaxVotesPerVoter = <Staking as ElectionDataProvider>::MaxVotesPerVoter;
    type MaxWinners = MaxActiveValidators;
    type Solution = TestNposSolution;

    fn solution_weight(v: u32, t: u32, a: u32, d: u32) -> Weight {
        <<Self as multi_phase::Config>::WeightInfo as multi_phase::WeightInfo>::submit_unsigned(
            v, t, a, d,
        )
    }
}

pub struct TestBenchmarkingConfig;
impl multi_phase::BenchmarkingConfig for TestBenchmarkingConfig {
    const VOTERS: [u32; 2] = [1000, 2000];
    const TARGETS: [u32; 2] = [500, 1000];
    const ACTIVE_VOTERS: [u32; 2] = [500, 800];
    const DESIRED_TARGETS: [u32; 2] = [200, 400];
    const SNAPSHOT_MAXIMUM_VOTERS: u32 = 1000;
    const MINER_MAXIMUM_VOTERS: u32 = 1000;
    const MAXIMUM_TARGETS: u32 = 300;
}

impl multi_phase::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type EstimateCallFee = ConstU32<1_000>;
    type SignedPhase = SignedPhase;
    type UnsignedPhase = UnsignedPhase;
    type BetterSignedThreshold = ();
    type OffchainRepeat = OffchainRepeat;
    type MinerTxPriority = MinerTxPriority;
    type SignedRewardBase = SignedRewardBase;
    type SignedDepositBase =
        multi_phase::GeometricDepositBase<Balance, SignedFixedDeposit, SignedDepositIncreaseFactor>;
    type SignedDepositByte = ();
    type SignedDepositWeight = ();
    type SignedMaxWeight = SignedMaxWeight;
    type SignedMaxSubmissions = SignedMaxSubmissions;
    type SignedMaxRefunds = SignedMaxRefunds;
    type SlashHandler = Treasury;
    type RewardHandler = StakingRewards;
    type DataProvider = Staking;
    type Fallback = onchain::OnChainExecution<OnChainSeqPhragmen<Self>>;
    type GovernanceFallback = onchain::OnChainExecution<OnChainSeqPhragmen<Self>>;
    type ForceOrigin = frame_system::EnsureRoot<AccountId>;
    type MaxWinners = MaxActiveValidators;
    type ElectionBounds = ElectionBounds;
    type WeightInfo = ();
    type BenchmarkingConfig = TestBenchmarkingConfig;
    type MinerConfig = Self;
    type Solver = SequentialPhragmen<AccountId, multi_phase::SolutionAccuracyOf<Self>, ()>;
}

impl<C> frame_system::offchain::CreateTransactionBase<C> for Test
where
    RuntimeCall: From<C>,
{
    type RuntimeCall = RuntimeCall;
    type Extrinsic = TestXt;
}

impl<LocalCall> frame_system::offchain::CreateInherent<LocalCall> for Test
where
    RuntimeCall: From<LocalCall>,
{
    fn create_inherent(call: Self::RuntimeCall) -> Self::Extrinsic {
        UncheckedExtrinsic::new_bare(call)
    }
}

pub type ValidatorAccountId = (
    AccountId,       // stash
    UintAuthorityId, // authority discovery ID
);

// Build genesis storage according to the mock runtime.
pub struct ExtBuilder<T> {
    initial_authorities: Vec<ValidatorAccountId>,
    stash: Balance,
    endowed_accounts: Vec<AccountId>,
    endowment: Balance,
    root: Option<AccountId>,
    total_supply: Balance,
    non_stakeable: Perquintill,
    pool_balance: Balance,
    ideal_stake: Perquintill,
    target_inflation: Perquintill,
    filtered_accounts: Vec<AccountId>,
    _phantom: PhantomData<T>,
}

impl<T> Default for ExtBuilder<T> {
    fn default() -> Self {
        Self {
            initial_authorities: vec![],
            stash: 0,
            endowed_accounts: vec![],
            endowment: 0,
            root: None,
            total_supply: 0,
            non_stakeable: Default::default(),
            pool_balance: 0,
            ideal_stake: Default::default(),
            target_inflation: Default::default(),
            filtered_accounts: vec![],
            _phantom: Default::default(),
        }
    }
}

impl<T> ExtBuilder<T>
where
    T: frame_system::Config<AccountId = AccountId>,
    T: pallet_balances::Config<Balance = Balance>,
    T: pallet_treasury::Config,
    T: pallet_session::Config<Keys = UintAuthorityId, ValidatorId = AccountId>,
    T: pallet_sudo::Config,
    T: pallet_staking::Config<CurrencyBalance = Balance>,
    T: pallet_gear_staking_rewards::Config,
    T: pallet_timestamp::Config,
    T: pallet_authorship::Config,
    T: pallet_election_provider_multi_phase::Config,
{
    pub fn stash(mut self, s: Balance) -> Self {
        self.stash = s;
        self
    }

    pub fn endowment(mut self, e: Balance) -> Self {
        self.endowment = e;
        self
    }

    pub fn root(mut self, a: AccountId) -> Self {
        self.root = Some(a);
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

    pub fn total_supply(mut self, e: Balance) -> Self {
        self.total_supply = e;
        self
    }

    pub fn non_stakeable(mut self, q: Perquintill) -> Self {
        self.non_stakeable = q;
        self
    }

    pub fn pool_balance(mut self, e: Balance) -> Self {
        self.pool_balance = e;
        self
    }

    pub fn ideal_stake(mut self, q: Perquintill) -> Self {
        self.ideal_stake = q;
        self
    }

    pub fn target_inflation(mut self, q: Perquintill) -> Self {
        self.target_inflation = q;
        self
    }

    pub fn filtered_accounts(mut self, accounts: Vec<AccountId>) -> Self {
        self.filtered_accounts = accounts;
        self
    }

    pub fn build(self) -> sp_io::TestExternalities {
        let mut storage = system::GenesisConfig::<T>::default()
            .build_storage()
            .unwrap();

        let balances: Vec<(AccountId, u128)> = self
            .initial_authorities
            .iter()
            .map(|x| (x.0, self.stash))
            .chain(self.endowed_accounts.iter().map(|k| (*k, self.endowment)))
            .collect();

        pallet_balances::GenesisConfig::<T> { balances }
            .assimilate_storage(&mut storage)
            .unwrap();

        pallet_treasury::GenesisConfig::<T>::default()
            .assimilate_storage(&mut storage)
            .unwrap();

        pallet_session::GenesisConfig::<T> {
            keys: self
                .initial_authorities
                .iter()
                .map(|x| (x.0, x.0, x.1.clone()))
                .collect(),
            ..Default::default()
        }
        .assimilate_storage(&mut storage)
        .unwrap();

        pallet_sudo::GenesisConfig::<T> { key: self.root }
            .assimilate_storage(&mut storage)
            .unwrap();

        pallet_staking::GenesisConfig::<T> {
            validator_count: self.initial_authorities.len() as u32,
            minimum_validator_count: self.initial_authorities.len() as u32,
            stakers: self
                .initial_authorities
                .iter()
                .map(|x| {
                    (
                        x.0,
                        x.0,
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

        pallet_gear_staking_rewards::GenesisConfig::<T> {
            pool_balance: self.pool_balance,
            non_stakeable: self.non_stakeable,
            ideal_stake: self.ideal_stake,
            target_inflation: self.target_inflation,
            filtered_accounts: self.filtered_accounts,
        }
        .assimilate_storage(&mut storage)
        .unwrap();

        let mut ext: sp_io::TestExternalities = storage.into();
        ext.execute_with(|| {
            let new_blk = BlockNumberFor::<T>::one();
            frame_system::Pallet::<T>::set_block_number(new_blk);
            on_initialize::<T>(new_blk);

            // ensure total supply is as expected
            let total_supply = pallet_balances::Pallet::<T>::total_issuance();
            if total_supply < self.total_supply {
                // Mint the difference to SIGNER user
                let diff = self.total_supply.saturating_sub(total_supply);
                let _ = CurrencyOf::<T>::deposit_creating(&SIGNER, diff);
            }
        });

        ext
    }
}

#[allow(unused)]
pub(crate) fn run_to_block_impl<T>(n: BlockNumberFor<T>)
where
    T: frame_system::Config,
    T: pallet_timestamp::Config,
    T: pallet_authorship::Config,
    T: pallet_staking::Config,
    T: pallet_session::Config,
    T: pallet_election_provider_multi_phase::Config,
{
    while frame_system::Pallet::<T>::block_number() < n {
        let current_blk = frame_system::Pallet::<T>::block_number();
        on_finalize::<T>(current_blk);

        let new_block_number = current_blk + One::one();
        frame_system::Pallet::<T>::set_block_number(new_block_number);
        on_initialize::<T>(new_block_number);
    }
}

#[allow(unused)]
pub(crate) fn run_to_block(n: BlockNumberFor<Test>) {
    run_to_block_impl::<Test>(n)
}

#[allow(unused)]
pub(crate) fn run_for_n_blocks(n: u64) {
    let now = System::block_number();
    let until = now + n;
    for current_blk in now..until {
        on_finalize::<Test>(current_blk);

        let new_block_number = current_blk + 1;
        System::set_block_number(new_block_number);
        on_initialize::<Test>(new_block_number);
    }
}

pub fn run_to_unsigned() {
    while !matches!(
        ElectionProviderMultiPhase::current_phase(),
        multi_phase::Phase::Unsigned(_)
    ) {
        run_to_block(System::block_number() + 1);
    }
}

pub fn run_to_signed() {
    while !matches!(
        ElectionProviderMultiPhase::current_phase(),
        multi_phase::Phase::Signed
    ) {
        run_to_block(System::block_number() + 1);
    }
}

// Run on_initialize hooks in order as they appear in AllPalletsWithSystem.
pub(crate) fn on_initialize<T>(new_block_number: BlockNumberFor<T>)
where
    T: frame_system::Config,
    T: pallet_timestamp::Config,
    T: pallet_authorship::Config,
    T: pallet_staking::Config,
    T: pallet_session::Config,
    T: pallet_election_provider_multi_phase::Config,
{
    let moment = <T as pallet_timestamp::Config>::Moment::from(MILLISECS_PER_BLOCK as u32);
    pallet_timestamp::Pallet::<T>::set_timestamp(moment.mul(new_block_number));
    pallet_authorship::Pallet::<T>::on_initialize(new_block_number);
    pallet_staking::Pallet::<T>::on_initialize(new_block_number);
    pallet_session::Pallet::<T>::on_initialize(new_block_number);
    pallet_election_provider_multi_phase::Pallet::<T>::on_initialize(new_block_number);
}

// Run on_finalize hooks (in pallets reverse order, as they appear in AllPalletsWithSystem)
pub(crate) fn on_finalize<T>(current_blk: BlockNumberFor<T>)
where
    T: frame_system::Config,
    T: pallet_staking::Config,
    T: pallet_authorship::Config,
{
    pallet_staking::Pallet::<T>::on_finalize(current_blk);
    pallet_authorship::Pallet::<T>::on_finalize(current_blk);
}

pub fn default_test_ext() -> sp_io::TestExternalities {
    ExtBuilder::<Test>::default()
        .initial_authorities(vec![
            (VAL_1_STASH, VAL_1_AUTH_ID),
            (VAL_2_STASH, VAL_2_AUTH_ID),
            (VAL_3_STASH, VAL_3_AUTH_ID),
        ])
        .stash(VALIDATOR_STAKE)
        .endowment(ENDOWMENT)
        .endowed_accounts(vec![SIGNER])
        .build()
}

pub(crate) fn validators_total_balance() -> u128 {
    pallet_staking::Validators::<Test>::iter()
        .map(|(stash_id, _)| Balances::free_balance(stash_id))
        .fold(0_u128, |acc, x| acc.saturating_add(x))
}

pub(crate) fn nominators_total_balance() -> u128 {
    pallet_staking::Nominators::<Test>::iter()
        .map(|(stash_id, _)| Balances::free_balance(stash_id))
        .fold(0_u128, |acc, x| acc.saturating_add(x))
}

// Returns the chain state as a tuple
// (`total_issuance`, `stakeable_amount`, `treasury_balance`, `staking_rewards_pool_balance`)
pub(crate) fn chain_state() -> (u128, u128, u128, u128) {
    (
        Balances::total_issuance(),
        StakingRewards::total_stakeable_tokens(),
        Balances::free_balance(Treasury::account_id()),
        StakingRewards::pool(),
    )
}

pub(crate) mod two_block_producers {
    use super::*;

    pub(crate) type TxExtension = (pallet_gear_staking_rewards::StakingBlackList<Test>,);
    type TestXt = sp_runtime::testing::TestXt<RuntimeCall, TxExtension>;
    type Block = TestBlock<TestXt>;

    construct_runtime!(
        pub enum Test
        {
            System: system,
            Timestamp: pallet_timestamp,
            Authorship: pallet_authorship,
            Balances: pallet_balances,
            Staking: pallet_staking,
            Session: pallet_session,
            Historical: pallet_session_historical,
            Treasury: pallet_treasury,
            Sudo: pallet_sudo,
            Utility: pallet_utility,
            ElectionProviderMultiPhase: multi_phase,
            StakingRewards: pallet_gear_staking_rewards,
        }
    );

    impl<C> frame_system::offchain::CreateTransactionBase<C> for Test
    where
        RuntimeCall: From<C>,
    {
        type RuntimeCall = RuntimeCall;
        type Extrinsic = TestXt;
    }

    impl<LocalCall> frame_system::offchain::CreateInherent<LocalCall> for Test
    where
        RuntimeCall: From<LocalCall>,
    {
        fn create_inherent(call: Self::RuntimeCall) -> Self::Extrinsic {
            UncheckedExtrinsic::new_bare(call)
        }
    }

    common::impl_pallet_system!(Test, DbWeight = RocksDbWeight, BlockWeights = ());
    common::impl_pallet_timestamp!(Test);

    pub struct BlockAuthor;

    impl FindAuthor<AccountId> for BlockAuthor {
        fn find_author<'a, I: 'a>(_: I) -> Option<AccountId> {
            let block_number = System::block_number() as usize;
            let validators = [VAL_1_STASH, VAL_2_STASH];
            let index = block_number % validators.len();

            Some(validators[index])
        }
    }

    common::impl_pallet_authorship!(Test, EventHandler = Staking, FindAuthor = BlockAuthor);
    common::impl_pallet_balances!(Test);
    common::impl_pallet_staking!(
        Test,
        EraPayout = StakingRewards,
        Slash = Treasury,
        Reward = StakingRewards,
        NextNewSession = Session,
        ElectionProvider = onchain::OnChainExecution<OnChainSeqPhragmen<Test>>,
        GenesisElectionProvider = onchain::OnChainExecution<OnChainSeqPhragmen<Test>>,
    );

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

    parameter_types! {
        pub TreasuryAccount: AccountId = Treasury::account_id();
    }

    impl pallet_treasury::Config for Test {
        type PalletId = TreasuryPalletId;
        type Currency = Balances;
        type RejectOrigin = EnsureRoot<AccountId>;
        type RuntimeEvent = RuntimeEvent;
        type SpendPeriod = SpendPeriod;
        type Burn = Burn;
        type BurnDestination = ();
        type SpendFunds = ();
        type WeightInfo = ();
        type MaxApprovals = MaxApprovals;
        type SpendOrigin = NeverEnsureOrigin<u128>;
        type AssetKind = ();
        type Beneficiary = Self::AccountId;
        type BeneficiaryLookup = IdentityLookup<Self::Beneficiary>;
        type Paymaster = PayFromAccount<Balances, TreasuryAccount>;
        type BalanceConverter = UnityAssetBalanceConversion;
        type PayoutPeriod = ConstU64<10>;
        type BlockNumberProvider = System;
    }

    impl pallet_sudo::Config for Test {
        type RuntimeEvent = RuntimeEvent;
        type RuntimeCall = RuntimeCall;
        type WeightInfo = ();
    }

    impl pallet_utility::Config for Test {
        type RuntimeEvent = RuntimeEvent;
        type RuntimeCall = RuntimeCall;
        type WeightInfo = ();
        type PalletsOrigin = OriginCaller;
    }

    impl multi_phase::MinerConfig for Test {
        type AccountId = AccountId;
        type MaxLength = MinerMaxLength;
        type MaxWeight = MinerMaxWeight;
        type MaxVotesPerVoter = <Staking as ElectionDataProvider>::MaxVotesPerVoter;
        type MaxWinners = MaxActiveValidators;
        type Solution = TestNposSolution;

        fn solution_weight(v: u32, t: u32, a: u32, d: u32) -> Weight {
            <<Self as multi_phase::Config>::WeightInfo as multi_phase::WeightInfo>::submit_unsigned(
                v, t, a, d,
            )
        }
    }

    impl multi_phase::Config for Test {
        type RuntimeEvent = RuntimeEvent;
        type Currency = Balances;
        type EstimateCallFee = ConstU32<1_000>;
        type SignedPhase = SignedPhase;
        type UnsignedPhase = UnsignedPhase;
        type BetterSignedThreshold = ();
        type OffchainRepeat = OffchainRepeat;
        type MinerTxPriority = MinerTxPriority;
        type SignedRewardBase = SignedRewardBase;
        type SignedDepositBase = multi_phase::GeometricDepositBase<
            Balance,
            SignedFixedDeposit,
            SignedDepositIncreaseFactor,
        >;
        type SignedDepositByte = ();
        type SignedDepositWeight = ();
        type SignedMaxWeight = SignedMaxWeight;
        type SignedMaxSubmissions = SignedMaxSubmissions;
        type SignedMaxRefunds = SignedMaxRefunds;
        type SlashHandler = Treasury;
        type RewardHandler = ();
        type DataProvider = Staking;
        type Fallback = onchain::OnChainExecution<OnChainSeqPhragmen<Self>>;
        type GovernanceFallback = onchain::OnChainExecution<OnChainSeqPhragmen<Self>>;
        type ForceOrigin = frame_system::EnsureRoot<AccountId>;
        type MaxWinners = MaxActiveValidators;
        type ElectionBounds = ElectionBounds;
        type WeightInfo = ();
        type BenchmarkingConfig = TestBenchmarkingConfig;
        type MinerConfig = Self;
        type Solver = SequentialPhragmen<AccountId, multi_phase::SolutionAccuracyOf<Self>, ()>;
    }

    impl pallet_gear_staking_rewards::Config for Test {
        type RuntimeEvent = RuntimeEvent;
        type BondCallFilter = ();
        type AccountFilter = NonStakingAccountsFilter;
        type PalletId = StakingRewardsPalletId;
        type RefillOrigin = EnsureRoot<AccountId>;
        type WithdrawOrigin = EnsureRoot<AccountId>;
        type MillisecondsPerYear = MillisecondsPerYear;
        type MinInflation = MinInflation;
        type MaxROI = MaxROI;
        type Falloff = Falloff;
        type WeightInfo = ();
    }

    #[allow(unused)]
    pub(crate) fn run_to_block(n: BlockNumberFor<Test>) {
        run_to_block_impl::<Test>(n)
    }
}
