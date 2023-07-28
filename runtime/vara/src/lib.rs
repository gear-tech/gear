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

#![cfg_attr(not(feature = "std"), no_std)]
// `construct_runtime!` does a lot of recursion and requires us to increase the limit to 256.
#![recursion_limit = "256"]

// Make the WASM binary available.
#[cfg(feature = "std")]
include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));

use common::storage::{Mailbox, Messenger};
use frame_election_provider_support::{onchain, SequentialPhragmen};
use frame_support::weights::ConstantMultiplier;
pub use frame_support::{
    codec::{Decode, Encode, MaxEncodedLen},
    construct_runtime,
    dispatch::{DispatchClass, WeighData},
    parameter_types,
    traits::{
        ConstU128, ConstU16, ConstU32, Contains, Currency, EitherOf, EitherOfDiverse,
        EqualPrivilegeOnly, Everything, FindAuthor, InstanceFilter, KeyOwnerProofSystem,
        LockIdentifier, Nothing, OnUnbalanced, Randomness, StorageInfo, U128CurrencyToVote,
        WithdrawReasons,
    },
    weights::{
        constants::{
            BlockExecutionWeight, ExtrinsicBaseWeight, RocksDbWeight, WEIGHT_REF_TIME_PER_MILLIS,
            WEIGHT_REF_TIME_PER_SECOND,
        },
        Weight,
    },
    PalletId, RuntimeDebug, StorageValue,
};
use frame_system::{
    limits::{BlockLength, BlockWeights},
    EnsureRoot,
};
pub use pallet_gear::manager::{ExtManager, HandleKind};
pub use pallet_gear_payment::{CustomChargeTransactionPayment, DelegateFee};
pub use pallet_gear_staking_rewards::StakingBlackList;
use pallet_grandpa::{
    fg_primitives, AuthorityId as GrandpaId, AuthorityList as GrandpaAuthorityList,
};
use pallet_im_online::sr25519::AuthorityId as ImOnlineId;
use pallet_session::historical::{self as pallet_session_historical};
pub use pallet_timestamp::Call as TimestampCall;
pub use pallet_transaction_payment::{CurrencyAdapter, Multiplier};
pub use runtime_common::{
    constants::{RENT_RESUME_WEEK_FACTOR, RESUME_SESSION_DURATION_HOUR_FACTOR},
    impl_runtime_apis_plus_common, BlockHashCount, DealWithFees, GasConverter,
    AVERAGE_ON_INITIALIZE_RATIO, GAS_LIMIT_MIN_PERCENTAGE_NUM, NORMAL_DISPATCH_RATIO,
    VALUE_PER_GAS,
};
pub use runtime_primitives::{AccountId, Signature};
use runtime_primitives::{Balance, BlockNumber, Hash, Index, Moment};
use sp_api::impl_runtime_apis;
use sp_core::{crypto::KeyTypeId, ConstU64, OpaqueMetadata, H256};
use sp_runtime::{
    create_runtime_str, generic, impl_opaque_keys,
    traits::{AccountIdLookup, BlakeTwo256, Block as BlockT, ConvertInto, NumberFor, OpaqueKeys},
    transaction_validity::{TransactionPriority, TransactionSource, TransactionValidity},
    ApplyExtrinsicResult, Perbill, Percent, Permill, Perquintill,
};
use sp_std::{
    convert::{TryFrom, TryInto},
    prelude::*,
};
#[cfg(feature = "std")]
use sp_version::NativeVersion;
use sp_version::RuntimeVersion;
use static_assertions::const_assert;

#[cfg(any(feature = "std", test))]
pub use frame_system::Call as SystemCall;
#[cfg(any(feature = "std", test))]
pub use pallet_balances::Call as BalancesCall;
#[cfg(any(feature = "std", test))]
pub use pallet_staking::StakerStatus;
#[cfg(any(feature = "std", test))]
pub use pallet_sudo::Call as SudoCall;
#[cfg(any(feature = "std", test))]
pub use sp_runtime::BuildStorage;

pub use pallet_gear;
#[cfg(feature = "debug-mode")]
pub use pallet_gear_debug;
pub use pallet_gear_gas;
pub use pallet_gear_payment;

pub mod constants;

pub use constants::{currency::*, time::*};

// Weights used in the runtime.
mod weights;

pub mod governance;
use governance::{pallet_custom_origins, GeneralAdmin, Treasurer, TreasurySpender};

mod extensions;
pub use extensions::DisableValueTransfers;

mod migrations;

// By this we inject compile time version including commit hash
// (https://github.com/paritytech/substrate/blob/297b3948f4a0f7f6504d4b654e16cb5d9201e523/utils/build-script-utils/src/version.rs#L44)
// into the WASM runtime blob. This is used by the `runtime_wasmBlobVersion` RPC call.
// The format of the version is `x.y.z-commit_hash`, where the `x.y.z` is the version of this crate,
// and the `commit_hash` is the hash of the commit from which the WASM blob was built.
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
#[link_section = "wasm_blob_version"]
static _WASM_BLOB_VERSION: [u8; const_str::to_byte_array!(env!("SUBSTRATE_CLI_IMPL_VERSION"))
    .len()] = const_str::to_byte_array!(env!("SUBSTRATE_CLI_IMPL_VERSION"));

#[sp_version::runtime_version]
pub const VERSION: RuntimeVersion = RuntimeVersion {
    spec_name: create_runtime_str!("vara"),
    impl_name: create_runtime_str!("vara"),
    authoring_version: 1,
    // The version of the runtime specification. A full node will not attempt to use its native
    //   runtime in substitute for the on-chain Wasm runtime unless all of `spec_name`,
    //   `spec_version`, and `authoring_version` are the same between Wasm and native.
    spec_version: 220,
    impl_version: 1,
    apis: RUNTIME_API_VERSIONS,
    transaction_version: 1,
    state_version: 1,
};

/// The BABE epoch configuration at genesis.
pub const BABE_GENESIS_EPOCH_CONFIG: sp_consensus_babe::BabeEpochConfiguration =
    sp_consensus_babe::BabeEpochConfiguration {
        c: PRIMARY_PROBABILITY,
        allowed_slots: sp_consensus_babe::AllowedSlots::PrimaryAndSecondaryPlainSlots,
    };

// We'll verify that WEIGHT_REF_TIME_PER_SECOND does not overflow, allowing us to use
// simple multiply and divide operators instead of saturating or checked ones.
const_assert!(WEIGHT_REF_TIME_PER_SECOND.checked_div(3).is_some());
const_assert!((WEIGHT_REF_TIME_PER_SECOND / 3).checked_mul(2).is_some());

/// We allow for 1/3 of block time for computations, with maximum proof size.
///
/// It's 2/3 sec for vara runtime with 2 second block duration.
const MAXIMUM_BLOCK_WEIGHT: Weight =
    Weight::from_parts(WEIGHT_REF_TIME_PER_SECOND * 2 / 3, u64::MAX);

/// The version information used to identify this runtime when compiled natively.
#[cfg(feature = "std")]
pub fn native_version() -> NativeVersion {
    NativeVersion {
        runtime_version: VERSION,
        can_author_with: Default::default(),
    }
}

parameter_types! {
    pub const Version: RuntimeVersion = VERSION;
    pub const SS58Prefix: u8 = 137;
    pub RuntimeBlockWeights: BlockWeights = runtime_common::block_weights_for(MAXIMUM_BLOCK_WEIGHT);
    pub RuntimeBlockLength: BlockLength =
        BlockLength::max_with_normal_ratio(5 * 1024 * 1024, NORMAL_DISPATCH_RATIO);
}

// Configure FRAME pallets to include in runtime.

impl frame_system::Config for Runtime {
    /// The basic call filter to use in dispatchable.
    type BaseCallFilter = Everything;
    /// Block & extrinsics weights: base values and limits.
    type BlockWeights = RuntimeBlockWeights;
    /// The maximum length of a block (in bytes).
    type BlockLength = RuntimeBlockLength;
    /// The identifier used to distinguish between accounts.
    type AccountId = AccountId;
    /// The aggregated dispatch type that is available for extrinsics.
    type RuntimeCall = RuntimeCall;
    /// The lookup mechanism to get account ID from whatever is passed in dispatchers.
    type Lookup = AccountIdLookup<AccountId, ()>;
    /// The index type for storing how many extrinsics an account has signed.
    type Index = Index;
    /// The index type for blocks.
    type BlockNumber = BlockNumber;
    /// The type for hashing blocks and tries.
    type Hash = Hash;
    /// The hashing algorithm used.
    type Hashing = BlakeTwo256;
    /// The header type.
    type Header = generic::Header<BlockNumber, BlakeTwo256>;
    /// The ubiquitous event type.
    type RuntimeEvent = RuntimeEvent;
    /// The ubiquitous origin type.
    type RuntimeOrigin = RuntimeOrigin;
    /// Maximum number of block number to block hash mappings to keep (oldest pruned first).
    type BlockHashCount = BlockHashCount;
    /// The weight of database operations that the runtime can invoke.
    type DbWeight = RocksDbWeight;
    /// Version of the runtime.
    type Version = Version;
    /// Converts a module to the index of the module in `construct_runtime!`.
    ///
    /// This type is being generated by `construct_runtime!`.
    type PalletInfo = PalletInfo;
    /// What to do if a new account is created.
    type OnNewAccount = ();
    /// What to do if an account is fully reaped from the system.
    type OnKilledAccount = ();
    /// The data to be stored in an account.
    type AccountData = pallet_balances::AccountData<Balance>;
    /// Weight information for the extrinsics of this pallet.
    type SystemWeightInfo = weights::frame_system::SubstrateWeight<Runtime>;
    /// This is used as an identifier of the chain. 42 is the generic substrate prefix.
    type SS58Prefix = SS58Prefix;
    /// The set code logic, just the default since we're not a parachain.
    type OnSetCode = ();
    type MaxConsumers = ConstU32<16>;
}

parameter_types! {
    pub const EpochDuration: u64 = EPOCH_DURATION_IN_SLOTS;
    pub const ExpectedBlockTime: Moment = MILLISECS_PER_BLOCK;
    pub const ReportLongevity: u64 =
        BondingDuration::get() as u64 * SessionsPerEra::get() as u64 * EpochDuration::get();
    pub const MaxSetIdSessionEntries: u32 = BondingDuration::get() * SessionsPerEra::get();
}

impl pallet_babe::Config for Runtime {
    type EpochDuration = EpochDuration;
    type ExpectedBlockTime = ExpectedBlockTime;
    type EpochChangeTrigger = pallet_babe::ExternalTrigger;
    type DisabledValidators = Session;

    type WeightInfo = ();
    type MaxAuthorities = MaxAuthorities;

    type KeyOwnerProof =
        <Historical as KeyOwnerProofSystem<(KeyTypeId, pallet_babe::AuthorityId)>>::Proof;
    type EquivocationReportSystem = ();
}

impl pallet_grandpa::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;

    type WeightInfo = ();
    type MaxAuthorities = MaxAuthorities;
    type MaxSetIdSessionEntries = MaxSetIdSessionEntries;
    type KeyOwnerProof = <Historical as KeyOwnerProofSystem<(KeyTypeId, GrandpaId)>>::Proof;
    type EquivocationReportSystem = ();
}

impl pallet_authorship::Config for Runtime {
    type FindAuthor = pallet_session::FindAccountFromAuthorIndex<Self, Babe>;
    type EventHandler = ImOnline;
}

parameter_types! {
    pub const MinimumPeriod: Moment = SLOT_DURATION / 2;
}

impl pallet_timestamp::Config for Runtime {
    /// A timestamp: milliseconds since the unix epoch.
    type Moment = Moment;
    type OnTimestampSet = Babe;
    type MinimumPeriod = MinimumPeriod;
    type WeightInfo = weights::pallet_timestamp::SubstrateWeight<Runtime>;
}

parameter_types! {
    pub MaximumSchedulerWeight: Weight = Perbill::from_percent(80) *
        RuntimeBlockWeights::get().max_block;
    // Retry a scheduled item every 30 blocks (1 minute) until the preimage exists.
    pub const NoPreimagePostponement: Option<u32> = Some(30);
}

impl pallet_scheduler::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type RuntimeOrigin = RuntimeOrigin;
    type PalletsOrigin = OriginCaller;
    type RuntimeCall = RuntimeCall;
    type MaximumWeight = MaximumSchedulerWeight;
    type ScheduleOrigin = EnsureRoot<AccountId>;
    type MaxScheduledPerBlock = ConstU32<512>;
    type WeightInfo = pallet_scheduler::weights::SubstrateWeight<Runtime>;
    type OriginPrivilegeCmp = EqualPrivilegeOnly;
    type Preimages = Preimage;
}

parameter_types! {
    pub const PreimageMaxSize: u32 = 4096 * 1024;
    pub const PreimageBaseDeposit: Balance = DOLLARS;
    pub const PreimageByteDeposit: Balance = CENTS;
}

impl pallet_preimage::Config for Runtime {
    type WeightInfo = pallet_preimage::weights::SubstrateWeight<Runtime>;
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type ManagerOrigin = EnsureRoot<AccountId>;
    type BaseDeposit = PreimageBaseDeposit;
    type ByteDeposit = PreimageByteDeposit;
}

impl pallet_balances::Config for Runtime {
    type MaxLocks = ConstU32<50>;
    type MaxReserves = ();
    type ReserveIdentifier = [u8; 8];
    /// The type for recording an account's balance.
    type Balance = Balance;
    /// The ubiquitous event type.
    type RuntimeEvent = RuntimeEvent;
    type DustRemoval = ();
    type ExistentialDeposit = ConstU128<EXISTENTIAL_DEPOSIT>;
    type AccountStore = System;
    type WeightInfo = weights::pallet_balances::SubstrateWeight<Runtime>;
}

parameter_types! {
    pub const TransactionByteFee: Balance = 1;
    pub const QueueLengthStep: u128 = 10;
    pub const OperationalFeeMultiplier: u8 = 5;
}

impl pallet_transaction_payment::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type OnChargeTransaction = CurrencyAdapter<Balances, DealWithFees<Runtime>>;
    type OperationalFeeMultiplier = OperationalFeeMultiplier;
    type WeightToFee = ConstantMultiplier<u128, ConstU128<VALUE_PER_GAS>>;
    type LengthToFee = ConstantMultiplier<u128, ConstU128<VALUE_PER_GAS>>;
    type FeeMultiplierUpdate = pallet_gear_payment::GearFeeMultiplier<Runtime, QueueLengthStep>;
}

parameter_types! {
    pub const MinAuthorities: u32 = 1;
}

impl validator_set::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type AddRemoveOrigin = EnsureRoot<AccountId>;
    type MinAuthorities = MinAuthorities;
}

impl_opaque_keys! {
    pub struct SessionKeys {
        pub babe: Babe,
        pub grandpa: Grandpa,
        pub im_online: ImOnline,
        pub authority_discovery: AuthorityDiscovery,
    }
}

impl pallet_session::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type ValidatorId = <Self as frame_system::Config>::AccountId;
    type ValidatorIdOf = validator_set::ValidatorOf<Self>;
    type ShouldEndSession = Babe;
    type NextSessionRotation = Babe;
    type SessionManager = ValidatorSet;
    type SessionHandler = <SessionKeys as OpaqueKeys>::KeyTypeIdProviders;
    type Keys = SessionKeys;
    type WeightInfo = pallet_session::weights::SubstrateWeight<Runtime>;
}

impl pallet_session_historical::Config for Runtime {
    type FullIdentification = pallet_staking::Exposure<AccountId, Balance>;
    type FullIdentificationOf = pallet_staking::ExposureOf<Runtime>;
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
            RuntimeCall::Proxy(pallet_proxy::Call::proxy { call, .. })
            | RuntimeCall::Proxy(pallet_proxy::Call::proxy_announced { call, .. }) => {
                Self::contains(call)
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
}

impl pallet_gear_staking_rewards::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type BondCallFilter = BondCallFilter;
    type AccountFilter = NonStakingAccountsFilter;
    type PalletId = StakingRewardsPalletId;
    type RefillOrigin = EnsureRoot<AccountId>;
    type WithdrawOrigin = EnsureRoot<AccountId>;
    type MillisecondsPerYear = ConstU64<MILLISECONDS_PER_YEAR>;
    type MinInflation = MinInflation;
    type MaxROI = MaxROI;
    type Falloff = Falloff;
    type WeightInfo = pallet_gear_staking_rewards::weights::SubstrateWeight<Runtime>;
}

// TODO: review staking parameters - currently copying Kusama
parameter_types! {
    // Six sessions in an era (12 hours)
    pub const SessionsPerEra: sp_staking::SessionIndex = 6;
    // 42 eras for unbonding (7 days)
    pub const BondingDuration: sp_staking::EraIndex = 14;
    // 41 eras during which slashes can be cancelled (slightly less than 7 days)
    pub const SlashDeferDuration: sp_staking::EraIndex = 13;
    pub const MaxNominatorRewardedPerValidator: u32 = 256;
    pub const OffendingValidatorsThreshold: Perbill = Perbill::from_percent(17);
    pub HistoryDepth: u32 = 84;
}

/// Only the root origin can cancel the slash
type AdminOrigin = EnsureRoot<AccountId>;

pub struct StakingBenchmarkingConfig;
impl pallet_staking::BenchmarkingConfig for StakingBenchmarkingConfig {
    type MaxNominators = ConstU32<1000>;
    type MaxValidators = ConstU32<1000>;
}

parameter_types! {
    pub const MaxActiveValidators: u32 = 100;
    pub const MaxOnChainElectingVoters: u32 = 500;
    pub const MaxOnChainElectableTargets: u16 = 100;
}

pub struct OnChainSeqPhragmen;
impl onchain::Config for OnChainSeqPhragmen {
    type System = Runtime;
    type Solver = SequentialPhragmen<AccountId, Perbill>;
    type DataProvider = Staking;
    type WeightInfo = frame_election_provider_support::weights::SubstrateWeight<Runtime>;
    type MaxWinners = MaxActiveValidators;
    type VotersBound = MaxOnChainElectingVoters;
    type TargetsBound = MaxOnChainElectableTargets;
}

impl pallet_staking::Config for Runtime {
    type MaxNominations = ConstU32<16>; // TODO: review with NPoS enabled
    type Currency = Balances;
    type CurrencyBalance = Balance;
    type UnixTime = Timestamp;
    type CurrencyToVote = U128CurrencyToVote;
    type ElectionProvider = onchain::OnChainExecution<OnChainSeqPhragmen>;
    type GenesisElectionProvider = onchain::OnChainExecution<OnChainSeqPhragmen>;
    // Burning the reward remainder for now.
    // TODO: set remainder back to `RewardsStash<Self, Treasury>` to stop burning `Treasury` part.
    type RewardRemainder = ();
    type RuntimeEvent = RuntimeEvent;
    type Slash = Treasury;
    type Reward = StakingRewards;
    type SessionsPerEra = SessionsPerEra;
    type BondingDuration = BondingDuration;
    type SlashDeferDuration = SlashDeferDuration;
    type AdminOrigin = AdminOrigin;
    type SessionInterface = Self;
    type EraPayout = StakingRewards;
    type NextNewSession = Session;
    type MaxNominatorRewardedPerValidator = MaxNominatorRewardedPerValidator;
    type OffendingValidatorsThreshold = OffendingValidatorsThreshold;
    type VoterList = BagsList;
    type MaxUnlockingChunks = ConstU32<32>;
    type BenchmarkingConfig = StakingBenchmarkingConfig;
    type OnStakerSlash = ();
    type WeightInfo = pallet_staking::weights::SubstrateWeight<Runtime>;
    type TargetList = pallet_staking::UseValidatorsMap<Self>;
    type HistoryDepth = HistoryDepth;
}

// Mocked threshoulds
const THRESHOLDS: [sp_npos_elections::VoteWeight; 9] =
    [10, 20, 30, 40, 50, 60, 1_000, 2_000, 10_000];
parameter_types! {
    // TODO: replace mocked thresholds with true ones
    pub const BagThresholds: &'static [u64] = &THRESHOLDS;
}

impl pallet_bags_list::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type ScoreProvider = Staking;
    type WeightInfo = pallet_bags_list::weights::SubstrateWeight<Runtime>;
    type BagThresholds = BagThresholds;
    type Score = sp_npos_elections::VoteWeight;
}

parameter_types! {
    pub const ImOnlineUnsignedPriority: TransactionPriority = TransactionPriority::max_value();
    /// We prioritize im-online heartbeats over election solution submission.
    pub const StakingUnsignedPriority: TransactionPriority = TransactionPriority::max_value() / 2;
    pub const MaxAuthorities: u32 = 32;
    pub const MaxKeys: u32 = 10_000;
    pub const MaxPeerInHeartbeats: u32 = 10_000;
    pub const MaxPeerDataEncodingSize: u32 = 1_000;
}

impl pallet_im_online::Config for Runtime {
    type AuthorityId = ImOnlineId;
    type RuntimeEvent = RuntimeEvent;
    type NextSessionRotation = Babe;
    type ValidatorSet = Historical;
    type ReportUnresponsiveness = ();
    type UnsignedPriority = ImOnlineUnsignedPriority;
    type WeightInfo = pallet_im_online::weights::SubstrateWeight<Runtime>;
    type MaxKeys = MaxKeys;
    type MaxPeerInHeartbeats = MaxPeerInHeartbeats;
    type MaxPeerDataEncodingSize = MaxPeerDataEncodingSize;
}

impl pallet_authority_discovery::Config for Runtime {
    type MaxAuthorities = MaxAuthorities;
}

parameter_types! {
    pub const BasicDeposit: Balance = 10 * DOLLARS;       // 258 bytes on-chain
    pub const FieldDeposit: Balance = 250 * CENTS;        // 66 bytes on-chain
    pub const SubAccountDeposit: Balance = 2 * DOLLARS;   // 53 bytes on-chain
    pub const MaxSubAccounts: u32 = 100;
    pub const MaxAdditionalFields: u32 = 100;
    pub const MaxRegistrars: u32 = 20;
}

impl pallet_identity::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type BasicDeposit = BasicDeposit;
    type FieldDeposit = FieldDeposit;
    type SubAccountDeposit = SubAccountDeposit;
    type MaxSubAccounts = MaxSubAccounts;
    type MaxAdditionalFields = MaxAdditionalFields;
    type MaxRegistrars = MaxRegistrars;
    type Slashed = Treasury;
    type ForceOrigin = EitherOf<EnsureRoot<AccountId>, GeneralAdmin>;
    type RegistrarOrigin = EitherOf<EnsureRoot<AccountId>, GeneralAdmin>;
    type WeightInfo = pallet_identity::weights::SubstrateWeight<Runtime>;
}

parameter_types! {
    pub const ProposalBond: Permill = Permill::from_percent(5);
    pub const ProposalBondMinimum: Balance = DOLLARS;
    pub const SpendPeriod: BlockNumber = DAYS;
    pub const Burn: Permill = Permill::from_percent(50);
    pub const TreasuryPalletId: PalletId = PalletId(*b"py/trsry");
    pub const MaxApprovals: u32 = 100;
}

impl pallet_treasury::Config for Runtime {
    type PalletId = TreasuryPalletId;
    type Currency = Balances;
    type ApproveOrigin = EitherOfDiverse<EnsureRoot<AccountId>, Treasurer>;
    type RejectOrigin = EitherOfDiverse<EnsureRoot<AccountId>, Treasurer>;
    type RuntimeEvent = RuntimeEvent;
    type OnSlash = ();
    type ProposalBond = ProposalBond;
    type ProposalBondMinimum = ProposalBondMinimum;
    type ProposalBondMaximum = ();
    type SpendPeriod = SpendPeriod;
    type Burn = Burn;
    type BurnDestination = ();
    type SpendFunds = (); // TODO: set to Bounties in NPoS
    type WeightInfo = pallet_treasury::weights::SubstrateWeight<Runtime>;
    type MaxApprovals = MaxApprovals;
    type SpendOrigin = TreasurySpender;
}

impl pallet_sudo::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type RuntimeCall = RuntimeCall;
}

impl pallet_utility::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type RuntimeCall = RuntimeCall;
    type WeightInfo = weights::pallet_utility::SubstrateWeight<Runtime>;
    type PalletsOrigin = OriginCaller;
}

parameter_types! {
    // One storage item; key size is 32; value is size 4+4+16+32 bytes = 56 bytes.
    pub const DepositBase: Balance = deposit(1, 88);
    // Additional storage item size of 32 bytes.
    pub const DepositFactor: Balance = deposit(0, 32);
}

impl pallet_multisig::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type RuntimeCall = RuntimeCall;
    type Currency = Balances;
    type DepositBase = DepositBase;
    type DepositFactor = DepositFactor;
    type MaxSignatories = ConstU32<100>;
    type WeightInfo = pallet_multisig::weights::SubstrateWeight<Runtime>;
}

parameter_types! {
    // One storage item; key size 32, value size 8; .
    pub const ProxyDepositBase: Balance = deposit(1, 8);
    // Additional storage item size of 33 bytes.
    pub const ProxyDepositFactor: Balance = deposit(0, 33);
    pub const AnnouncementDepositBase: Balance = deposit(1, 8);
    pub const AnnouncementDepositFactor: Balance = deposit(0, 66);
}

/// The type used to represent the kinds of proxying allowed.
#[derive(
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
    Any,
    NonTransfer,
    Governance,
    Staking,
    IdentityJudgement,
    CancelProxy,
}

impl Default for ProxyType {
    fn default() -> Self {
        Self::Any
    }
}

impl InstanceFilter<RuntimeCall> for ProxyType {
    fn filter(&self, c: &RuntimeCall) -> bool {
        match self {
            ProxyType::Any => true,
            ProxyType::NonTransfer => !matches!(
                c,
                RuntimeCall::Balances(..)
                    | RuntimeCall::Sudo(..)
                    | RuntimeCall::Vesting(pallet_vesting::Call::vested_transfer { .. })
                    | RuntimeCall::Vesting(pallet_vesting::Call::force_vested_transfer { .. })
            ),
            ProxyType::Governance => matches!(
                c,
                RuntimeCall::Treasury(..)
                    | RuntimeCall::ConvictionVoting(..)
                    | RuntimeCall::Referenda(..)
                    | RuntimeCall::FellowshipCollective(..)
                    | RuntimeCall::FellowshipReferenda(..)
                    | RuntimeCall::Whitelist(..)
            ),
            ProxyType::Staking => matches!(c, RuntimeCall::Staking(..)),
            ProxyType::IdentityJudgement => matches!(
                c,
                RuntimeCall::Identity(pallet_identity::Call::provide_judgement { .. })
                    | RuntimeCall::Utility(..)
            ),
            ProxyType::CancelProxy => {
                matches!(
                    c,
                    RuntimeCall::Proxy(pallet_proxy::Call::reject_announcement { .. })
                )
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

impl pallet_proxy::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type RuntimeCall = RuntimeCall;
    type Currency = Balances;
    type ProxyType = ProxyType;
    type ProxyDepositBase = ProxyDepositBase;
    type ProxyDepositFactor = ProxyDepositFactor;
    type MaxProxies = ConstU32<32>;
    type WeightInfo = pallet_proxy::weights::SubstrateWeight<Runtime>;
    type MaxPending = ConstU32<32>;
    type CallHasher = BlakeTwo256;
    type AnnouncementDepositBase = AnnouncementDepositBase;
    type AnnouncementDepositFactor = AnnouncementDepositFactor;
}

impl pallet_gear_program::Config for Runtime {
    type Scheduler = GearScheduler;
    type CurrentBlockNumber = Gear;
}

parameter_types! {
    pub const GasLimitMaxPercentage: Percent = Percent::from_percent(GAS_LIMIT_MIN_PERCENTAGE_NUM);
    pub BlockGasLimit: u64 = GasLimitMaxPercentage::get() * RuntimeBlockWeights::get()
        .max_block.ref_time();

    pub const ReserveThreshold: u32 = 1;
    pub const WaitlistCost: u64 = 100;
    pub const MailboxCost: u64 = 100;
    pub const ReservationCost: u64 = 100;
    pub const DispatchHoldCost: u64 = 100;

    pub const OutgoingLimit: u32 = 1024;
    pub const MailboxThreshold: u64 = 3000;
}

parameter_types! {
    pub Schedule: pallet_gear::Schedule<Runtime> = Default::default();
}

impl pallet_gear::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Randomness = pallet_babe::RandomnessFromOneEpochAgo<Runtime>;
    type Currency = Balances;
    type GasPrice = GasConverter;
    type WeightInfo = weights::pallet_gear::SubstrateWeight<Runtime>;
    type Schedule = Schedule;
    type OutgoingLimit = OutgoingLimit;
    type DebugInfo = DebugInfo;
    type CodeStorage = GearProgram;
    type ProgramStorage = GearProgram;
    type MailboxThreshold = MailboxThreshold;
    type ReservationsLimit = ConstU64<256>;
    type Messenger = GearMessenger;
    type GasProvider = GearGas;
    type BlockLimiter = GearGas;
    type Scheduler = GearScheduler;
    type QueueRunner = Gear;
    type Voucher = GearVoucher;
    type ProgramRentFreePeriod = ConstU32<RENT_FREE_PERIOD>;
    type ProgramResumeMinimalRentPeriod = ConstU32<{ WEEKS * RENT_RESUME_WEEK_FACTOR }>;
    type ProgramRentCostPerBlock = ConstU128<RENT_COST_PER_BLOCK>;
    type ProgramResumeSessionDuration = ConstU32<{ HOURS * RESUME_SESSION_DURATION_HOUR_FACTOR }>;
}

#[cfg(feature = "debug-mode")]
impl pallet_gear_debug::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type WeightInfo = pallet_gear_debug::weights::GearSupportWeight<Runtime>;
    type CodeStorage = GearProgram;
    type ProgramStorage = GearProgram;
    type Messenger = GearMessenger;
}

impl pallet_gear_scheduler::Config for Runtime {
    type BlockLimiter = GearGas;
    type ReserveThreshold = ReserveThreshold;
    type WaitlistCost = WaitlistCost;
    type MailboxCost = MailboxCost;
    type ReservationCost = ReservationCost;
    type DispatchHoldCost = DispatchHoldCost;
}

impl pallet_gear_gas::Config for Runtime {
    type BlockGasLimit = BlockGasLimit;
}

impl pallet_gear_messenger::Config for Runtime {
    type BlockLimiter = GearGas;
    type CurrentBlockNumber = Gear;
}

impl pallet_airdrop::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type WeightInfo = weights::pallet_airdrop::SubstrateWeight<Runtime>;
    type VestingSchedule = Vesting;
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
// TODO: in case of the `send_reply_with_voucher` call we have to iterate through the
// user's mailbox to dig out the stored message source `program_id` to check if it has
// issued a voucher to pay for the reply extrinsic transaction fee.
// Isn't there a better way to do that?
impl DelegateFee<RuntimeCall, AccountId> for DelegateFeeAccountBuilder {
    fn delegate_fee(call: &RuntimeCall, who: &AccountId) -> Option<AccountId> {
        match call {
            RuntimeCall::Gear(pallet_gear::Call::send_message_with_voucher {
                destination, ..
            }) => Some(GearVoucher::voucher_account_id(who, destination)),
            RuntimeCall::Gear(pallet_gear::Call::send_reply_with_voucher {
                reply_to_id, ..
            }) => <<GearMessenger as Messenger>::Mailbox as Mailbox>::peek(who, reply_to_id).map(
                |stored_message| GearVoucher::voucher_account_id(who, &stored_message.source()),
            ),
            _ => None,
        }
    }
}

impl pallet_gear_payment::Config for Runtime {
    type ExtraFeeCallFilter = ExtraFeeFilter;
    type DelegateFee = DelegateFeeAccountBuilder;
    type Messenger = GearMessenger;
}

parameter_types! {
    pub const VoucherPalletId: PalletId = PalletId(*b"py/vouch");
}

impl pallet_gear_voucher::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type PalletId = VoucherPalletId;
    type WeightInfo = weights::pallet_gear_voucher::SubstrateWeight<Runtime>;
}

impl<C> frame_system::offchain::SendTransactionTypes<C> for Runtime
where
    RuntimeCall: From<C>,
{
    type Extrinsic = UncheckedExtrinsic;
    type OverarchingCall = RuntimeCall;
}

parameter_types! {
    pub const MinVestedTransfer: Balance = 100 * CENTS;
    pub UnvestedFundsAllowedWithdrawReasons: WithdrawReasons =
        WithdrawReasons::except(WithdrawReasons::TRANSFER | WithdrawReasons::RESERVE);
}

impl pallet_vesting::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type BlockNumberToBalance = ConvertInto;
    type MinVestedTransfer = MinVestedTransfer;
    type WeightInfo = pallet_vesting::weights::SubstrateWeight<Runtime>;
    type UnvestedFundsAllowedWithdrawReasons = UnvestedFundsAllowedWithdrawReasons;
    const MAX_VESTING_SCHEDULES: u32 = 28;
}

// Create the runtime by composing the FRAME pallets that were previously configured.
#[cfg(feature = "debug-mode")]
construct_runtime!(
    pub enum Runtime where
        Block = Block,
        NodeBlock = runtime_primitives::Block,
        UncheckedExtrinsic = UncheckedExtrinsic
    {
        System: frame_system = 0,
        Timestamp: pallet_timestamp = 1,
        Authorship: pallet_authorship = 2,
        AuthorityDiscovery: pallet_authority_discovery = 9,
        Babe: pallet_babe = 3,
        Grandpa: pallet_grandpa = 4,
        Balances: pallet_balances = 5,
        Vesting: pallet_vesting = 10,
        TransactionPayment: pallet_transaction_payment = 6,
        BagsList: pallet_bags_list = 11,
        ImOnline: pallet_im_online = 12,
        Staking: pallet_staking = 13,
        Session: pallet_session = 7,
        Treasury: pallet_treasury = 14,
        Historical: pallet_session_historical = 15,

        // Governance
        ConvictionVoting: pallet_conviction_voting = 16,
        Referenda: pallet_referenda = 17,
        FellowshipCollective: pallet_ranked_collective::<Instance1> = 18,
        FellowshipReferenda: pallet_referenda::<Instance2> = 19,
        Origins: pallet_custom_origins = 20,
        Whitelist: pallet_whitelist = 21,

        // TODO: Remove in stage 2
        ValidatorSet: validator_set = 98,
        Sudo: pallet_sudo = 99,

        Scheduler: pallet_scheduler = 22,
        Preimage: pallet_preimage = 23,
        Identity: pallet_identity = 24,
        Proxy: pallet_proxy = 25,
        Multisig: pallet_multisig = 26,
        Utility: pallet_utility = 8,
        GearProgram: pallet_gear_program = 100,
        GearMessenger: pallet_gear_messenger = 101,
        GearScheduler: pallet_gear_scheduler = 102,
        GearGas: pallet_gear_gas = 103,
        Gear: pallet_gear = 104,
        GearPayment: pallet_gear_payment = 105,
        StakingRewards: pallet_gear_staking_rewards = 106,
        GearVoucher: pallet_gear_voucher = 107,

        // TODO: remove from production version
        Airdrop: pallet_airdrop = 198,

        // Only available with "debug-mode" feature on
        GearDebug: pallet_gear_debug = 199,
    }
);

#[cfg(not(feature = "debug-mode"))]
construct_runtime!(
    pub enum Runtime where
        Block = Block,
        NodeBlock = runtime_primitives::Block,
        UncheckedExtrinsic = UncheckedExtrinsic
    {
        System: frame_system = 0,
        Timestamp: pallet_timestamp = 1,
        Authorship: pallet_authorship = 2,
        AuthorityDiscovery: pallet_authority_discovery = 9,
        Babe: pallet_babe = 3,
        Grandpa: pallet_grandpa = 4,
        Balances: pallet_balances = 5,
        Vesting: pallet_vesting = 10,
        TransactionPayment: pallet_transaction_payment = 6,
        BagsList: pallet_bags_list = 11,
        ImOnline: pallet_im_online = 12,
        Staking: pallet_staking = 13,
        Session: pallet_session = 7,
        Treasury: pallet_treasury = 14,
        Historical: pallet_session_historical = 15,

        // Governance
        ConvictionVoting: pallet_conviction_voting = 16,
        Referenda: pallet_referenda = 17,
        FellowshipCollective: pallet_ranked_collective::<Instance1> = 18,
        FellowshipReferenda: pallet_referenda::<Instance2> = 19,
        Origins: pallet_custom_origins = 20,
        Whitelist: pallet_whitelist = 21,

        // TODO: Remove in stage 2
        ValidatorSet: validator_set = 98,
        Sudo: pallet_sudo = 99,

        Scheduler: pallet_scheduler = 22,
        Preimage: pallet_preimage = 23,
        Identity: pallet_identity = 24,
        Proxy: pallet_proxy = 25,
        Multisig: pallet_multisig = 26,
        Utility: pallet_utility = 8,
        GearProgram: pallet_gear_program = 100,
        GearMessenger: pallet_gear_messenger = 101,
        GearScheduler: pallet_gear_scheduler = 102,
        GearGas: pallet_gear_gas = 103,
        Gear: pallet_gear = 104,
        GearPayment: pallet_gear_payment = 105,
        StakingRewards: pallet_gear_staking_rewards = 106,
        GearVoucher: pallet_gear_voucher = 107,

        // TODO: remove from production version
        Airdrop: pallet_airdrop = 198,
    }
);

/// The address format for describing accounts.
pub type Address = sp_runtime::MultiAddress<AccountId, ()>;
/// Block header type as expected by this runtime.
pub type Header = generic::Header<BlockNumber, BlakeTwo256>;
/// Block type as expected by this runtime.
pub type Block = generic::Block<Header, UncheckedExtrinsic>;
/// The SignedExtension to the basic transaction logic.
pub type SignedExtra = (
    // RELEASE: remove before final release
    DisableValueTransfers,
    // Keep as long as it's needed
    StakingBlackList<Runtime>,
    frame_system::CheckNonZeroSender<Runtime>,
    frame_system::CheckSpecVersion<Runtime>,
    frame_system::CheckTxVersion<Runtime>,
    frame_system::CheckGenesis<Runtime>,
    frame_system::CheckEra<Runtime>,
    frame_system::CheckNonce<Runtime>,
    frame_system::CheckWeight<Runtime>,
    CustomChargeTransactionPayment<Runtime>,
);
/// Unchecked extrinsic type as expected by this runtime.
pub type UncheckedExtrinsic =
    generic::UncheckedExtrinsic<Address, RuntimeCall, Signature, SignedExtra>;
/// The payload being signed in transactions.
pub type SignedPayload = generic::SignedPayload<RuntimeCall, SignedExtra>;
/// Executive: handles dispatch to the various modules.
pub type Executive = frame_executive::Executive<
    Runtime,
    Block,
    frame_system::ChainContext<Runtime>,
    Runtime,
    AllPalletsWithSystem,
    migrations::Migrations,
>;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod integration_tests;

#[cfg(feature = "debug-mode")]
type DebugInfo = GearDebug;
#[cfg(not(feature = "debug-mode"))]
type DebugInfo = ();

#[cfg(feature = "runtime-benchmarks")]
#[macro_use]
extern crate frame_benchmarking;

#[cfg(feature = "runtime-benchmarks")]
mod benches {
    define_benchmarks!(
        // Substrate pallets
        [frame_system, SystemBench::<Runtime>]
        [pallet_balances, Balances]
        [pallet_timestamp, Timestamp]
        [pallet_utility, Utility]
        // Gear pallets
        [pallet_airdrop, Airdrop]
        [pallet_gear, Gear]
        [pallet_gear_voucher, GearVoucher]
    );
}

impl_runtime_apis_plus_common! {
    impl sp_consensus_babe::BabeApi<Block> for Runtime {
        fn configuration() -> sp_consensus_babe::BabeConfiguration {
            // The choice of `c` parameter (where `1 - c` represents the
            // probability of a slot being empty), is done in accordance to the
            // slot duration and expected target block time, for safely
            // resisting network delays of maximum two seconds.
            // <https://research.web3.foundation/en/latest/polkadot/BABE/Babe/#6-practical-results>
            sp_consensus_babe::BabeConfiguration {
                slot_duration: Babe::slot_duration(),
                epoch_length: EpochDuration::get(),
                c: BABE_GENESIS_EPOCH_CONFIG.c,
                authorities: Babe::authorities().to_vec(),
                randomness: Babe::randomness(),
                allowed_slots: BABE_GENESIS_EPOCH_CONFIG.allowed_slots,
            }
        }

        fn current_epoch_start() -> sp_consensus_babe::Slot {
            Babe::current_epoch_start()
        }

        fn current_epoch() -> sp_consensus_babe::Epoch {
            Babe::current_epoch()
        }

        fn next_epoch() -> sp_consensus_babe::Epoch {
            Babe::next_epoch()
        }

        fn generate_key_ownership_proof(
            _slot: sp_consensus_babe::Slot,
            authority_id: sp_consensus_babe::AuthorityId,
        ) -> Option<sp_consensus_babe::OpaqueKeyOwnershipProof> {
            Historical::prove((sp_consensus_babe::KEY_TYPE, authority_id))
                .map(|p| p.encode())
                .map(sp_consensus_babe::OpaqueKeyOwnershipProof::new)
        }

        fn submit_report_equivocation_unsigned_extrinsic(
            equivocation_proof: sp_consensus_babe::EquivocationProof<<Block as BlockT>::Header>,
            key_owner_proof: sp_consensus_babe::OpaqueKeyOwnershipProof,
        ) -> Option<()> {
            let key_owner_proof = key_owner_proof.decode()?;

            Babe::submit_unsigned_equivocation_report(
                equivocation_proof,
                key_owner_proof,
            )
        }

    }

    #[cfg(feature = "try-runtime")]
    impl frame_try_runtime::TryRuntime<Block> for Runtime {
        fn on_runtime_upgrade(checks: frame_try_runtime::UpgradeCheckSelect) -> (Weight, Weight) {
            // NOTE: intentional unwrap: we don't want to propagate the error backwards, and want to
            // have a backtrace here. If any of the pre/post migration checks fail, we shall stop
            // right here and right now.
            let weight = Executive::try_runtime_upgrade(checks).unwrap();
            (weight, RuntimeBlockWeights::get().max_block)
        }

        fn execute_block(
            block: Block,
            state_root_check: bool,
            signature_check: bool,
            select: frame_try_runtime::TryStateSelect
        ) -> Weight {
            log::info!(
                target: "node-runtime",
                "try-runtime: executing block {:?} / root checks: {:?} / signature_checks: {:?} / try-state-select: {:?}",
                block.header.hash(),
                state_root_check,
                signature_check,
                select,
            );
            // NOTE: intentional unwrap: we don't want to propagate the error backwards, and want to
            // have a backtrace here.
            Executive::try_execute_block(block, state_root_check, signature_check, select).unwrap()
        }
    }
}
