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
#![allow(clippy::items_after_test_module)]

// Make the WASM binary available.
#[cfg(feature = "std")]
include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));

pub use frame_support::{
    codec::{Decode, Encode, MaxEncodedLen},
    construct_runtime,
    dispatch::{DispatchClass, DispatchError, WeighData},
    parameter_types,
    traits::{
        ConstU128, ConstU16, ConstU32, Contains, Currency, EitherOf, EitherOfDiverse,
        EqualPrivilegeOnly, Everything, FindAuthor, InstanceFilter, KeyOwnerProofSystem,
        LockIdentifier, NeverEnsureOrigin, Nothing, OnUnbalanced, Randomness, StorageInfo,
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
use frame_system::limits::{BlockLength, BlockWeights};
pub use pallet_gear::manager::{ExtManager, HandleKind};
pub use pallet_timestamp::Call as TimestampCall;
pub use runtime_primitives::{AccountId, Signature, VARA_SS58_PREFIX};
use runtime_primitives::{Balance, BlockNumber, Hash, Moment, Nonce};
use sp_api::impl_runtime_apis;
#[cfg(any(feature = "std", test))]
use sp_api::{
    CallApiAt, CallContext, Extensions, OverlayedChanges, ProofRecorder, StateBackend,
    StorageTransactionCache,
};
use sp_core::{ConstBool, ConstU64, OpaqueMetadata, H256};
#[cfg(any(feature = "std", test))]
use sp_runtime::traits::HashFor;
use sp_runtime::{
    create_runtime_str, generic,
    traits::{AccountIdLookup, BlakeTwo256, Block as BlockT},
    transaction_validity::{TransactionSource, TransactionValidity},
    ApplyExtrinsicResult, Perbill, Percent,
};
use sp_std::{
    convert::{TryFrom, TryInto},
    prelude::*,
};
#[cfg(feature = "std")]
use sp_version::NativeVersion;
use sp_version::RuntimeVersion;

/// Currency related constants
pub mod currency {
    use runtime_primitives::Balance;

    pub const UNITS: Balance = 1_000_000_000_000; // 10^(-12) precision

    /// Base economic unit, 10 Vara.
    pub const ECONOMIC_UNITS: Balance = UNITS * 10;
    pub const ECONOMIC_CENTIUNITS: Balance = ECONOMIC_UNITS / 100;

    /// The existential deposit.
    pub const EXISTENTIAL_DEPOSIT: Balance = 10 * UNITS; // 10 Vara

    /// The program rent cost per block.
    pub const RENT_COST_PER_BLOCK: Balance = 125_000_000;

    /// Helper function to calculate various deposits for using pallets' storage
    pub const fn deposit(items: u32, bytes: u32) -> Balance {
        // TODO: review numbers (#2650)
        items as Balance * 15 * ECONOMIC_CENTIUNITS + (bytes as Balance) * 6 * ECONOMIC_CENTIUNITS
    }
}

/// Time and block constants
pub mod time {
    use runtime_primitives::{BlockNumber, Moment};

    pub const MILLISECS_PER_BLOCK: Moment = 3000;

    // Milliseconds per year for the Julian year (365.25 days).
    pub const MILLISECONDS_PER_YEAR: u64 = 1000 * 3600 * 24 * 36525 / 100;

    // NOTE: Currently it is not possible to change the slot duration after the chain has started.
    //       Attempting to do so will brick block production.
    pub const SLOT_DURATION: Moment = MILLISECS_PER_BLOCK;

    // Time is measured by number of blocks.
    pub const MINUTES: BlockNumber = 60_000 / (MILLISECS_PER_BLOCK as BlockNumber);
    pub const HOURS: BlockNumber = MINUTES * 60;
    pub const DAYS: BlockNumber = HOURS * 24;
    pub const WEEKS: BlockNumber = DAYS * 7;
    pub const MONTHS: BlockNumber = WEEKS * 4;

    // NOTE: Currently it is not possible to change the epoch duration after the chain has started.
    //       Attempting to do so will brick block production.
    pub const EPOCH_DURATION_IN_BLOCKS: BlockNumber = 2 * HOURS;
    pub const EPOCH_DURATION_IN_SLOTS: u64 = {
        const SLOT_FILL_RATE: f64 = MILLISECS_PER_BLOCK as f64 / SLOT_DURATION as f64;

        (EPOCH_DURATION_IN_BLOCKS as f64 * SLOT_FILL_RATE) as u64
    };

    // 1 in 4 blocks (on average, not counting collisions) will be primary BABE blocks.
    pub const PRIMARY_PROBABILITY: (u64, u64) = (1, 4);
}

pub use currency::*;
pub use time::*;

pub const RENT_RESUME_WEEK_FACTOR: BlockNumber = 4;

pub const BANK_ADDRESS: [u8; 32] = *b"gearbankgearbankgearbankgearbank";
pub const RESUME_SESSION_DURATION_HOUR_FACTOR: BlockNumber = 1;
pub const RENT_FREE_PERIOD_MONTH_FACTOR: BlockNumber = 6;
pub const RENT_DISABLED_DELTA_WEEK_FACTOR: BlockNumber = 1;
pub const BLOCK_AUTHOR: [u8; 32] = *b"default//block//author//address/";
pub const RANDOM_USER: [u8; 32] = *b"random//user//address//random//u";

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
    spec_version: 1050,
    impl_version: 1,
    apis: RUNTIME_API_VERSIONS,
    transaction_version: 1,
    state_version: 1,
};

/// We assume that ~3% of the block weight is consumed by `on_initialize` handlers.
/// This is used to limit the maximal weight of a single extrinsic.
///
/// Mostly we don't produce any calculations in `on_initialize` hook,
/// so it's safe to reduce from default 10 to custom 3 percents.
pub const AVERAGE_ON_INITIALIZE_RATIO: Perbill = Perbill::from_percent(3);
pub const NORMAL_DISPATCH_RATIO_NUM: u8 = 25;
pub const GAS_LIMIT_MIN_PERCENTAGE_NUM: u8 = 100 - NORMAL_DISPATCH_RATIO_NUM;

// Extrinsics with DispatchClass::Normal only account for user messages
// TODO: consider making the normal extrinsics share adjustable in runtime
pub const NORMAL_DISPATCH_RATIO: Perbill = Perbill::from_percent(NORMAL_DISPATCH_RATIO_NUM as u32);

/// Returns common for gear protocol `BlockWeights` depend on given max block weight.
pub fn block_weights_for(maximum_block_weight: Weight) -> BlockWeights {
    BlockWeights::builder()
        .base_block(BlockExecutionWeight::get())
        .for_class(DispatchClass::all(), |weights| {
            weights.base_extrinsic = ExtrinsicBaseWeight::get();
        })
        .for_class(DispatchClass::Normal, |weights| {
            weights.max_total = Some(NORMAL_DISPATCH_RATIO * maximum_block_weight);
        })
        .for_class(DispatchClass::Operational, |weights| {
            weights.max_total = Some(maximum_block_weight);
            // Operational transactions have some extra reserved space, so that they
            // are included even if block reached `MAXIMUM_BLOCK_WEIGHT`.
            weights.reserved =
                Some(maximum_block_weight - NORMAL_DISPATCH_RATIO * maximum_block_weight);
        })
        .avg_block_initialization(AVERAGE_ON_INITIALIZE_RATIO)
        .build_or_panic()
}

parameter_types! {
    pub const BlockHashCount: BlockNumber = 2400;
}

pub const VALUE_PER_GAS: u128 = 25;

pub type NegativeImbalance<T> = <pallet_balances::Pallet<T> as Currency<
    <T as frame_system::Config>::AccountId,
>>::NegativeImbalance;

pub struct DealWithFees<R>(sp_std::marker::PhantomData<R>);
impl<R> OnUnbalanced<NegativeImbalance<R>> for DealWithFees<R>
where
    R: pallet_balances::Config + pallet_authorship::Config,
    <R as frame_system::Config>::AccountId: From<AccountId>,
    <R as frame_system::Config>::AccountId: Into<AccountId>,
{
    fn on_unbalanceds<B>(mut fees_then_tips: impl Iterator<Item = NegativeImbalance<R>>) {
        if let Some(fees) = fees_then_tips.next() {
            if let Some(author) = <pallet_authorship::Pallet<R>>::author() {
                <pallet_balances::Pallet<R>>::resolve_creating(&author, fees);
            }
            if let Some(tips) = fees_then_tips.next() {
                if let Some(author) = <pallet_authorship::Pallet<R>>::author() {
                    <pallet_balances::Pallet<R>>::resolve_creating(&author, tips);
                }
            }
        }
    }
}

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
    pub const SS58Prefix: u8 = VARA_SS58_PREFIX;
    pub RuntimeBlockWeights: BlockWeights = block_weights_for(MAXIMUM_BLOCK_WEIGHT);
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
    type SystemWeightInfo = ();
    /// This is used as an identifier of the chain. 42 is the generic substrate prefix.
    type SS58Prefix = SS58Prefix;
    /// The set code logic, just the default since we're not a parachain.
    type OnSetCode = ();
    type MaxConsumers = ConstU32<16>;
}

pub struct FixedBlockAuthor;
impl FindAuthor<AccountId> for FixedBlockAuthor {
    fn find_author<'a, I: 'a>(_: I) -> Option<AccountId> {
        Some(BLOCK_AUTHOR.into())
    }
}
impl pallet_authorship::Config for Runtime {
    type FindAuthor = FixedBlockAuthor;
    type EventHandler = ();
}

parameter_types! {
    pub const MinimumPeriod: Moment = SLOT_DURATION / 2;
}

impl pallet_timestamp::Config for Runtime {
    /// A timestamp: milliseconds since the unix epoch.
    type Moment = Moment;
    type OnTimestampSet = ();
    type MinimumPeriod = MinimumPeriod;
    type WeightInfo = ();
}

parameter_types! {
    // For weight estimation, we assume that the most locks on an individual account will be 50.
    // This number may need to be adjusted in the future if this assumption no longer holds true.
    pub const MaxLocks: u32 = 50;
    pub const MaxReserves: u32 = 50;
}

impl pallet_balances::Config for Runtime {
    type MaxLocks = MaxLocks;
    type MaxReserves = MaxReserves;
    type ReserveIdentifier = [u8; 8];
    type Balance = Balance;
    type RuntimeEvent = RuntimeEvent;
    type DustRemoval = ();
    type ExistentialDeposit = ConstU128<EXISTENTIAL_DEPOSIT>;
    type AccountStore = System;
    type WeightInfo = ();
    type FreezeIdentifier = ();
    type MaxFreezes = ();
    type RuntimeHoldReason = RuntimeHoldReason;
    type MaxHolds = ConstU32<2>;
}

impl pallet_sudo::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type RuntimeCall = RuntimeCall;
    type WeightInfo = ();
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

    pub const PerformanceMultiplier: u32 = 100;
}

impl pallet_gear_program::Config for Runtime {
    type Scheduler = GearScheduler;
    type CurrentBlockNumber = Gear;
}

parameter_types! {
    pub Schedule: pallet_gear::Schedule<Runtime> = Default::default();
    pub BankAddress: AccountId = BANK_ADDRESS.into();
    pub const GasMultiplier: common::GasMultiplier<Balance, u64> = common::GasMultiplier::ValuePerGas(VALUE_PER_GAS);
}

impl pallet_gear_bank::Config for Runtime {
    type Currency = Balances;
    type BankAddress = BankAddress;
    type GasMultiplier = GasMultiplier;
}

pub struct FakeRandomness;
impl<Output: Default> Randomness<Output, BlockNumber> for FakeRandomness {
    fn random(_subject: &[u8]) -> (Output, BlockNumber) {
        Default::default()
    }
}

impl pallet_gear::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Randomness = FakeRandomness;
    type WeightInfo = ();
    type Schedule = Schedule;
    type OutgoingLimit = OutgoingLimit;
    type PerformanceMultiplier = PerformanceMultiplier;
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
    type ProgramRentFreePeriod = ConstU32<{ MONTHS * RENT_FREE_PERIOD_MONTH_FACTOR }>;
    type ProgramResumeMinimalRentPeriod = ConstU32<{ WEEKS * RENT_RESUME_WEEK_FACTOR }>;
    type ProgramRentCostPerBlock = ConstU128<RENT_COST_PER_BLOCK>;
    type ProgramResumeSessionDuration = ConstU32<{ HOURS * RESUME_SESSION_DURATION_HOUR_FACTOR }>;

    type ProgramRentEnabled = ConstBool<false>;

    type ProgramRentDisabledDelta = ConstU32<{ WEEKS * RENT_DISABLED_DELTA_WEEK_FACTOR }>;
}

impl pallet_gear_debug::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type WeightInfo = ();
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

construct_runtime!(
    pub struct Runtime
    {
        System: frame_system = 0,
        Timestamp: pallet_timestamp = 1,
        Authorship: pallet_authorship = 2,
        Balances: pallet_balances = 5,

        GearProgram: pallet_gear_program = 100,
        GearMessenger: pallet_gear_messenger = 101,
        GearScheduler: pallet_gear_scheduler = 102,
        GearGas: pallet_gear_gas = 103,
        Gear: pallet_gear = 104,
        GearBank: pallet_gear_bank = 108,

        Sudo: pallet_sudo = 99,

        GearDebug: pallet_gear_debug = 199,
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
    frame_system::CheckNonZeroSender<Runtime>,
    frame_system::CheckSpecVersion<Runtime>,
    frame_system::CheckTxVersion<Runtime>,
    frame_system::CheckGenesis<Runtime>,
    frame_system::CheckEra<Runtime>,
    frame_system::CheckNonce<Runtime>,
    frame_system::CheckWeight<Runtime>,
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
    (),
>;

type DebugInfo = GearDebug;

impl_runtime_apis! {
    impl sp_api::Core<Block> for Runtime {
        fn version() -> RuntimeVersion {
            VERSION
        }

        fn execute_block(block: Block) {
            Executive::execute_block(block);
        }

        fn initialize_block(header: &<Block as BlockT>::Header) {
            Executive::initialize_block(header)
        }
    }

    impl sp_api::Metadata<Block> for Runtime {
        fn metadata() -> OpaqueMetadata {
            OpaqueMetadata::new(Runtime::metadata().into())
        }

        fn metadata_at_version(version: u32) -> Option<OpaqueMetadata> {
            Runtime::metadata_at_version(version)
        }

        fn metadata_versions() -> sp_std::vec::Vec<u32> {
            Runtime::metadata_versions()
        }
    }

    impl sp_block_builder::BlockBuilder<Block> for Runtime {
        fn apply_extrinsic(extrinsic: <Block as BlockT>::Extrinsic) -> ApplyExtrinsicResult {
            Executive::apply_extrinsic(extrinsic)
        }

        fn finalize_block() -> <Block as BlockT>::Header {
            Executive::finalize_block()
        }

        fn inherent_extrinsics(data: sp_inherents::InherentData) -> Vec<<Block as BlockT>::Extrinsic> {
            data.create_extrinsics()
        }

        fn check_inherents(
            block: Block,
            data: sp_inherents::InherentData,
        ) -> sp_inherents::CheckInherentsResult {
            data.check_extrinsics(&block)
        }
    }

    impl sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block> for Runtime {
        fn validate_transaction(
            source: TransactionSource,
            tx: <Block as BlockT>::Extrinsic,
            block_hash: <Block as BlockT>::Hash,
        ) -> TransactionValidity {
            Executive::validate_transaction(source, tx, block_hash)
        }
    }

    impl frame_system_rpc_runtime_api::AccountNonceApi<Block, AccountId, Nonce> for Runtime {
        fn account_nonce(account: AccountId) -> Nonce {
            System::account_nonce(account)
        }
    }

    // Here we implement our custom runtime API.
    impl pallet_gear_rpc_runtime_api::GearApi<Block> for Runtime {
        fn calculate_gas_info(
            account_id: H256,
            kind: HandleKind,
            payload: Vec<u8>,
            value: u128,
            allow_other_panics: bool,
            initial_gas: Option<u64>,
            gas_allowance: Option<u64>,
        ) -> Result<pallet_gear::GasInfo, Vec<u8>> {
            Gear::calculate_gas_info(account_id, kind, payload, value, allow_other_panics, initial_gas, gas_allowance)
        }

        fn gear_run_extrinsic(max_gas: Option<u64>) -> <Block as BlockT>::Extrinsic {
            UncheckedExtrinsic::new_unsigned(
                pallet_gear::Call::run { max_gas }.into()
            )
        }

        fn read_state(program_id: H256, payload: Vec<u8>, gas_allowance: Option<u64>,) -> Result<Vec<u8>, Vec<u8>> {
            Gear::read_state(program_id, payload, gas_allowance)
        }

        fn read_state_using_wasm(
            program_id: H256,
            payload: Vec<u8>,
            fn_name: Vec<u8>,
            wasm: Vec<u8>,
            argument: Option<Vec<u8>>,
            gas_allowance: Option<u64>,
        ) -> Result<Vec<u8>, Vec<u8>> {
            Gear::read_state_using_wasm(program_id, payload, fn_name, wasm, argument, gas_allowance)
        }

        fn read_metahash(program_id: H256, gas_allowance: Option<u64>,) -> Result<H256, Vec<u8>> {
            Gear::read_metahash(program_id, gas_allowance)
        }
    }
}

#[cfg(any(feature = "std", test))]
impl<B, C> Clone for RuntimeApiImpl<B, C>
where
    B: BlockT,
    C: CallApiAt<B>,
    C::StateBackend: StateBackend<HashFor<B>>,
    <C::StateBackend as StateBackend<HashFor<B>>>::Transaction: Clone,
{
    fn clone(&self) -> Self {
        Self {
            call: <&C>::clone(&self.call),
            transaction_depth: self.transaction_depth.clone(),
            changes: self.changes.clone(),
            storage_transaction_cache: self.storage_transaction_cache.clone(),
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
    C::StateBackend: StateBackend<HashFor<B>>,
    <C::StateBackend as StateBackend<HashFor<B>>>::Transaction: Clone,
{
    type Params = (
        u16,
        OverlayedChanges,
        StorageTransactionCache<B, C::StateBackend>,
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
                self.storage_transaction_cache.into_inner(),
                self.recorder,
                self.call_context,
                self.extensions.into_inner(),
                self.extensions_generated_for.into_inner(),
            ),
        )
    }

    fn from_parts(call: &C, params: Self::Params) -> Self {
        Self {
            call: unsafe { std::mem::transmute(call) },
            transaction_depth: params.0.into(),
            changes: core::cell::RefCell::new(params.1),
            storage_transaction_cache: core::cell::RefCell::new(params.2),
            recorder: params.3,
            call_context: params.4,
            extensions: core::cell::RefCell::new(params.5),
            extensions_generated_for: core::cell::RefCell::new(params.6),
        }
    }
}
