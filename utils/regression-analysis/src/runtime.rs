use crate::weights::BenchmarkWeight;
use frame_support::{
    construct_runtime, parameter_types,
    sp_runtime::traits::{BlakeTwo256, IdentityLookup},
    weights::constants::RocksDbWeight,
};
use sp_runtime::{testing::Header, traits::ConstU64};
use std::convert::{TryFrom, TryInto};

pub struct GasConverter;

impl common::GasPrice for GasConverter {
    type Balance = u128;
}

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<BenchmarkConfig>;
type Block = frame_system::mocking::MockBlock<BenchmarkConfig>;

construct_runtime!(
    pub enum BenchmarkConfig
    where
        Block = Block,
        NodeBlock = Block,
        UncheckedExtrinsic = UncheckedExtrinsic,
        {
            System: frame_system,
            Balances: pallet_balances,
            GearProgram: pallet_gear_program,
            GearMessenger: pallet_gear_messenger,
            GearGas: pallet_gear_gas,
            GearScheduler: pallet_gear_scheduler,
            Gear: pallet_gear,
        }
);

parameter_types! {
    pub const ExistentialDeposit: u64 = 0;
    pub const BlockGasLimit: u64 = 0;
}

// TODO: type Something = <gear_runtime::Config>::Something;

impl frame_system::Config for BenchmarkConfig {
    type BaseCallFilter = frame_support::traits::Everything;
    type BlockWeights = ();
    type BlockLength = ();
    type Origin = Origin;
    type Call = Call;
    type Index = u128;
    type BlockNumber = u64;
    type Hash = sp_core::H256;
    type Hashing = BlakeTwo256;
    type AccountId = u64;
    type Lookup = IdentityLookup<Self::AccountId>;
    type Header = Header;
    type Event = Event;
    type BlockHashCount = ();
    type DbWeight = RocksDbWeight;
    type Version = ();
    type PalletInfo = PalletInfo;
    type AccountData = pallet_balances::AccountData<u128>;
    type OnNewAccount = ();
    type OnKilledAccount = ();
    type SystemWeightInfo = ();
    type SS58Prefix = ();
    type OnSetCode = ();
    type MaxConsumers = frame_support::traits::ConstU32<16>;
}

impl pallet_balances::Config for BenchmarkConfig {
    type Balance = u128;
    type DustRemoval = ();
    type Event = Event;
    type ExistentialDeposit = ExistentialDeposit;
    type AccountStore = System;
    type WeightInfo = ();
    type MaxLocks = ();
    type MaxReserves = ();
    type ReserveIdentifier = [u8; 8];
}

impl pallet_authorship::Config for BenchmarkConfig {
    type FindAuthor = ();
    type UncleGenerations = ();
    type FilterUncle = ();
    type EventHandler = ();
}

impl pallet_timestamp::Config for BenchmarkConfig {
    type Moment = u64;
    type OnTimestampSet = ();
    type MinimumPeriod = ();
    type WeightInfo = ();
}

impl pallet_gear_program::Config for BenchmarkConfig {
    type Event = Event;
    type WeightInfo = ();
    type Currency = Balances;
    type Messenger = GearMessenger;
}

impl pallet_gear_messenger::Config for BenchmarkConfig {
    type Currency = Balances;
    type BlockLimiter = GearGas;
}

impl pallet_gear_gas::Config for BenchmarkConfig {
    type BlockGasLimit = BlockGasLimit;
}

impl pallet_gear_scheduler::Config for BenchmarkConfig {
    type BlockLimiter = GearGas;
    type ReserveThreshold = ConstU64<1>;
    type WaitlistCost = ConstU64<100>;
}

impl pallet_gear::Config for BenchmarkConfig {
    type Event = Event;
    type Currency = Balances;
    type GasPrice = GasConverter;
    type WeightInfo = BenchmarkWeight<BenchmarkConfig>;
    type Schedule = ();
    type OutgoingLimit = ();
    type DebugInfo = ();
    type CodeStorage = GearProgram;
    type MailboxThreshold = ();
    type Messenger = GearMessenger;
    type GasProvider = GearGas;
    type BlockLimiter = GearGas;
    type Scheduler = GearScheduler;
}
