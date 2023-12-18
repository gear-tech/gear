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

use crate as pallet_gear_staking_rewards;
use frame_election_provider_support::{
    onchain, ElectionDataProvider, SequentialPhragmen, VoteWeight,
};
use frame_support::{
    construct_runtime, parameter_types,
    traits::{
        ConstU32, Contains, Currency, Everything, FindAuthor, Hooks,
        NeverEnsureOrigin,
    },
    weights::{constants::RocksDbWeight, Weight},
    PalletId,
};
use frame_system::{self as system, pallet_prelude::BlockNumberFor, EnsureRoot};
use pallet_election_provider_multi_phase::{self as multi_phase};
use pallet_session::historical::{self as pallet_session_historical};
use sp_core::{crypto::key_types, H256};
use sp_runtime::{
    testing::{Block as TestBlock, UintAuthorityId},
    traits::{BlakeTwo256, IdentityLookup, OpaqueKeys},
    BuildStorage, KeyTypeId, Perbill, Permill, Perquintill,
};
use sp_std::convert::{TryFrom, TryInto};

pub(crate) type SignedExtra = pallet_gear_staking_rewards::StakingBlackList<Test>;
type TestXt = sp_runtime::testing::TestXt<RuntimeCall, SignedExtra>;
type Block = TestBlock<TestXt>;
type AccountId = u64;
// type BlockNumber = u64;
type Balance = u128;

pub(crate) type Executive = frame_executive::Executive<
    Test,
    Block,
    frame_system::ChainContext<Test>,
    Test,
    AllPalletsWithSystem,
>;

pub(crate) const SIGNER: AccountId = 1;
pub(crate) const VAL_1_STASH: AccountId = 10;
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
        BagsList: pallet_bags_list::<Instance1>,
        Sudo: pallet_sudo,
        Utility: pallet_utility,
        ElectionProviderMultiPhase: multi_phase,
        StakingRewards: pallet_gear_staking_rewards,
    }
);

impl pallet_balances::Config for Test {
    type MaxLocks = ConstU32<1024>;
    type MaxReserves = ();
    type ReserveIdentifier = [u8; 8];
    type Balance = Balance;
    type DustRemoval = ();
    type RuntimeEvent = RuntimeEvent;
    type ExistentialDeposit = ExistentialDeposit;
    type AccountStore = System;
    type WeightInfo = ();
    type FreezeIdentifier = ();
    type MaxFreezes = ();
    type RuntimeHoldReason = RuntimeHoldReason;
    type MaxHolds = ();
}

parameter_types! {
    pub const BlockHashCount: u64 = 250;
    pub const SS58Prefix: u8 = 42;
    pub const ExistentialDeposit: Balance = EXISTENTIAL_DEPOSIT;
}

impl system::Config for Test {
    type BaseCallFilter = Everything;
    type BlockWeights = ();
    type BlockLength = ();
    type DbWeight = RocksDbWeight;
    type RuntimeOrigin = RuntimeOrigin;
    type RuntimeCall = RuntimeCall;
    type Nonce = u64;
    type Hash = H256;
    type Hashing = BlakeTwo256;
    type AccountId = AccountId;
    type Lookup = IdentityLookup<Self::AccountId>;
    type Block = Block;
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
    type MaxConsumers = ConstU32<16>;
}

pub struct FixedBlockAuthor;

impl FindAuthor<u64> for FixedBlockAuthor {
    fn find_author<'a, I>(_digests: I) -> Option<u64>
    where
        I: 'a + IntoIterator<Item = (sp_runtime::ConsensusEngineId, &'a [u8])>,
    {
        Some(VAL_1_STASH)
    }
}

impl pallet_authorship::Config for Test {
    type FindAuthor = FixedBlockAuthor;

    type EventHandler = Staking;
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
    // 6 sessions in an era
    pub const SessionsPerEra: u32 = 6;
    // 8 eras for unbonding
    pub const BondingDuration: u32 = 8;
    pub const SlashDeferDuration: u32 = 7;
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

frame_election_provider_support::generate_solution_type!(
    #[compact]
    pub struct TestNposSolution::<
        VoterIndex = u32,
        TargetIndex = u16,
        Accuracy = sp_runtime::PerU16,
        MaxVoters = ConstU32::<2_000>,
    >(16)
);

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
    type CurrencyBalance = <Self as pallet_balances::Config>::Balance;
    type CurrencyToVote = ();
    type ElectionProvider = onchain::OnChainExecution<OnChainSeqPhragmen>;
    type GenesisElectionProvider = onchain::OnChainExecution<OnChainSeqPhragmen>;
    type RewardRemainder = ();
    type RuntimeEvent = RuntimeEvent;
    type Slash = Treasury;
    type Reward = StakingRewards;
    type SessionsPerEra = SessionsPerEra;
    type BondingDuration = BondingDuration;
    type SlashDeferDuration = SlashDeferDuration;
    type AdminOrigin = EnsureRoot<AccountId>;
    type SessionInterface = Self;
    type EraPayout = StakingRewards;
    type NextNewSession = Session;
    type MaxNominatorRewardedPerValidator = MaxNominatorRewardedPerValidator;
    type OffendingValidatorsThreshold = OffendingValidatorsThreshold;
    type VoterList = BagsList;
    type TargetList = pallet_staking::UseValidatorsMap<Self>;
    type MaxUnlockingChunks = ConstU32<32>;
    type HistoryDepth = HistoryDepth;
    type EventListeners = ();
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
    pub const ProposalBond: Permill = Permill::from_percent(5);
    pub const ProposalBondMinimum: u128 = UNITS;
    pub const SpendPeriod: u32 = 100;
    pub const Burn: Permill = Permill::from_percent(50);
    pub const TreasuryPalletId: PalletId = PalletId(*b"py/trsry");
    pub const MaxApprovals: u32 = 100;
}

impl pallet_treasury::Config for Test {
    type PalletId = TreasuryPalletId;
    type Currency = Balances;
    type ApproveOrigin = EnsureRoot<AccountId>;
    type RejectOrigin = EnsureRoot<AccountId>;
    type RuntimeEvent = RuntimeEvent;
    type OnSlash = ();
    type ProposalBond = ProposalBond;
    type ProposalBondMinimum = ProposalBondMinimum;
    type ProposalBondMaximum = ();
    type SpendPeriod = SpendPeriod;
    type Burn = Burn;
    type BurnDestination = ();
    type SpendFunds = ();
    type WeightInfo = ();
    type MaxApprovals = MaxApprovals;
    type SpendOrigin = NeverEnsureOrigin<u128>;
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
    type BetterUnsignedThreshold = BetterUnsignedThreshold;
    type BetterSignedThreshold = ();
    type OffchainRepeat = OffchainRepeat;
    type MinerTxPriority = MinerTxPriority;
    type SignedRewardBase = SignedRewardBase;
    type SignedDepositBase = SignedDepositBase;
    type SignedDepositByte = ();
    type SignedDepositWeight = ();
    type SignedMaxWeight = SignedMaxWeight;
    type SignedMaxSubmissions = SignedMaxSubmissions;
    type SignedMaxRefunds = SignedMaxRefunds;
    type SlashHandler = Treasury;
    type RewardHandler = StakingRewards;
    type DataProvider = Staking;
    type Fallback = onchain::OnChainExecution<OnChainSeqPhragmen>;
    type GovernanceFallback = onchain::OnChainExecution<OnChainSeqPhragmen>;
    type ForceOrigin = frame_system::EnsureRoot<AccountId>;
    type MaxElectableTargets = MaxElectableTargets;
    type MaxWinners = MaxActiveValidators;
    type MaxElectingVoters = MaxElectingVoters;
    type WeightInfo = ();
    type BenchmarkingConfig = TestBenchmarkingConfig;
    type MinerConfig = Self;
    type Solver = SequentialPhragmen<AccountId, multi_phase::SolutionAccuracyOf<Self>, ()>;
}

impl<C> frame_system::offchain::SendTransactionTypes<C> for Test
where
    RuntimeCall: From<C>,
{
    type OverarchingCall = RuntimeCall;
    type Extrinsic = TestXt;
}

pub type ValidatorAccountId = (
    AccountId,       // stash
    UintAuthorityId, // authority discovery ID
);

// Build genesis storage according to the mock runtime.
#[derive(Default)]
pub struct ExtBuilder {
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
        let mut storage = system::GenesisConfig::<Test>::default()
            .build_storage()
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

        TreasuryConfig::default()
            .assimilate_storage(&mut storage)
            .unwrap();

        SessionConfig {
            keys: self
                .initial_authorities
                .iter()
                .map(|x| (x.0, x.0, x.1.clone()))
                .collect(),
        }
        .assimilate_storage(&mut storage)
        .unwrap();

        SudoConfig { key: self.root }
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

        StakingRewardsConfig {
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
        on_finalize(current_blk);

        let new_block_number = current_blk + 1;
        System::set_block_number(new_block_number);
        on_initialize(new_block_number);
    }
}

#[allow(unused)]
pub(crate) fn run_for_n_blocks(n: u64) {
    let now = System::block_number();
    let until = now + n;
    for current_blk in now..until {
        on_finalize(current_blk);

        let new_block_number = current_blk + 1;
        System::set_block_number(new_block_number);
        on_initialize(new_block_number);
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
pub(crate) fn on_initialize(new_block_number: BlockNumberFor<Test>) {
    Timestamp::set_timestamp(new_block_number.saturating_mul(MILLISECS_PER_BLOCK));
    Authorship::on_initialize(new_block_number);
    Staking::on_initialize(new_block_number);
    Session::on_initialize(new_block_number);
    ElectionProviderMultiPhase::on_initialize(new_block_number);
}

// Run on_finalize hooks (in pallets reverse order, as they appear in AllPalletsWithSystem)
pub(crate) fn on_finalize(current_blk: BlockNumberFor<Test>) {
    Staking::on_finalize(current_blk);
    Authorship::on_finalize(current_blk);
}

pub fn default_test_ext() -> sp_io::TestExternalities {
    ExtBuilder::default()
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
