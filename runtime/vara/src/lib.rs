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

#![cfg_attr(not(feature = "std"), no_std)]
// `construct_runtime!` does a lot of recursion and requires us to increase the limit to 256.
#![recursion_limit = "256"]
#![allow(clippy::items_after_test_module)]
#![allow(clippy::legacy_numeric_constants)]
#![allow(non_local_definitions)]

#[cfg(feature = "runtime-benchmarks")]
#[macro_use]
extern crate frame_benchmarking;

// Make the WASM binary available.
#[cfg(all(feature = "std", not(fuzz)))]
include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));

use common::{DelegateFee, storage::Messenger};
use frame_election_provider_support::{
    ElectionDataProvider, NposSolution, SequentialPhragmen, VoteWeight,
    bounds::ElectionBoundsBuilder, onchain,
};
use frame_support::{
    dispatch::DispatchInfo,
    pallet_prelude::{
        InvalidTransaction, TransactionLongevity, TransactionValidityError, ValidTransaction,
    },
    weights::ConstantMultiplier,
};
use frame_system::{
    EnsureRoot,
    limits::{BlockLength, BlockWeights},
};
use gbuiltin_proxy::ProxyType as BuiltinProxyType;
use pallet_election_provider_multi_phase::{GeometricDepositBase, SolutionAccuracyOf};
use pallet_gear_builtin::ActorWithId;
use pallet_grandpa::{
    AuthorityId as GrandpaId, AuthorityList as GrandpaAuthorityList, fg_primitives,
};
use pallet_identity::legacy::IdentityInfo;
use pallet_im_online::sr25519::AuthorityId as ImOnlineId;
use pallet_session::historical as pallet_session_historical;
use parity_scale_codec::{Decode, Encode, MaxEncodedLen};
use runtime_primitives::{Balance, BlockNumber, Hash, Moment, Nonce};
use scale_info::TypeInfo;
use sp_api::impl_runtime_apis;
use sp_authority_discovery::AuthorityId as AuthorityDiscoveryId;
use sp_core::{ConstBool, ConstU8, ConstU64, H256, OpaqueMetadata, crypto::KeyTypeId};
use sp_runtime::{
    ApplyExtrinsicResult, FixedU128, Perbill, Percent, Permill, Perquintill, RuntimeDebug,
    create_runtime_str, generic, impl_opaque_keys,
    traits::{
        AccountIdConversion, AccountIdLookup, BlakeTwo256, Block as BlockT, ConvertInto,
        DispatchInfoOf, Dispatchable, IdentityLookup, NumberFor, One, SignedExtension,
    },
    transaction_validity::{TransactionPriority, TransactionSource, TransactionValidity},
};
use sp_std::{
    convert::{TryFrom, TryInto},
    prelude::*,
};
use sp_version::RuntimeVersion;

#[cfg(not(feature = "dev"))]
use sp_runtime::traits::OpaqueKeys;

#[cfg(any(feature = "std", test))]
use {
    sp_api::{CallApiAt, CallContext, ProofRecorder},
    sp_externalities::Extensions,
    sp_runtime::traits::HashingFor,
    sp_state_machine::OverlayedChanges,
};

pub use pallet_gear;
pub use pallet_gear_gas;
pub use pallet_gear_payment;

pub use frame_support::{
    PalletId, StorageValue, construct_runtime, derive_impl,
    dispatch::{DispatchClass, WeighData},
    genesis_builder_helper::{build_state, get_preset},
    parameter_types,
    traits::{
        ConstU16, ConstU32, ConstU128, Contains, Currency, EitherOf, EitherOfDiverse,
        EqualPrivilegeOnly, Everything, FindAuthor, InstanceFilter, KeyOwnerProofSystem,
        LinearStoragePrice, LockIdentifier, Nothing, OnUnbalanced, Randomness, SortedMembers,
        StorageInfo, VariantCountOf, WithdrawReasons,
        fungible::HoldConsideration,
        tokens::{PayFromAccount, UnityAssetBalanceConversion},
    },
    weights::{
        Weight,
        constants::{
            BlockExecutionWeight, ExtrinsicBaseWeight, RocksDbWeight, WEIGHT_REF_TIME_PER_MILLIS,
            WEIGHT_REF_TIME_PER_SECOND,
        },
    },
};
pub use gear_runtime_common::{
    AVERAGE_ON_INITIALIZE_RATIO, BlockHashCount, DealWithFees, GAS_LIMIT_MIN_PERCENTAGE_NUM,
    NORMAL_DISPATCH_LENGTH_RATIO, NORMAL_DISPATCH_WEIGHT_RATIO, VALUE_PER_GAS,
    constants::{
        RENT_DISABLED_DELTA_WEEK_FACTOR, RENT_FREE_PERIOD_MONTH_FACTOR, RENT_RESUME_WEEK_FACTOR,
        RESUME_SESSION_DURATION_HOUR_FACTOR,
    },
    impl_runtime_apis_plus_common,
};
pub use pallet_gear::manager::{ExtManager, HandleKind};
pub use pallet_gear_payment::CustomChargeTransactionPayment;
pub use pallet_gear_staking_rewards::StakingBlackList;
#[allow(deprecated)]
pub use pallet_transaction_payment::{
    CurrencyAdapter, FeeDetails, Multiplier, RuntimeDispatchInfo,
};
pub use runtime_primitives::{AccountId, Signature, VARA_SS58_PREFIX};

#[cfg(all(feature = "dev", any(feature = "std", test)))]
pub use pallet_sudo::Call as SudoCall;

#[cfg(any(feature = "std", test))]
pub use {
    frame_system::Call as SystemCall, pallet_balances::Call as BalancesCall,
    pallet_staking::StakerStatus, pallet_timestamp::Call as TimestampCall,
    sp_runtime::BuildStorage,
};

pub mod constants;
pub mod genesis_config_presets;

pub use constants::{currency::*, time::*};

// Weights used in the runtime.
mod weights;

// Voters weights
mod bag_thresholds;

pub mod governance;
use governance::{GeneralAdmin, StakingAdmin, Treasurer, TreasurySpender, pallet_custom_origins};

mod migrations;

// By this we assert if runtime compiled with "dev" feature.
#[cfg_attr(
    all(target_arch = "wasm32", feature = "dev"),
    unsafe(link_section = "dev_runtime")
)]
static _DEV_RUNTIME: u8 = 0;

// By this we inject compile time version including commit hash
// (https://github.com/paritytech/substrate/blob/297b3948f4a0f7f6504d4b654e16cb5d9201e523/utils/build-script-utils/src/version.rs#L44)
// into the WASM runtime blob. This is used by the `runtime_wasmBlobVersion` RPC call.
// The format of the version is `x.y.z-commit_hash`, where the `x.y.z` is the version of this crate,
// and the `commit_hash` is the hash of the commit from which the WASM blob was built.
#[cfg_attr(target_arch = "wasm32", unsafe(link_section = "wasm_blob_version"))]
static _WASM_BLOB_VERSION: [u8; const_str::to_byte_array!(env!("SUBSTRATE_CLI_IMPL_VERSION"))
    .len()] = const_str::to_byte_array!(env!("SUBSTRATE_CLI_IMPL_VERSION"));

/// Vara Network runtime version.
#[cfg(not(feature = "dev"))]
#[sp_version::runtime_version]
pub const VERSION: RuntimeVersion = RuntimeVersion {
    spec_name: create_runtime_str!("vara"),
    impl_name: create_runtime_str!("vara"),

    spec_version: 1910,

    apis: RUNTIME_API_VERSIONS,
    authoring_version: 1,
    impl_version: 1,
    state_version: 1,
    transaction_version: 1,
};

/// Vara Network Testnet (and dev) runtime version.
#[cfg(feature = "dev")]
#[sp_version::runtime_version]
pub const VERSION: RuntimeVersion = RuntimeVersion {
    spec_name: create_runtime_str!("vara-testnet"),
    impl_name: create_runtime_str!("vara-testnet"),

    spec_version: 1910,

    apis: RUNTIME_API_VERSIONS,
    authoring_version: 1,
    impl_version: 1,
    state_version: 1,
    transaction_version: 1,
};

/// The BABE epoch configuration at genesis.
pub const BABE_GENESIS_EPOCH_CONFIG: sp_consensus_babe::BabeEpochConfiguration =
    sp_consensus_babe::BabeEpochConfiguration {
        c: PRIMARY_PROBABILITY,
        allowed_slots: sp_consensus_babe::AllowedSlots::PrimaryAndSecondaryPlainSlots,
    };

// We'll verify that WEIGHT_REF_TIME_PER_SECOND does not overflow, allowing us to use
// simple multiply and divide operators instead of saturating or checked ones.
const _: () = assert!(WEIGHT_REF_TIME_PER_SECOND.checked_div(3).is_some());
const _: () = assert!((WEIGHT_REF_TIME_PER_SECOND / 3).checked_mul(2).is_some());

/// We allow for 1/3 of block time for computations, with maximum proof size.
///
/// It's 3/3 sec for vara runtime with 3 second block duration.
const MAXIMUM_BLOCK_WEIGHT: Weight = Weight::from_parts(
    WEIGHT_REF_TIME_PER_MILLIS * MILLISECS_PER_BLOCK / 3,
    u64::MAX,
);

parameter_types! {
    pub const Version: RuntimeVersion = VERSION;
    pub const SS58Prefix: u8 = VARA_SS58_PREFIX;
    pub RuntimeBlockWeights: BlockWeights = gear_runtime_common::block_weights_for(MAXIMUM_BLOCK_WEIGHT);
    pub RuntimeBlockLength: BlockLength =
        BlockLength::max_with_normal_ratio(5 * 1024 * 1024, NORMAL_DISPATCH_LENGTH_RATIO);
}

// Configure FRAME pallets to include in runtime.

#[derive_impl(frame_system::config_preludes::SolochainDefaultConfig)]
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
    /// The nonce type for storing how many extrinsics an account has signed.
    type Nonce = Nonce;
    /// The type for hashing blocks and tries.
    type Hash = Hash;
    /// The hashing algorithm used.
    type Hashing = BlakeTwo256;
    /// The block type.
    type Block = Block;
    /// The ubiquitous event type.
    type RuntimeEvent = RuntimeEvent;
    /// Contains an aggregation of all tasks in this runtime.
    type RuntimeTask = RuntimeTask;
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
    type MaxNominators = MaxNominators;

    type KeyOwnerProof = sp_session::MembershipProof;
    type EquivocationReportSystem =
        pallet_babe::EquivocationReportSystem<Self, Offences, Historical, ReportLongevity>;
}

impl pallet_grandpa::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;

    type WeightInfo = ();
    type MaxAuthorities = MaxAuthorities;
    type MaxNominators = MaxNominators;
    type MaxSetIdSessionEntries = MaxSetIdSessionEntries;
    type KeyOwnerProof = sp_session::MembershipProof;
    type EquivocationReportSystem =
        pallet_grandpa::EquivocationReportSystem<Self, Offences, Historical, ReportLongevity>;
}

impl pallet_authorship::Config for Runtime {
    type FindAuthor = pallet_session::FindAccountFromAuthorIndex<Self, Babe>;
    type EventHandler = (Staking, ImOnline);
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
    pub const PreimageBaseDeposit: Balance = ECONOMIC_UNITS;
    pub const PreimageByteDeposit: Balance = ECONOMIC_CENTIUNITS;
    pub const PreimageHoldReason: RuntimeHoldReason = RuntimeHoldReason::Preimage(pallet_preimage::HoldReason::Preimage);
}

impl pallet_preimage::Config for Runtime {
    type WeightInfo = pallet_preimage::weights::SubstrateWeight<Runtime>;
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type ManagerOrigin = EnsureRoot<AccountId>;
    type Consideration = HoldConsideration<
        AccountId,
        Balances,
        PreimageHoldReason,
        LinearStoragePrice<PreimageBaseDeposit, PreimageByteDeposit, Balance>,
    >;
}

parameter_types! {
    // For weight estimation, we assume that the most locks on an individual account will be 50.
    // This number may need to be adjusted in the future if this assumption no longer holds true.
    pub const MaxLocks: u32 = 50;
    pub const MaxReserves: u32 = 50;
}

impl pallet_balances::Config for Runtime {
    type RuntimeHoldReason = RuntimeHoldReason;
    type RuntimeFreezeReason = RuntimeFreezeReason;
    type MaxLocks = MaxLocks;
    type MaxReserves = MaxReserves;
    type ReserveIdentifier = [u8; 8];
    type Balance = Balance;
    type RuntimeEvent = RuntimeEvent;
    type DustRemoval = pallet_gear_staking_rewards::OffsetPoolDust<Self>;
    type ExistentialDeposit = ConstU128<EXISTENTIAL_DEPOSIT>;
    type AccountStore = System;
    type WeightInfo = weights::pallet_balances::SubstrateWeight<Runtime>;
    type FreezeIdentifier = RuntimeFreezeReason;
    type MaxFreezes = VariantCountOf<RuntimeFreezeReason>;
}

parameter_types! {
    pub const TransactionByteFee: Balance = 1;
    pub const QueueLengthStep: u128 = 1000;
    pub const OperationalFeeMultiplier: u8 = 5;
}

// Can't use `FungibleAdapter` here until Treasury pallet migrates to fungibles
// <https://github.com/paritytech/polkadot-sdk/issues/226>
#[allow(deprecated)]
impl pallet_transaction_payment::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type OnChargeTransaction = CurrencyAdapter<Balances, DealWithFees<Runtime>>;
    type OperationalFeeMultiplier = OperationalFeeMultiplier;
    type WeightToFee = ConstantMultiplier<u128, ConstU128<VALUE_PER_GAS>>;
    type LengthToFee = ConstantMultiplier<u128, ConstU128<VALUE_PER_GAS>>;
    type FeeMultiplierUpdate = pallet_gear_payment::GearFeeMultiplier<Runtime, QueueLengthStep>;
}

// **IMPORTANT**: update this value with care, GearEthBridge is sensitive to this.
impl_opaque_keys! {
    pub struct SessionKeys {
        pub babe: Babe,
        pub grandpa: Grandpa,
        pub im_online: ImOnline,
        pub authority_discovery: AuthorityDiscovery,
    }
}

#[cfg(feature = "dev")]
mod grandpa_keys_handler {
    use super::{AccountId, GearEthBridge, Grandpa};
    use frame_support::traits::OneSessionHandler;
    use sp_runtime::BoundToRuntimeAppPublic;
    use sp_std::vec::Vec;

    /// Due to requirement of pallet_session to have one keys handler for each
    /// type of opaque keys, this implementation is necessary: aggregates
    /// `Grandpa` and `GearEthBridge` handling of grandpa keys rotations.
    pub struct GrandpaAndGearEthBridge;

    impl BoundToRuntimeAppPublic for GrandpaAndGearEthBridge {
        type Public = <Grandpa as BoundToRuntimeAppPublic>::Public;
    }

    impl OneSessionHandler<AccountId> for GrandpaAndGearEthBridge {
        type Key = <Grandpa as OneSessionHandler<AccountId>>::Key;
        fn on_before_session_ending() {
            Grandpa::on_before_session_ending();
            GearEthBridge::on_before_session_ending();
        }
        fn on_disabled(validator_index: u32) {
            Grandpa::on_disabled(validator_index);
            GearEthBridge::on_disabled(validator_index);
        }
        fn on_genesis_session<'a, I>(validators: I)
        where
            I: 'a + Iterator<Item = (&'a AccountId, Self::Key)>,
            AccountId: 'a,
        {
            let validators: Vec<_> = validators.collect();
            Grandpa::on_genesis_session(validators.clone().into_iter());
            GearEthBridge::on_genesis_session(validators.into_iter());
        }
        fn on_new_session<'a, I>(changed: bool, validators: I, queued_validators: I)
        where
            I: 'a + Iterator<Item = (&'a AccountId, Self::Key)>,
            AccountId: 'a,
        {
            let validators: Vec<_> = validators.collect();
            let queued_validators: Vec<_> = queued_validators.collect();
            Grandpa::on_new_session(
                changed,
                validators.clone().into_iter(),
                queued_validators.clone().into_iter(),
            );
            GearEthBridge::on_new_session(
                changed,
                validators.into_iter(),
                queued_validators.into_iter(),
            );
        }
    }
}

#[cfg(feature = "dev")]
pub type VaraSessionHandler = (
    Babe,
    grandpa_keys_handler::GrandpaAndGearEthBridge,
    ImOnline,
    AuthorityDiscovery,
);

#[cfg(not(feature = "dev"))]
pub type VaraSessionHandler = <SessionKeys as OpaqueKeys>::KeyTypeIdProviders;

impl pallet_session::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type ValidatorId = <Self as frame_system::Config>::AccountId;
    type ValidatorIdOf = pallet_staking::StashOf<Self>;
    type ShouldEndSession = Babe;
    type NextSessionRotation = Babe;
    // **IMPORTANT**: update this value with care, GearEthBridge is sensitive to this.
    type SessionManager = pallet_session_historical::NoteHistoricalRoot<Self, Staking>;
    type SessionHandler = VaraSessionHandler;
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

parameter_types! {
    // phase durations. 1/4 of the last session for each.
    pub const SignedPhase: u32 = EPOCH_DURATION_IN_BLOCKS / 4;
    pub const UnsignedPhase: u32 = EPOCH_DURATION_IN_BLOCKS / 4;

    // signed config
    pub const SignedFixedDeposit: Balance = deposit(2, 0);
    pub const SignedDepositIncreaseFactor: Percent = Percent::from_percent(10);

    pub const SignedRewardBase: Balance = ECONOMIC_UNITS;
    pub const SignedDepositBase: Balance = ECONOMIC_UNITS;
    pub const SignedDepositByte: Balance = ECONOMIC_CENTIUNITS;

    // miner configs
    pub const MultiPhaseUnsignedPriority: TransactionPriority = StakingUnsignedPriority::get() - 1u64;
    pub MinerMaxWeight: Weight = RuntimeBlockWeights::get()
        .get(DispatchClass::Normal)
        .max_extrinsic.expect("Normal extrinsics have a weight limit configured; qed")
        .saturating_sub(BlockExecutionWeight::get());
    // Solution can occupy 90% of normal block size
    pub MinerMaxLength: u32 = Perbill::from_rational(9u32, 10) *
        *RuntimeBlockLength::get()
        .max
        .get(DispatchClass::Normal);
}

frame_election_provider_support::generate_solution_type!(
    #[compact]
    pub struct NposSolution16::<
        VoterIndex = u32,
        TargetIndex = u16,
        Accuracy = sp_runtime::PerU16,
        MaxVoters = MaxElectingVoters,
    >(16)
);

parameter_types! {
    // 16; TODO: Kusama has 24 => which one is more appropriate?
    pub const MaxNominations: u32 = <NposSolution16 as NposSolution>::LIMIT as u32;
    pub MaxElectingVoters: u32 = 40_000;
    /// We take the top 40_000 nominators as electing voters and all of the validators as electable
    /// targets. Whilst this is the case, we cannot and shall not increase the size of the
    /// validator intentions.
    pub ElectionBounds: frame_election_provider_support::bounds::ElectionBounds =
        ElectionBoundsBuilder::default().voters_count(MaxElectingVoters::get().into()).build();
    // OnChain values are lower.
    pub MaxOnChainElectingVoters: u32 = 5000;
    pub MaxOnChainElectableTargets: u16 = 1250;
    // The maximum winners that can be elected by the Election pallet which is equivalent to the
    // maximum active validators the staking pallet can have.
    pub MaxActiveValidators: u32 = 1000;
}

pub struct OnChainSeqPhragmen;
impl onchain::Config for OnChainSeqPhragmen {
    type System = Runtime;
    type Solver = SequentialPhragmen<
        AccountId,
        pallet_election_provider_multi_phase::SolutionAccuracyOf<Runtime>,
    >;
    type DataProvider = <Runtime as pallet_election_provider_multi_phase::Config>::DataProvider;
    type WeightInfo = frame_election_provider_support::weights::SubstrateWeight<Runtime>;
    type MaxWinners = <Runtime as pallet_election_provider_multi_phase::Config>::MaxWinners;
    type Bounds = ElectionBounds;
}

impl pallet_election_provider_multi_phase::MinerConfig for Runtime {
    type AccountId = AccountId;
    type MaxLength = MinerMaxLength;
    type MaxWeight = MinerMaxWeight;
    type Solution = NposSolution16;
    type MaxVotesPerVoter =
    <<Self as pallet_election_provider_multi_phase::Config>::DataProvider as ElectionDataProvider>::MaxVotesPerVoter;
    type MaxWinners = MaxActiveValidators;

    // The unsigned submissions have to respect the weight of the submit_unsigned call, thus their
    // weight estimate function is wired to this call's weight.
    fn solution_weight(v: u32, t: u32, a: u32, d: u32) -> Weight {
        <
            <Self as pallet_election_provider_multi_phase::Config>::WeightInfo
            as
            pallet_election_provider_multi_phase::WeightInfo
        >::submit_unsigned(v, t, a, d)
    }
}
pub struct ElectionProviderBenchmarkConfig;
impl pallet_election_provider_multi_phase::BenchmarkingConfig for ElectionProviderBenchmarkConfig {
    const VOTERS: [u32; 2] = [1000, 2000];
    const TARGETS: [u32; 2] = [500, 1000];
    const ACTIVE_VOTERS: [u32; 2] = [500, 800];
    const DESIRED_TARGETS: [u32; 2] = [200, 400];
    const SNAPSHOT_MAXIMUM_VOTERS: u32 = 1000;
    const MINER_MAXIMUM_VOTERS: u32 = 1000;
    const MAXIMUM_TARGETS: u32 = 300;
}

impl pallet_election_provider_multi_phase::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type EstimateCallFee = TransactionPayment;
    type SignedPhase = SignedPhase;
    type UnsignedPhase = UnsignedPhase;
    type BetterSignedThreshold = ();
    type OffchainRepeat = OffchainRepeat;
    type MinerTxPriority = MultiPhaseUnsignedPriority;
    type MinerConfig = Self;
    type SignedMaxSubmissions = ConstU32<10>;
    type SignedRewardBase = SignedRewardBase;
    type SignedDepositBase =
        GeometricDepositBase<Balance, SignedFixedDeposit, SignedDepositIncreaseFactor>;
    type SignedDepositByte = SignedDepositByte;
    type SignedMaxRefunds = ConstU32<3>;
    type SignedDepositWeight = ();
    type SignedMaxWeight = MinerMaxWeight;
    type SlashHandler = Treasury;
    type RewardHandler = StakingRewards;
    type DataProvider = Staking;
    type Fallback = onchain::OnChainExecution<OnChainSeqPhragmen>;
    type GovernanceFallback = onchain::OnChainExecution<OnChainSeqPhragmen>;
    type Solver = SequentialPhragmen<AccountId, SolutionAccuracyOf<Self>, ()>;
    type ForceOrigin = AdminOrigin;
    type MaxWinners = MaxActiveValidators;
    type ElectionBounds = ElectionBounds;
    type BenchmarkingConfig = ElectionProviderBenchmarkConfig;
    type WeightInfo = pallet_election_provider_multi_phase::weights::SubstrateWeight<Self>;
}

parameter_types! {
    // Six sessions in an era (12 hours)
    // **IMPORTANT**: update this value with care, GearEthBridge is sensitive to this.
    pub const SessionsPerEra: sp_staking::SessionIndex = 6;
    // 42 eras for unbonding (7 days)
    pub const BondingDuration: sp_staking::EraIndex = 14;
    // 41 eras during which slashes can be cancelled (slightly less than 7 days)
    pub const SlashDeferDuration: sp_staking::EraIndex = 13;
    pub const MaxExposurePageSize: u32 = 256;
    // Note: this is not really correct as Max Nominators is (MaxExposurePageSize * page_count) but
    // this is an unbounded number. We just set it to a reasonably high value, 1 full page
    // of nominators.
    pub const MaxNominators: u32 = 512;
    pub const MaxControllersInDeprecationBatch: u32 = 5900;
    pub const OffendingValidatorsThreshold: Perbill = Perbill::from_percent(17);
    // 2 hour session, 30 min unsigned phase, 16 offchain executions.
    pub OffchainRepeat: BlockNumber = UnsignedPhase::get() / 16;
    pub HistoryDepth: u32 = 84;
}

/// Only the root or staking admin origin can cancel the slash or manage election provider
type AdminOrigin = EitherOfDiverse<EnsureRoot<AccountId>, StakingAdmin>;

pub struct StakingBenchmarkingConfig;
impl pallet_staking::BenchmarkingConfig for StakingBenchmarkingConfig {
    type MaxNominators = ConstU32<1000>;
    type MaxValidators = ConstU32<1000>;
}

impl pallet_staking::Config for Runtime {
    type Currency = Balances;
    type CurrencyBalance = Balance;
    type UnixTime = Timestamp;
    type CurrencyToVote = sp_staking::currency_to_vote::U128CurrencyToVote;
    type ElectionProvider = ElectionProviderMultiPhase;
    type GenesisElectionProvider = onchain::OnChainExecution<OnChainSeqPhragmen>;
    // Burning the reward remainder for now.
    // TODO: set remainder back to `RewardProxy<Self, Treasury>` to stop burning `Treasury` part.
    type RewardRemainder = ();
    type RuntimeEvent = RuntimeEvent;
    type Slash = Treasury;
    type Reward = StakingRewards;
    // **IMPORTANT**: update this value with care, GearEthBridge is sensitive to this.
    type SessionsPerEra = SessionsPerEra;
    type BondingDuration = BondingDuration;
    type SlashDeferDuration = SlashDeferDuration;
    type AdminOrigin = AdminOrigin;
    type SessionInterface = Self;
    type EraPayout = StakingRewards;
    type NextNewSession = Session;
    type MaxExposurePageSize = MaxExposurePageSize;
    type VoterList = BagsList;
    type TargetList = pallet_staking::UseValidatorsMap<Self>;
    type NominationsQuota = pallet_staking::FixedNominationsQuota<{ MaxNominations::get() }>;
    type MaxUnlockingChunks = ConstU32<32>;
    type MaxControllersInDeprecationBatch = MaxControllersInDeprecationBatch;
    type HistoryDepth = HistoryDepth;
    type EventListeners = NominationPools;
    type WeightInfo = pallet_staking::weights::SubstrateWeight<Runtime>;
    type BenchmarkingConfig = StakingBenchmarkingConfig;
    type DisablingStrategy = pallet_staking::UpToLimitDisablingStrategy;
}

parameter_types! {
    pub const BagThresholds: &'static [u64] = &bag_thresholds::THRESHOLDS;
}

impl pallet_bags_list::Config<pallet_bags_list::Instance1> for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type ScoreProvider = Staking;
    type BagThresholds = BagThresholds;
    type Score = VoteWeight;
    type WeightInfo = pallet_bags_list::weights::SubstrateWeight<Runtime>;
}

parameter_types! {
    pub const NominationPoolsPalletId: PalletId = PalletId(*b"py/nopls");
    pub const MaxPointsToBalance: u8 = 10;
}

use sp_runtime::traits::Convert;
pub struct BalanceToU256;
impl Convert<Balance, sp_core::U256> for BalanceToU256 {
    fn convert(balance: Balance) -> sp_core::U256 {
        sp_core::U256::from(balance)
    }
}
pub struct U256ToBalance;
impl Convert<sp_core::U256, Balance> for U256ToBalance {
    fn convert(n: sp_core::U256) -> Balance {
        n.try_into().unwrap_or(Balance::max_value())
    }
}

impl pallet_nomination_pools::Config for Runtime {
    type WeightInfo = ();
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type RuntimeFreezeReason = RuntimeFreezeReason;
    type RewardCounter = FixedU128;
    type BalanceToU256 = BalanceToU256;
    type U256ToBalance = U256ToBalance;
    type StakeAdapter = pallet_nomination_pools::adapter::TransferStake<Self, Staking>;
    type PostUnbondingPoolsWindow = ConstU32<4>;
    type MaxMetadataLen = ConstU32<256>;
    // we use the same number of allowed unlocking chunks as with staking.
    type MaxUnbonding = <Self as pallet_staking::Config>::MaxUnlockingChunks;
    type PalletId = NominationPoolsPalletId;
    type MaxPointsToBalance = MaxPointsToBalance;
    type AdminOrigin = EnsureRoot<AccountId>;
}

impl pallet_offences::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type IdentificationTuple = pallet_session_historical::IdentificationTuple<Self>;
    type OnOffenceHandler = Staking;
}

parameter_types! {
    pub const ProposalBond: Permill = Permill::from_percent(5);
    pub const ProposalBondMinimum: Balance = ECONOMIC_UNITS;
    pub const SpendPeriod: BlockNumber = DAYS;
    pub const Burn: Permill = Permill::zero();
    pub const TipCountdown: BlockNumber = DAYS;
    pub const TipFindersFee: Percent = Percent::from_percent(20);
    pub const TipReportDepositBase: Balance = ECONOMIC_UNITS;
    pub const DataDepositPerByte: Balance = ECONOMIC_CENTIUNITS;
    pub const TreasuryPalletId: PalletId = PalletId(*b"py/trsry");
    pub const PayoutSpendPeriod: BlockNumber = 30 * DAYS;
    pub TreasuryAccount: AccountId = Treasury::account_id();
    pub const MaximumReasonLength: u32 = 300;
    pub const MaxApprovals: u32 = 100;
    pub const MaxBalance: Balance = Balance::max_value();
}

impl pallet_treasury::Config for Runtime {
    type PalletId = TreasuryPalletId;
    type Currency = Balances;
    type RejectOrigin = EitherOfDiverse<EnsureRoot<AccountId>, Treasurer>;
    type RuntimeEvent = RuntimeEvent;
    type SpendPeriod = SpendPeriod;
    type Burn = Burn;
    type BurnDestination = ();
    type SpendFunds = Bounties;
    type WeightInfo = pallet_treasury::weights::SubstrateWeight<Runtime>;
    type MaxApprovals = MaxApprovals;
    type SpendOrigin = TreasurySpender;
    type AssetKind = ();
    type Beneficiary = Self::AccountId;
    type BeneficiaryLookup = IdentityLookup<Self::Beneficiary>;
    type Paymaster = PayFromAccount<Balances, TreasuryAccount>;
    type BalanceConverter = UnityAssetBalanceConversion;
    type PayoutPeriod = PayoutSpendPeriod;
}

parameter_types! {
    pub const BountyCuratorDeposit: Permill = Permill::from_percent(50);
    pub const BountyValueMinimum: Balance = 5 * ECONOMIC_UNITS;
    pub const BountyDepositBase: Balance = ECONOMIC_UNITS;
    pub const CuratorDepositMultiplier: Permill = Permill::from_percent(50);
    pub const CuratorDepositMin: Balance = ECONOMIC_UNITS;
    pub const CuratorDepositMax: Balance = 100 * ECONOMIC_UNITS;
    pub const BountyDepositPayoutDelay: BlockNumber = DAYS;
    pub const BountyUpdatePeriod: BlockNumber = 14 * DAYS;
}

impl pallet_bounties::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type BountyDepositBase = BountyDepositBase;
    type BountyDepositPayoutDelay = BountyDepositPayoutDelay;
    type BountyUpdatePeriod = BountyUpdatePeriod;
    type CuratorDepositMultiplier = CuratorDepositMultiplier;
    type CuratorDepositMin = CuratorDepositMin;
    type CuratorDepositMax = CuratorDepositMax;
    type BountyValueMinimum = BountyValueMinimum;
    type DataDepositPerByte = DataDepositPerByte;
    type MaximumReasonLength = MaximumReasonLength;
    type WeightInfo = pallet_bounties::weights::SubstrateWeight<Runtime>;
    type ChildBountyManager = ChildBounties;
    type OnSlash = Treasury;
}

parameter_types! {
    pub const ChildBountyValueMinimum: Balance = ECONOMIC_UNITS;
}

impl pallet_child_bounties::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type MaxActiveChildBountyCount = ConstU32<5>;
    type ChildBountyValueMinimum = ChildBountyValueMinimum;
    type WeightInfo = pallet_child_bounties::weights::SubstrateWeight<Runtime>;
}

parameter_types! {
    pub const ImOnlineUnsignedPriority: TransactionPriority = TransactionPriority::max_value();
    /// We prioritize im-online heartbeats over election solution submission.
    pub const StakingUnsignedPriority: TransactionPriority = TransactionPriority::max_value() / 2;
    pub const MaxAuthorities: u32 = 100_000;
    pub const MaxKeys: u32 = 10_000;
    pub const MaxPeerInHeartbeats: u32 = 10_000;
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
}

impl pallet_authority_discovery::Config for Runtime {
    type MaxAuthorities = MaxAuthorities;
}

parameter_types! {
    pub const BasicDeposit: Balance = 10 * ECONOMIC_UNITS;       // 258 bytes on-chain
    pub const ByteDeposit: Balance = deposit(0, 1);
    pub const SubAccountDeposit: Balance = 2 * ECONOMIC_UNITS;   // 53 bytes on-chain
    pub const MaxSubAccounts: u32 = 100;
    pub const MaxAdditionalFields: u32 = 100;
    pub const MaxRegistrars: u32 = 20;
}

impl pallet_identity::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type BasicDeposit = BasicDeposit;
    type ByteDeposit = ByteDeposit;
    type SubAccountDeposit = SubAccountDeposit;
    type MaxSubAccounts = MaxSubAccounts;
    type IdentityInformation = IdentityInfo<MaxAdditionalFields>;
    type MaxRegistrars = MaxRegistrars;
    type Slashed = Treasury;
    type ForceOrigin = EitherOf<EnsureRoot<AccountId>, GeneralAdmin>;
    type RegistrarOrigin = EitherOf<EnsureRoot<AccountId>, GeneralAdmin>;
    type OffchainSignature = Signature;
    type SigningPublicKey = <Signature as sp_runtime::traits::Verify>::Signer;
    type UsernameAuthorityOrigin = EnsureRoot<Self::AccountId>;
    type PendingUsernameExpiration = ConstU32<{ 7 * DAYS }>;
    type MaxSuffixLength = ConstU32<7>;
    type MaxUsernameLength = ConstU32<32>;
    type WeightInfo = pallet_identity::weights::SubstrateWeight<Runtime>;
}

#[cfg(feature = "dev")]
impl pallet_sudo::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type RuntimeCall = RuntimeCall;
    type WeightInfo = ();
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
            ProxyType::NonTransfer => {
                // Dev pallets.
                #[cfg(feature = "dev")]
                if matches!(c, |RuntimeCall::GearEthBridge(..)| RuntimeCall::Sudo(..)) {
                    return false;
                }

                !matches!(
                    c,
                    // Classic pallets.
                    RuntimeCall::Balances(..) | RuntimeCall::Vesting(..)
                    // Gear pallets.
                    | RuntimeCall::Gear(..)
                    | RuntimeCall::GearVoucher(..)
                    | RuntimeCall::StakingRewards(..)
                )
            }
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
    // 64 MB, must be less than max runtime heap memory.
    // NOTE: currently runtime heap memory is 1 GB (see https://shorturl.at/DET45)
    pub const OutgoingBytesLimit: u32 = 64 * 1024 * 1024;
    pub const MailboxThreshold: u64 = 3000;

    pub const PerformanceMultiplier: u32 = 100;
}

parameter_types! {
    pub Schedule: pallet_gear::Schedule<Runtime> = Default::default();
    pub const BankPalletId: PalletId = PalletId(*b"py/gbank");
    pub const GasMultiplier: common::GasMultiplier<Balance, u64> = common::GasMultiplier::ValuePerGas(VALUE_PER_GAS);
    pub const TreasuryGasFeeShare: Percent = Percent::one();
    pub const TreasuryTxFeeShare: Percent = Percent::one();
}

impl pallet_gear_bank::Config for Runtime {
    type Currency = Balances;
    type PalletId = BankPalletId;
    type GasMultiplier = GasMultiplier;
    type TreasuryAddress = TreasuryAccount;
    type TreasuryGasFeeShare = TreasuryGasFeeShare;
    type TreasuryTxFeeShare = TreasuryTxFeeShare;
}

impl pallet_gear::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Randomness = pallet_babe::RandomnessFromOneEpochAgo<Runtime>;
    type WeightInfo = pallet_gear::weights::SubstrateWeight<Runtime>;
    type Schedule = Schedule;
    type OutgoingLimit = OutgoingLimit;
    type OutgoingBytesLimit = OutgoingBytesLimit;
    type PerformanceMultiplier = PerformanceMultiplier;
    type CodeStorage = GearProgram;
    type ProgramStorage = GearProgram;
    type MailboxThreshold = MailboxThreshold;
    type ReservationsLimit = ConstU64<256>;
    type Messenger = GearMessenger;
    type GasProvider = GearGas;
    type BlockLimiter = GearGas;
    type Scheduler = GearScheduler;
    type QueueRunner = Gear;
    type BuiltinDispatcherFactory = GearBuiltin;
    type ProgramRentFreePeriod = ConstU32<{ MONTHS * RENT_FREE_PERIOD_MONTH_FACTOR }>;
    type ProgramResumeMinimalRentPeriod = ConstU32<{ WEEKS * RENT_RESUME_WEEK_FACTOR }>;
    type ProgramRentCostPerBlock = ConstU128<RENT_COST_PER_BLOCK>;
    type ProgramResumeSessionDuration = ConstU32<{ HOURS * RESUME_SESSION_DURATION_HOUR_FACTOR }>;

    #[cfg(feature = "runtime-benchmarks")]
    type ProgramRentEnabled = ConstBool<true>;

    #[cfg(not(feature = "runtime-benchmarks"))]
    type ProgramRentEnabled = ConstBool<false>;

    type ProgramRentDisabledDelta = ConstU32<{ WEEKS * RENT_DISABLED_DELTA_WEEK_FACTOR }>;
    type RentPoolId = pallet_gear_staking_rewards::RentPoolId<Self>;
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

/// Builtin actors arranged in a tuple.
///
/// # Security
/// Make sure to mint ED for each new builtin actor added to the tuple.
#[cfg(not(feature = "dev"))]
pub type BuiltinActors = (
    ActorWithId<1, pallet_gear_builtin::bls12_381::Actor<Runtime>>,
    ActorWithId<2, pallet_gear_builtin::staking::Actor<Runtime>>,
    // The ID = 3 is for the pallet_gear_eth_bridge::Actor.
    ActorWithId<4, pallet_gear_builtin::proxy::Actor<Runtime>>,
);

#[cfg(feature = "dev")]
const ETH_BRIDGE_BUILTIN_ID: u64 = 3;

/// Builtin actors arranged in a tuple.
///
/// # Security
/// Make sure to mint ED for each new builtin actor added to the tuple.
#[cfg(feature = "dev")]
pub type BuiltinActors = (
    ActorWithId<1, pallet_gear_builtin::bls12_381::Actor<Runtime>>,
    ActorWithId<2, pallet_gear_builtin::staking::Actor<Runtime>>,
    ActorWithId<{ ETH_BRIDGE_BUILTIN_ID }, pallet_gear_eth_bridge::Actor<Runtime>>,
    ActorWithId<4, pallet_gear_builtin::proxy::Actor<Runtime>>,
);

impl pallet_gear_builtin::Config for Runtime {
    type RuntimeCall = RuntimeCall;
    type Builtins = BuiltinActors;
    type BlockLimiter = GearGas;
    type WeightInfo = pallet_gear_builtin::weights::SubstrateWeight<Runtime>;
}

parameter_types! {
    pub const GearEthBridgePalletId: PalletId = PalletId(*b"py/gethb");

    pub GearEthBridgeAdminAccount: AccountId = GearEthBridgePalletId::get().into_sub_account_truncating("bridge_admin");
    pub GearEthBridgePauserAccount: AccountId = GearEthBridgePalletId::get().into_sub_account_truncating("bridge_pauser");
}

#[cfg(feature = "dev")]
parameter_types! {
    pub GearEthBridgeBuiltinAddress: AccountId
        = GearBuiltin::generate_actor_id(ETH_BRIDGE_BUILTIN_ID).into_bytes().into();
}

#[cfg(feature = "dev")]
pub struct GearEthBridgeAdminAccounts;
#[cfg(feature = "dev")]
impl SortedMembers<AccountId> for GearEthBridgeAdminAccounts {
    fn sorted_members() -> Vec<AccountId> {
        vec![GearEthBridgeAdminAccount::get()]
    }
}

#[cfg(feature = "dev")]
impl pallet_gear_eth_bridge::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type PalletId = GearEthBridgePalletId;
    type BuiltinAddress = GearEthBridgeBuiltinAddress;
    type AdminOrigin = frame_system::EnsureSignedBy<GearEthBridgeAdminAccounts, AccountId>;
    type MaxPayloadSize = ConstU32<16_384>; // 16 KiB
    type QueueCapacity = ConstU32<2048>;
    type SessionsPerEra = SessionsPerEra;
    type BridgeAdmin = GearEthBridgeAdminAccount;
    type BridgePauser = GearEthBridgePauserAccount;
    type WeightInfo = pallet_gear_eth_bridge::weights::SubstrateWeight<Runtime>;
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
            RuntimeCall::GearVoucher(voucher_call) => voucher_call.get_sponsor(who.clone()),
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
    pub const MinVoucherDuration: BlockNumber = MINUTES;
    pub const MaxVoucherDuration: BlockNumber = 3 * MONTHS;
}

impl pallet_gear_voucher::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type PalletId = VoucherPalletId;
    type WeightInfo = pallet_gear_voucher::weights::SubstrateWeight<Runtime>;
    type CallsDispatcher = pallet_gear::PrepaidCallDispatcher<Runtime>;
    type Mailbox = <GearMessenger as Messenger>::Mailbox;
    type MaxProgramsAmount = ConstU8<32>;
    type MaxDuration = MaxVoucherDuration;
    type MinDuration = MinVoucherDuration;
}

impl<C> frame_system::offchain::SendTransactionTypes<C> for Runtime
where
    RuntimeCall: From<C>,
{
    type Extrinsic = UncheckedExtrinsic;
    type OverarchingCall = RuntimeCall;
}

parameter_types! {
    pub const MinVestedTransfer: Balance = 100 * ECONOMIC_CENTIUNITS;
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
    type BlockNumberProvider = System;
    const MAX_VESTING_SCHEDULES: u32 = 28;
}

// Create the runtime by composing the FRAME pallets that were previously configured.
#[cfg(feature = "dev")]
#[frame_support::runtime]
mod runtime {
    #[runtime::runtime]
    #[runtime::derive(
        RuntimeCall,
        RuntimeEvent,
        RuntimeError,
        RuntimeOrigin,
        RuntimeFreezeReason,
        RuntimeHoldReason,
        RuntimeSlashReason,
        RuntimeLockId,
        RuntimeTask
    )]
    pub struct Runtime;

    #[runtime::pallet_index(0)]
    pub type System = frame_system;

    #[runtime::pallet_index(1)]
    pub type Timestamp = pallet_timestamp;

    #[runtime::pallet_index(2)]
    pub type Authorship = pallet_authorship;

    #[runtime::pallet_index(9)]
    pub type AuthorityDiscovery = pallet_authority_discovery;

    #[runtime::pallet_index(3)]
    pub type Babe = pallet_babe;

    #[runtime::pallet_index(4)]
    pub type Grandpa = pallet_grandpa;

    #[runtime::pallet_index(5)]
    pub type Balances = pallet_balances;

    #[runtime::pallet_index(10)]
    pub type Vesting = pallet_vesting;

    #[runtime::pallet_index(6)]
    pub type TransactionPayment = pallet_transaction_payment;

    #[runtime::pallet_index(11)]
    pub type BagsList = pallet_bags_list<Instance1>;

    #[runtime::pallet_index(12)]
    pub type ImOnline = pallet_im_online;

    #[runtime::pallet_index(13)]
    pub type Staking = pallet_staking;

    #[runtime::pallet_index(7)]
    pub type Session = pallet_session;

    #[runtime::pallet_index(14)]
    pub type Treasury = pallet_treasury;

    #[runtime::pallet_index(15)]
    pub type Historical = pallet_session_historical;

    #[runtime::pallet_index(8)]
    pub type Utility = pallet_utility;

    // Governance

    #[runtime::pallet_index(16)]
    pub type ConvictionVoting = pallet_conviction_voting;

    #[runtime::pallet_index(17)]
    pub type Referenda = pallet_referenda;

    #[runtime::pallet_index(18)]
    pub type FellowshipCollective = pallet_ranked_collective<Instance1>;

    #[runtime::pallet_index(19)]
    pub type FellowshipReferenda = pallet_referenda<Instance2>;

    #[runtime::pallet_index(20)]
    pub type Origins = pallet_custom_origins;

    #[runtime::pallet_index(21)]
    pub type Whitelist = pallet_whitelist;

    #[runtime::pallet_index(22)]
    pub type Scheduler = pallet_scheduler;

    #[runtime::pallet_index(23)]
    pub type Preimage = pallet_preimage;

    #[runtime::pallet_index(24)]
    pub type Identity = pallet_identity;

    #[runtime::pallet_index(25)]
    pub type Proxy = pallet_proxy;

    #[runtime::pallet_index(26)]
    pub type Multisig = pallet_multisig;

    #[runtime::pallet_index(27)]
    pub type ElectionProviderMultiPhase = pallet_election_provider_multi_phase;

    #[runtime::pallet_index(28)]
    pub type Offences = pallet_offences;

    #[runtime::pallet_index(29)]
    pub type Bounties = pallet_bounties;

    #[runtime::pallet_index(30)]
    pub type ChildBounties = pallet_child_bounties;

    #[runtime::pallet_index(31)]
    pub type NominationPools = pallet_nomination_pools;

    // Gear
    // NOTE (!): if adding new pallet, don't forget to extend non payable proxy filter.

    #[runtime::pallet_index(100)]
    pub type GearProgram = pallet_gear_program;

    #[runtime::pallet_index(101)]
    pub type GearMessenger = pallet_gear_messenger;

    #[runtime::pallet_index(102)]
    pub type GearScheduler = pallet_gear_scheduler;

    #[runtime::pallet_index(103)]
    pub type GearGas = pallet_gear_gas;

    #[runtime::pallet_index(104)]
    pub type Gear = pallet_gear;

    #[runtime::pallet_index(105)]
    pub type GearPayment = pallet_gear_payment;

    #[runtime::pallet_index(106)]
    pub type StakingRewards = pallet_gear_staking_rewards;

    #[runtime::pallet_index(107)]
    pub type GearVoucher = pallet_gear_voucher;

    #[runtime::pallet_index(108)]
    pub type GearBank = pallet_gear_bank;

    #[runtime::pallet_index(109)]
    pub type GearBuiltin = pallet_gear_builtin;

    #[runtime::pallet_index(110)]
    pub type GearEthBridge = pallet_gear_eth_bridge;

    #[runtime::pallet_index(99)]
    pub type Sudo = pallet_sudo;

    // NOTE (!): `pallet_airdrop` used to be idx(198).
    // NOTE (!): `pallet_gear_debug` used to be idx(199).
}

#[cfg(not(feature = "dev"))]
#[frame_support::runtime]
mod runtime {
    #[runtime::runtime]
    #[runtime::derive(
        RuntimeCall,
        RuntimeEvent,
        RuntimeError,
        RuntimeOrigin,
        RuntimeFreezeReason,
        RuntimeHoldReason,
        RuntimeSlashReason,
        RuntimeLockId,
        RuntimeTask
    )]
    pub struct Runtime;

    #[runtime::pallet_index(0)]
    pub type System = frame_system;

    #[runtime::pallet_index(1)]
    pub type Timestamp = pallet_timestamp;

    #[runtime::pallet_index(2)]
    pub type Authorship = pallet_authorship;

    #[runtime::pallet_index(9)]
    pub type AuthorityDiscovery = pallet_authority_discovery;

    #[runtime::pallet_index(3)]
    pub type Babe = pallet_babe;

    #[runtime::pallet_index(4)]
    pub type Grandpa = pallet_grandpa;

    #[runtime::pallet_index(5)]
    pub type Balances = pallet_balances;

    #[runtime::pallet_index(10)]
    pub type Vesting = pallet_vesting;

    #[runtime::pallet_index(6)]
    pub type TransactionPayment = pallet_transaction_payment;

    #[runtime::pallet_index(11)]
    pub type BagsList = pallet_bags_list<Instance1>;

    #[runtime::pallet_index(12)]
    pub type ImOnline = pallet_im_online;

    #[runtime::pallet_index(13)]
    pub type Staking = pallet_staking;

    #[runtime::pallet_index(7)]
    pub type Session = pallet_session;

    #[runtime::pallet_index(14)]
    pub type Treasury = pallet_treasury;

    #[runtime::pallet_index(15)]
    pub type Historical = pallet_session_historical;

    #[runtime::pallet_index(8)]
    pub type Utility = pallet_utility;

    // Governance

    #[runtime::pallet_index(16)]
    pub type ConvictionVoting = pallet_conviction_voting;

    #[runtime::pallet_index(17)]
    pub type Referenda = pallet_referenda;

    #[runtime::pallet_index(18)]
    pub type FellowshipCollective = pallet_ranked_collective<Instance1>;

    #[runtime::pallet_index(19)]
    pub type FellowshipReferenda = pallet_referenda<Instance2>;

    #[runtime::pallet_index(20)]
    pub type Origins = pallet_custom_origins;

    #[runtime::pallet_index(21)]
    pub type Whitelist = pallet_whitelist;

    #[runtime::pallet_index(22)]
    pub type Scheduler = pallet_scheduler;

    #[runtime::pallet_index(23)]
    pub type Preimage = pallet_preimage;

    #[runtime::pallet_index(24)]
    pub type Identity = pallet_identity;

    #[runtime::pallet_index(25)]
    pub type Proxy = pallet_proxy;

    #[runtime::pallet_index(26)]
    pub type Multisig = pallet_multisig;

    #[runtime::pallet_index(27)]
    pub type ElectionProviderMultiPhase = pallet_election_provider_multi_phase;

    #[runtime::pallet_index(28)]
    pub type Offences = pallet_offences;

    #[runtime::pallet_index(29)]
    pub type Bounties = pallet_bounties;

    #[runtime::pallet_index(30)]
    pub type ChildBounties = pallet_child_bounties;

    #[runtime::pallet_index(31)]
    pub type NominationPools = pallet_nomination_pools;

    // Gear

    #[runtime::pallet_index(100)]
    pub type GearProgram = pallet_gear_program;

    #[runtime::pallet_index(101)]
    pub type GearMessenger = pallet_gear_messenger;

    #[runtime::pallet_index(102)]
    pub type GearScheduler = pallet_gear_scheduler;

    #[runtime::pallet_index(103)]
    pub type GearGas = pallet_gear_gas;

    #[runtime::pallet_index(104)]
    pub type Gear = pallet_gear;

    #[runtime::pallet_index(105)]
    pub type GearPayment = pallet_gear_payment;

    #[runtime::pallet_index(106)]
    pub type StakingRewards = pallet_gear_staking_rewards;

    #[runtime::pallet_index(107)]
    pub type GearVoucher = pallet_gear_voucher;

    #[runtime::pallet_index(108)]
    pub type GearBank = pallet_gear_bank;

    #[runtime::pallet_index(109)]
    pub type GearBuiltin = pallet_gear_builtin;

    // Uncomment me, once ready for prod runtime.
    // #[runtime::pallet_index(110)]
    // pub type GearEthBridge = pallet_gear_eth_bridge;

    // NOTE (!): `pallet_sudo` used to be idx(99).
    // NOTE (!): `pallet_airdrop` used to be idx(198).
}

/// The address format for describing accounts.
pub type Address = sp_runtime::MultiAddress<AccountId, ()>;
/// Block header type as expected by this runtime.
pub type Header = generic::Header<BlockNumber, BlakeTwo256>;
/// Block type as expected by this runtime.
pub type Block = generic::Block<Header, UncheckedExtrinsic>;
/// The SignedExtension to the basic transaction logic.
pub type SignedExtra = (
    // Keep as long as it's needed
    StakingBlackList<Runtime>,
    frame_system::CheckNonZeroSender<Runtime>,
    frame_system::CheckSpecVersion<Runtime>,
    frame_system::CheckTxVersion<Runtime>,
    frame_system::CheckGenesis<Runtime>,
    frame_system::CheckEra<Runtime>,
    CustomCheckNonce<Runtime>,
    frame_system::CheckWeight<Runtime>,
    CustomChargeTransactionPayment<Runtime>,
    frame_metadata_hash_extension::CheckMetadataHash<Runtime>,
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

#[cfg(all(feature = "runtime-benchmarks", feature = "dev"))]
mod benches {
    define_benchmarks!(
        // Substrate pallets
        [frame_system, SystemBench::<Runtime>]
        [pallet_balances, Balances]
        [pallet_timestamp, Timestamp]
        [pallet_utility, Utility]
        // Gear pallets
        [pallet_gear, Gear]
        [pallet_gear_voucher, GearVoucher]
        [pallet_gear_builtin, GearBuiltin]
        [pallet_gear_eth_bridge, GearEthBridge]
    );
}

#[cfg(all(feature = "runtime-benchmarks", not(feature = "dev")))]
mod benches {
    define_benchmarks!(
        // Substrate pallets
        [frame_system, SystemBench::<Runtime>]
        [pallet_balances, Balances]
        [pallet_timestamp, Timestamp]
        [pallet_utility, Utility]
        // Gear pallets
        [pallet_gear, Gear]
        [pallet_gear_voucher, GearVoucher]
        [pallet_gear_builtin, GearBuiltin]
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
            let epoch_config = Babe::epoch_config().unwrap_or(BABE_GENESIS_EPOCH_CONFIG);
            sp_consensus_babe::BabeConfiguration {
                slot_duration: Babe::slot_duration(),
                epoch_length: EpochDuration::get(),
                c: epoch_config.c,
                authorities: Babe::authorities().to_vec(),
                randomness: Babe::randomness(),
                allowed_slots: epoch_config.allowed_slots,
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

    impl sp_authority_discovery::AuthorityDiscoveryApi<Block> for Runtime {
        fn authorities() -> Vec<AuthorityDiscoveryId> {
            AuthorityDiscovery::authorities()
        }
    }

    impl pallet_transaction_payment_rpc_runtime_api::TransactionPaymentCallApi<Block, Balance, RuntimeCall>
        for Runtime
    {
        fn query_call_info(call: RuntimeCall, len: u32) -> RuntimeDispatchInfo<Balance> {
            TransactionPayment::query_call_info(call, len)
        }
        fn query_call_fee_details(call: RuntimeCall, len: u32) -> FeeDetails<Balance> {
            TransactionPayment::query_call_fee_details(call, len)
        }
        fn query_weight_to_fee(weight: Weight) -> Balance {
            TransactionPayment::weight_to_fee(weight)
        }
        fn query_length_to_fee(length: u32) -> Balance {
            TransactionPayment::length_to_fee(length)
        }
    }

    impl pallet_nomination_pools_runtime_api::NominationPoolsApi<Block, AccountId, Balance> for Runtime {
        fn pending_rewards(who: AccountId) -> Balance {
            NominationPools::api_pending_rewards(who).unwrap_or_default()
        }

        fn points_to_balance(pool_id: pallet_nomination_pools::PoolId, points: Balance) -> Balance {
            NominationPools::api_points_to_balance(pool_id, points)
        }

        fn balance_to_points(pool_id: pallet_nomination_pools::PoolId, new_funds: Balance) -> Balance {
            NominationPools::api_balance_to_points(pool_id, new_funds)
        }

        fn pool_pending_slash(pool_id: pallet_nomination_pools::PoolId) -> Balance {
            NominationPools::api_pool_pending_slash(pool_id)
        }

        fn member_pending_slash(member: AccountId) -> Balance {
            NominationPools::api_member_pending_slash(member)
        }

        fn pool_needs_delegate_migration(pool_id: pallet_nomination_pools::PoolId) -> bool {
            NominationPools::api_pool_needs_delegate_migration(pool_id)
        }

        fn member_needs_delegate_migration(member: AccountId) -> bool {
            NominationPools::api_member_needs_delegate_migration(member)
        }

        fn member_total_balance(member: AccountId) -> Balance {
            NominationPools::api_member_total_balance(member)
        }

        fn pool_balance(pool_id: pallet_nomination_pools::PoolId) -> Balance {
            NominationPools::api_pool_balance(pool_id)
        }
    }

    impl pallet_staking_runtime_api::StakingApi<Block, Balance, AccountId> for Runtime {
        fn nominations_quota(balance: Balance) -> u32 {
            Staking::api_nominations_quota(balance)
        }

        fn eras_stakers_page_count(era: sp_staking::EraIndex, account: AccountId) -> sp_staking::Page {
            Staking::api_eras_stakers_page_count(era, account)
        }

        fn pending_rewards(era: sp_staking::EraIndex, account: AccountId) -> bool {
            Staking::api_pending_rewards(era, account)
        }
    }

    impl pallet_gear_staking_rewards_rpc_runtime_api::GearStakingRewardsApi<Block> for Runtime {
        fn inflation_info() -> pallet_gear_staking_rewards::InflationInfo {
            StakingRewards::inflation_info()
        }
    }

    impl pallet_gear_builtin_rpc_runtime_api::GearBuiltinApi<Block> for Runtime {
        fn query_actor_id(builtin_id: u64) -> H256 {
            GearBuiltin::generate_actor_id(builtin_id).into_bytes().into()
        }
    }

    impl pallet_gear_eth_bridge_rpc_runtime_api::GearEthBridgeApi<Block> for Runtime {
        fn merkle_proof(hash: H256) -> Option<pallet_gear_eth_bridge_rpc_runtime_api::Proof> {
            match () {
                #[cfg(not(feature = "dev"))]
                () => {
                    let _ = hash;
                    None
                },
                #[cfg(feature = "dev")]
                () => GearEthBridge::merkle_proof(hash),
            }
        }
    }

    impl sp_genesis_builder::GenesisBuilder<Block> for Runtime {
        fn build_state(config: Vec<u8>) -> sp_genesis_builder::Result {
            build_state::<RuntimeGenesisConfig>(config)
        }

        fn get_preset(id: &Option<sp_genesis_builder::PresetId>) -> Option<Vec<u8>> {
            get_preset::<RuntimeGenesisConfig>(id, genesis_config_presets::get_preset)
        }

        fn preset_names() -> Vec<sp_genesis_builder::PresetId> {
            genesis_config_presets::preset_names()
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

#[cfg(any(feature = "std", test))]
impl<B, C> Clone for RuntimeApiImpl<B, C>
where
    B: BlockT,
    C: CallApiAt<B>,
{
    fn clone(&self) -> Self {
        Self {
            call: <&C>::clone(&self.call),
            transaction_depth: self.transaction_depth.clone(),
            changes: self.changes.clone(),
            recorder: self.recorder.clone(),
            call_context: self.call_context,
            extensions: Default::default(),
            extensions_generated_for: self.extensions_generated_for.clone(),
        }
    }
}

/// Implementation of the `common::Deconstructable` trait to enable deconstruction into
/// and restoration from components for the `RuntimeApiImpl` struct.
///
/// substrate/primitives/api/proc-macro/src/impl_runtime_apis.rs:219
#[cfg(any(feature = "std", test))]
impl<B, C> common::Deconstructable<C> for RuntimeApiImpl<B, C>
where
    B: BlockT,
    C: CallApiAt<B>,
{
    type Params = (
        u16,
        OverlayedChanges<HashingFor<B>>,
        Option<ProofRecorder<B>>,
        CallContext,
        Extensions,
        Option<B::Hash>,
    );

    fn into_parts(self) -> (&'static C, Self::Params) {
        (
            self.call,
            (
                *core::cell::RefCell::borrow(&self.transaction_depth),
                self.changes.into_inner(),
                self.recorder,
                self.call_context,
                self.extensions.into_inner(),
                self.extensions_generated_for.into_inner(),
            ),
        )
    }

    fn from_parts(call: &C, params: Self::Params) -> Self {
        Self {
            call: unsafe { std::mem::transmute::<&C, &C>(call) },
            transaction_depth: params.0.into(),
            changes: core::cell::RefCell::new(params.1),
            recorder: params.2,
            call_context: params.3,
            extensions: core::cell::RefCell::new(params.4),
            extensions_generated_for: core::cell::RefCell::new(params.5),
        }
    }
}

/// Nonce check and increment to give replay protection for transactions.
///
/// # Transaction Validity
///
/// This extension affects `requires` and `provides` tags of validity, but DOES NOT
/// set the `priority` field. Make sure that AT LEAST one of the signed extension sets
/// some kind of priority upon validating transactions.
///
/// NOTE: Copy-paste from substrate/frame/system/src/extensions/check_nonce.rs,
/// but without providers and sufficients checks, so contains revert of changes
/// from substrate v1.3.0 https://github.com/paritytech/polkadot-sdk/pull/1578.
#[derive(Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct CustomCheckNonce<T: frame_system::Config>(#[codec(compact)] pub T::Nonce);

impl<T: frame_system::Config> CustomCheckNonce<T> {
    /// utility constructor. Used only in client/factory code.
    pub fn from(nonce: T::Nonce) -> Self {
        Self(nonce)
    }
}

impl<T: frame_system::Config> sp_std::fmt::Debug for CustomCheckNonce<T> {
    #[cfg(feature = "std")]
    fn fmt(&self, f: &mut sp_std::fmt::Formatter) -> sp_std::fmt::Result {
        write!(f, "CustomCheckNonce({})", self.0)
    }

    #[cfg(not(feature = "std"))]
    fn fmt(&self, _: &mut sp_std::fmt::Formatter) -> sp_std::fmt::Result {
        Ok(())
    }
}

impl<T: frame_system::Config> SignedExtension for CustomCheckNonce<T>
where
    T::RuntimeCall: Dispatchable<Info = DispatchInfo>,
{
    type AccountId = <frame_system::CheckNonce<T> as SignedExtension>::AccountId;
    type Call = <frame_system::CheckNonce<T> as SignedExtension>::Call;
    type AdditionalSigned = <frame_system::CheckNonce<T> as SignedExtension>::AdditionalSigned;
    type Pre = <frame_system::CheckNonce<T> as SignedExtension>::Pre;
    const IDENTIFIER: &'static str = <frame_system::CheckNonce<T> as SignedExtension>::IDENTIFIER;

    fn additional_signed(&self) -> sp_std::result::Result<(), TransactionValidityError> {
        Ok(())
    }

    fn pre_dispatch(
        self,
        who: &Self::AccountId,
        _call: &Self::Call,
        _info: &DispatchInfoOf<Self::Call>,
        _len: usize,
    ) -> Result<(), TransactionValidityError> {
        let mut account = frame_system::Account::<T>::get(who);

        if self.0 != account.nonce {
            return Err(if self.0 < account.nonce {
                InvalidTransaction::Stale
            } else {
                InvalidTransaction::Future
            }
            .into());
        }
        account.nonce += T::Nonce::one();
        frame_system::Account::<T>::insert(who, account);
        Ok(())
    }

    fn validate(
        &self,
        who: &Self::AccountId,
        _call: &Self::Call,
        _info: &DispatchInfoOf<Self::Call>,
        _len: usize,
    ) -> TransactionValidity {
        let account = frame_system::Account::<T>::get(who);

        if self.0 < account.nonce {
            return InvalidTransaction::Stale.into();
        }

        let provides = vec![Encode::encode(&(who, self.0))];
        let requires = if account.nonce < self.0 {
            vec![Encode::encode(&(who, self.0 - One::one()))]
        } else {
            vec![]
        };

        Ok(ValidTransaction {
            priority: 0,
            requires,
            provides,
            longevity: TransactionLongevity::max_value(),
            propagate: true,
        })
    }
}
