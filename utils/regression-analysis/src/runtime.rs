use crate::weights::BenchmarkWeight;
use frame_support::{construct_runtime, parameter_types};
use gear_runtime::{Block, UncheckedExtrinsic};
use std::convert::{TryFrom, TryInto};

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
    pub Schedule: pallet_gear::Schedule<BenchmarkConfig> = Default::default();
}

impl frame_system::Config for BenchmarkConfig {
    type BaseCallFilter = <gear_runtime::Runtime as frame_system::Config>::BaseCallFilter;
    type BlockWeights = <gear_runtime::Runtime as frame_system::Config>::BlockWeights;
    type BlockLength = <gear_runtime::Runtime as frame_system::Config>::BlockLength;
    type Origin = Origin;
    type Call = Call;
    type Index = <gear_runtime::Runtime as frame_system::Config>::Index;
    type BlockNumber = <gear_runtime::Runtime as frame_system::Config>::BlockNumber;
    type Hash = <gear_runtime::Runtime as frame_system::Config>::Hash;
    type Hashing = <gear_runtime::Runtime as frame_system::Config>::Hashing;
    type AccountId = <gear_runtime::Runtime as frame_system::Config>::AccountId;
    type Lookup = <gear_runtime::Runtime as frame_system::Config>::Lookup;
    type Header = <gear_runtime::Runtime as frame_system::Config>::Header;
    type Event = Event;
    type BlockHashCount = <gear_runtime::Runtime as frame_system::Config>::BlockHashCount;
    type DbWeight = <gear_runtime::Runtime as frame_system::Config>::DbWeight;
    type Version = <gear_runtime::Runtime as frame_system::Config>::Version;
    type PalletInfo = PalletInfo;
    type AccountData = <gear_runtime::Runtime as frame_system::Config>::AccountData;
    type OnNewAccount = <gear_runtime::Runtime as frame_system::Config>::OnNewAccount;
    type OnKilledAccount = <gear_runtime::Runtime as frame_system::Config>::OnKilledAccount;
    type SystemWeightInfo = <gear_runtime::Runtime as frame_system::Config>::SystemWeightInfo;
    type SS58Prefix = <gear_runtime::Runtime as frame_system::Config>::SS58Prefix;
    type OnSetCode = <gear_runtime::Runtime as frame_system::Config>::OnSetCode;
    type MaxConsumers = <gear_runtime::Runtime as frame_system::Config>::MaxConsumers;
}

impl pallet_balances::Config for BenchmarkConfig {
    type Balance = <gear_runtime::Runtime as pallet_balances::Config>::Balance;
    type DustRemoval = <gear_runtime::Runtime as pallet_balances::Config>::DustRemoval;
    type Event = Event;
    type ExistentialDeposit =
        <gear_runtime::Runtime as pallet_balances::Config>::ExistentialDeposit;
    type AccountStore = <gear_runtime::Runtime as pallet_balances::Config>::AccountStore;
    type WeightInfo = pallet_balances::weights::SubstrateWeight<Self>;
    type MaxLocks = <gear_runtime::Runtime as pallet_balances::Config>::MaxLocks;
    type MaxReserves = <gear_runtime::Runtime as pallet_balances::Config>::MaxReserves;
    type ReserveIdentifier = <gear_runtime::Runtime as pallet_balances::Config>::ReserveIdentifier;
}

impl pallet_authorship::Config for BenchmarkConfig {
    type FindAuthor = <gear_runtime::Runtime as pallet_authorship::Config>::FindAuthor;
    type UncleGenerations = <gear_runtime::Runtime as pallet_authorship::Config>::UncleGenerations;
    type FilterUncle = <gear_runtime::Runtime as pallet_authorship::Config>::FilterUncle;
    type EventHandler = <gear_runtime::Runtime as pallet_authorship::Config>::EventHandler;
}

impl pallet_timestamp::Config for BenchmarkConfig {
    type Moment = <gear_runtime::Runtime as pallet_timestamp::Config>::Moment;
    type OnTimestampSet = <gear_runtime::Runtime as pallet_timestamp::Config>::OnTimestampSet;
    type MinimumPeriod = <gear_runtime::Runtime as pallet_timestamp::Config>::MinimumPeriod;
    type WeightInfo = <gear_runtime::Runtime as pallet_timestamp::Config>::WeightInfo;
}

impl pallet_gear_program::Config for BenchmarkConfig {
    type Event = Event;
    type WeightInfo = <gear_runtime::Runtime as pallet_gear_program::Config>::WeightInfo;
    type Currency = <gear_runtime::Runtime as pallet_gear_program::Config>::Currency;
    type Messenger = <gear_runtime::Runtime as pallet_gear_program::Config>::Messenger;
}

impl pallet_gear_messenger::Config for BenchmarkConfig {
    type BlockLimiter = <gear_runtime::Runtime as pallet_gear_messenger::Config>::BlockLimiter;
}

impl pallet_gear_gas::Config for BenchmarkConfig {
    type BlockGasLimit = <gear_runtime::Runtime as pallet_gear_gas::Config>::BlockGasLimit;
}

impl pallet_gear_scheduler::Config for BenchmarkConfig {
    type BlockLimiter = <gear_runtime::Runtime as pallet_gear_scheduler::Config>::BlockLimiter;
    type ReserveThreshold =
        <gear_runtime::Runtime as pallet_gear_scheduler::Config>::ReserveThreshold;
    type WaitlistCost = <gear_runtime::Runtime as pallet_gear_scheduler::Config>::WaitlistCost;
    type MailboxCost = <gear_runtime::Runtime as pallet_gear_scheduler::Config>::MailboxCost;
}

impl pallet_gear::Config for BenchmarkConfig {
    type Event = Event;
    type Currency = <gear_runtime::Runtime as pallet_gear::Config>::Currency;
    type GasPrice = <gear_runtime::Runtime as pallet_gear::Config>::GasPrice;
    type WeightInfo = BenchmarkWeight<BenchmarkConfig>;
    type Schedule = Schedule;
    type OutgoingLimit = <gear_runtime::Runtime as pallet_gear::Config>::OutgoingLimit;
    type DebugInfo = <gear_runtime::Runtime as pallet_gear::Config>::DebugInfo;
    type CodeStorage = <gear_runtime::Runtime as pallet_gear::Config>::CodeStorage;
    type MailboxThreshold = <gear_runtime::Runtime as pallet_gear::Config>::MailboxThreshold;
    type Messenger = <gear_runtime::Runtime as pallet_gear::Config>::Messenger;
    type GasProvider = <gear_runtime::Runtime as pallet_gear::Config>::GasProvider;
    type BlockLimiter = <gear_runtime::Runtime as pallet_gear::Config>::BlockLimiter;
    type Scheduler = <gear_runtime::Runtime as pallet_gear::Config>::Scheduler;
}
