// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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

use crate as pallet_usage;
use codec::Decode;
use common::{CodeMetadata, CodeStorage, Origin as _};
use frame_support::{
    construct_runtime, parameter_types,
    traits::{ConstU64, FindAuthor, OffchainWorker, OnIdle, OnInitialize},
};
use frame_system as system;
use gear_core::{
    code::{Code, CodeAndId, InstrumentedCodeAndId},
    ids::ProgramId,
    program::Program,
};
use hex_literal::hex;
use parking_lot::RwLock;
use primitive_types::H256;
use sp_core::offchain::{
    testing::{PoolState, TestOffchainExt, TestTransactionPoolExt},
    Duration, OffchainDbExt, OffchainWorkerExt, TransactionPoolExt,
};
use sp_io::offchain;
use sp_runtime::{
    offchain::storage::StorageValueRef,
    testing::{Header, TestXt},
    traits::{BlakeTwo256, IdentityLookup},
    Perbill,
};
use sp_std::convert::{TryFrom, TryInto};
use std::sync::Arc;
use wasm_instrument::gas_metering::ConstantCostRules;

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;

pub const BLOCK_AUTHOR: u64 = 255;

// Configure a mock runtime to test the pallet.
construct_runtime!(
    pub enum Test where
        Block = Block,
        NodeBlock = Block,
        UncheckedExtrinsic = UncheckedExtrinsic,
    {
        System: system::{Pallet, Call, Config, Storage, Event<T>},
        GearProgram: pallet_gear_program::{Pallet, Storage, Event<T>},
        GearMessenger: pallet_gear_messenger::{Pallet},
        Gear: pallet_gear::{Pallet, Call, Storage, Event<T>},
        Gas: pallet_gas::{Pallet, Storage},
        Usage: pallet_usage::{Pallet, Call, Storage, Event<T>, ValidateUnsigned},
        Balances: pallet_balances::{Pallet, Call, Storage, Config<T>, Event<T>},
        Authorship: pallet_authorship::{Pallet, Storage},
        Timestamp: pallet_timestamp::{Pallet, Call, Storage, Inherent},
    }
);

impl pallet_balances::Config for Test {
    type MaxLocks = ();
    type MaxReserves = ();
    type ReserveIdentifier = [u8; 8];
    type Balance = u128;
    type DustRemoval = ();
    type Event = Event;
    type ExistentialDeposit = ExistentialDeposit;
    type AccountStore = System;
    type WeightInfo = ();
}

parameter_types! {
    pub const BlockHashCount: u64 = 250;
    pub const SS58Prefix: u8 = 42;
    pub const ExistentialDeposit: u64 = 100;
}

impl system::Config for Test {
    type BaseCallFilter = frame_support::traits::Everything;
    type BlockWeights = ();
    type BlockLength = ();
    type DbWeight = ();
    type Origin = Origin;
    type Call = Call;
    type Index = u64;
    type BlockNumber = u64;
    type Hash = H256;
    type Hashing = BlakeTwo256;
    type AccountId = u64;
    type Lookup = IdentityLookup<Self::AccountId>;
    type Header = Header;
    type Event = Event;
    type BlockHashCount = BlockHashCount;
    type Version = ();
    type PalletInfo = PalletInfo;
    type AccountData = pallet_balances::AccountData<u128>;
    type OnNewAccount = ();
    type OnKilledAccount = ();
    type SystemWeightInfo = ();
    type SS58Prefix = SS58Prefix;
    type OnSetCode = ();
    type MaxConsumers = frame_support::traits::ConstU32<16>;
}

pub struct GasConverter;
impl common::GasPrice for GasConverter {
    type Balance = u128;
}

impl pallet_gear_program::Config for Test {
    type Event = Event;
    type WeightInfo = ();
    type Currency = Balances;
    type Messenger = GearMessenger;
}

parameter_types! {
    pub const WaitListFeePerBlock: u64 = 100;
}

impl pallet_gear::Config for Test {
    type Event = Event;
    type Currency = Balances;
    type GasPrice = GasConverter;
    type WeightInfo = ();
    type OutgoingLimit = ();
    type DebugInfo = ();
    type WaitListFeePerBlock = WaitListFeePerBlock;
    type Schedule = ();
    type CodeStorage = GearProgram;
    type Messenger = GearMessenger;
    type ValueTreeProvider = Gas;
}

impl pallet_gas::Config for Test {
    type BlockGasLimit = ();
}

parameter_types! {
    pub const WaitListTraversalInterval: u32 = 5;
    pub const MaxBatchSize: u32 = 10;
    pub const ExpirationDuration: u64 = 3000;
    pub const ExternalSubmitterRewardFraction: Perbill = Perbill::from_percent(10);
}

impl pallet_gear_messenger::Config for Test {
    type Currency = Balances;
}

impl pallet_usage::Config for Test {
    type Event = Event;
    type PaymentProvider = Gear;
    type WeightInfo = ();
    type WaitListTraversalInterval = WaitListTraversalInterval;
    type ExpirationDuration = ExpirationDuration;
    type MaxBatchSize = MaxBatchSize;
    type TrapReplyExistentialGasLimit = ConstU64<1000>;
    type ExternalSubmitterRewardFraction = ExternalSubmitterRewardFraction;
    type Messenger = GearMessenger;
}

pub struct FixedBlockAuthor;

impl FindAuthor<u64> for FixedBlockAuthor {
    fn find_author<'a, I>(_digests: I) -> Option<u64>
    where
        I: 'a + IntoIterator<Item = (sp_runtime::ConsensusEngineId, &'a [u8])>,
    {
        Some(BLOCK_AUTHOR)
    }
}

impl pallet_authorship::Config for Test {
    type FindAuthor = FixedBlockAuthor;
    type UncleGenerations = ();
    type FilterUncle = ();
    type EventHandler = ();
}

pub(crate) type Extrinsic = TestXt<Call, ()>;

impl<LocalCall> frame_system::offchain::SendTransactionTypes<LocalCall> for Test
where
    Call: From<LocalCall>,
{
    type OverarchingCall = Call;
    type Extrinsic = Extrinsic;
}

impl pallet_timestamp::Config for Test {
    type Moment = u64;
    type OnTimestampSet = ();
    type MinimumPeriod = ();
    type WeightInfo = ();
}

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
    let mut t = system::GenesisConfig::default()
        .build_storage::<Test>()
        .unwrap();

    pallet_balances::GenesisConfig::<Test> {
        balances: vec![
            (1, 1_000_000_u128),
            (2, 1_000_000_u128),
            (3, 1_000_000_u128),
            (4, 1_000_000_u128),
            (5, 1_000_000_u128),
            (6, 1_000_000_u128),
            (7, 1_000_000_u128),
            (8, 1_000_000_u128),
            (9, 1_000_000_u128),
            (10, 1_000_000_u128),
            (BLOCK_AUTHOR, 101_u128),
        ],
    }
    .assimilate_storage(&mut t)
    .unwrap();

    let mut ext = sp_io::TestExternalities::new(t);
    ext.execute_with(|| System::set_block_number(1));
    ext
}

pub fn with_offchain_ext() -> (sp_io::TestExternalities, Arc<RwLock<PoolState>>) {
    let mut ext = new_test_ext();
    let (offchain, _) = TestOffchainExt::new();
    let (pool, pool_state) = TestTransactionPoolExt::new();

    ext.register_extension(OffchainDbExt::new(offchain.clone()));
    ext.register_extension(OffchainWorkerExt::new(offchain));
    ext.register_extension(TransactionPoolExt::new(pool));

    (ext, pool_state)
}

pub(crate) fn run_to_block(n: u64) {
    let now = System::block_number();
    for i in now + 1..=n {
        log::debug!("📦 Processing block {}", i);
        System::set_block_number(i);
        Usage::on_initialize(i);
        Gas::on_initialize(System::block_number());
        GearMessenger::on_initialize(System::block_number());
        Gear::on_idle(i, 1_000_000_000);
    }
}

pub(crate) fn run_to_block_with_ocw(n: u64) {
    let now = System::block_number();
    for i in now + 1..=n {
        System::set_block_number(i);
        Usage::on_initialize(i);
        increase_offchain_time(1_000);
        Usage::offchain_worker(i);
    }
}

pub(crate) fn increase_offchain_time(ms: u64) {
    offchain::sleep_until(offchain::timestamp().add(Duration::from_millis(ms)));
}

pub(crate) fn get_current_offchain_time() -> u64 {
    offchain::timestamp().unix_millis()
}

pub(crate) fn get_offchain_storage_value<T: Decode>(key: &[u8]) -> Option<T> {
    let storage_value_ref = StorageValueRef::persistent(key);
    storage_value_ref.get::<T>().ok().flatten()
}

pub(crate) fn set_program<T: pallet_gear::Config>(
    program_id: ProgramId,
    who: H256,
    bn: u32,
) -> Program {
    let code = Code::try_new(
        hex!("0061736d01000000010401600000020f0103656e76066d656d6f727902000103020100070a010668616e646c6500000a040102000b0019046e616d650203010000060d01000a656e762e6d656d6f7279").to_vec(),
        1,
        |_| ConstantCostRules::default(),
    )
    .expect("Error creating Code");

    let code_and_id = CodeAndId::new(code);

    let code_hash = code_and_id.code_id().into_origin();
    let _ = T::CodeStorage::add_code(code_and_id.clone(), CodeMetadata::new(who, bn));

    let code_and_id: InstrumentedCodeAndId = code_and_id.into();
    let (code, _) = code_and_id.into_parts();
    let program = Program::new(program_id, code);

    common::set_program(
        H256::from_slice(program.id().as_ref()),
        common::ActiveProgram {
            allocations: program.get_allocations().clone(),
            pages_with_data: Default::default(),
            code_hash,
            state: common::ProgramState::Initialized,
        },
    );

    program
}

pub(crate) fn decode_txs(pool: Arc<RwLock<PoolState>>) -> Vec<Extrinsic> {
    pool.read()
        .transactions
        .iter()
        .cloned()
        .map(|bytes| Extrinsic::decode(&mut &bytes[..]).unwrap())
        .collect::<Vec<_>>()
}
