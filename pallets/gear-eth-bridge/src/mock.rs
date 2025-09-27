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

use crate::{self as pallet_gear_eth_bridge};
use common::Origin as _;
use frame_support::{
    PalletId, construct_runtime, parameter_types,
    traits::{ConstBool, ConstU32, ConstU64, FindAuthor, Hooks, SortedMembers},
};
use frame_support_test::TestRandomness;
use frame_system::{self as system, EnsureSignedBy, pallet_prelude::BlockNumberFor};
use gprimitives::ActorId;
use pallet_gear_builtin::ActorWithId;
use pallet_session::{SessionManager, ShouldEndSession};
use sp_core::{H256, ed25519::Public};
use sp_runtime::{
    BuildStorage, impl_opaque_keys,
    traits::{AccountIdConversion, BlakeTwo256, IdentityLookup},
};
use sp_std::convert::{TryFrom, TryInto};

pub type AccountId = u64;
type BlockNumber = u64;
type Balance = u128;
type Block = frame_system::mocking::MockBlock<Test>;
pub type Moment = u64;

pub(crate) const SIGNER: AccountId = 1;
pub(crate) const BLOCK_AUTHOR: AccountId = 10001;

pub(crate) const EXISTENTIAL_DEPOSIT: u128 = UNITS;
pub(crate) const ENDOWMENT: u128 = 1_000 * UNITS;

pub(crate) const UNITS: u128 = 1_000_000_000_000; // 10^(-12) precision
pub(crate) const MILLISECS_PER_BLOCK: u64 = 3_000;

// Configure a mock runtime to test the pallet.
construct_runtime!(
    pub enum Test
    {
        System: system,
        Timestamp: pallet_timestamp,
        Authorship: pallet_authorship,
        Grandpa: pallet_grandpa,
        Balances: pallet_balances,
        Session: pallet_session,

        GearProgram: pallet_gear_program,
        GearMessenger: pallet_gear_messenger,
        GearScheduler: pallet_gear_scheduler,
        GearBank: pallet_gear_bank,
        Gear: pallet_gear,
        GearGas: pallet_gear_gas,
        GearBuiltin: pallet_gear_builtin,
        GearEthBridge: pallet_gear_eth_bridge,
    }
);

impl_opaque_keys! {
    pub struct SessionKeys {
        pub grandpa: Grandpa,
    }
}

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

            log::debug!(
                "on_new_session(changed={changed}, validators={validators:?}, queued_validators={queued_validators:?})"
            );

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

pub type VaraSessionHandler = (grandpa_keys_handler::GrandpaAndGearEthBridge,);

parameter_types! {
    pub const BlockHashCount: u64 = 250;
    pub const ExistentialDeposit: Balance = EXISTENTIAL_DEPOSIT;
}

common::impl_pallet_system!(Test);
common::impl_pallet_balances!(Test);
common::impl_pallet_authorship!(Test);
common::impl_pallet_timestamp!(Test);

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
    pub const MockTransportFee: Balance = UNITS;
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

pub const BUILTIN_ID: u64 = 3;

pub(crate) fn mock_builtin_id() -> ActorId {
    GearBuiltin::generate_actor_id(BUILTIN_ID)
}

impl pallet_gear_builtin::Config for Test {
    type RuntimeCall = RuntimeCall;
    type Builtins = (ActorWithId<BUILTIN_ID, crate::builtin::Actor<Test>>,);
    type BlockLimiter = GearGas;
    type WeightInfo = ();
}

pub const EPOCH_DURATION_IN_BLOCKS: BlockNumber = 6;

pub const SLOT_DURATION: Moment = MILLISECS_PER_BLOCK;

pub const EPOCH_DURATION_IN_SLOTS: u64 = {
    const SLOT_FILL_RATE: f64 = MILLISECS_PER_BLOCK as f64 / SLOT_DURATION as f64;

    (EPOCH_DURATION_IN_BLOCKS as f64 * SLOT_FILL_RATE) as u64
};

parameter_types! {
    pub const SessionsPerEra: u32 = 6;
    pub const EpochDuration: u64 = EPOCH_DURATION_IN_SLOTS;
    pub const ExpectedBlockTime: Moment = MILLISECS_PER_BLOCK;
    pub const MaxAuthorities: u32 = 100_000;
    pub const MaxNominatorRewardedPerValidator: u32 = 256;
}

impl pallet_grandpa::Config for Test {
    type RuntimeEvent = RuntimeEvent;

    type WeightInfo = ();
    type MaxAuthorities = MaxAuthorities;
    type MaxNominators = MaxNominatorRewardedPerValidator;
    type MaxSetIdSessionEntries = ();
    type KeyOwnerProof = sp_session::MembershipProof;
    type EquivocationReportSystem = ();
}

pub struct TestSessionRotator;

impl ShouldEndSession<BlockNumber> for TestSessionRotator {
    fn should_end_session(now: BlockNumber) -> bool {
        if now > 1 {
            (now - 1).is_multiple_of(EpochDuration::get())
        } else {
            false
        }
    }
}

pub fn era_validators(session_idx: u32, do_set_keys: bool) -> Vec<AccountId> {
    let era = session_idx / SessionsPerEra::get() + 1;

    let first_validator = 1_000 + era as u64;
    let last_validator = first_validator + 3;

    (first_validator..last_validator)
        .inspect(|&acc| {
            if do_set_keys {
                let grandpa = account_into_grandpa_key(acc);
                pallet_session::NextKeys::<Test>::insert(acc, SessionKeys { grandpa });
            }
        })
        .collect()
}

pub fn era_validators_authority_set(
    session_idx: u32,
) -> Vec<(
    sp_consensus_grandpa::AuthorityId,
    sp_consensus_grandpa::AuthorityWeight,
)> {
    era_validators(session_idx, false)
        .into_iter()
        .map(account_into_grandpa_pair)
        .collect()
}

pub fn account_into_grandpa_key(id: AccountId) -> sp_consensus_grandpa::AuthorityId {
    Public::from_raw(ActorId::from(id).into_bytes()).into()
}

pub fn account_into_grandpa_pair(
    id: AccountId,
) -> (
    sp_consensus_grandpa::AuthorityId,
    sp_consensus_grandpa::AuthorityWeight,
) {
    (account_into_grandpa_key(id), 1)
}

pub struct TestSessionManager;

impl SessionManager<AccountId> for TestSessionManager {
    fn new_session(session_idx: u32) -> Option<Vec<AccountId>> {
        session_idx
            .is_multiple_of(SessionsPerEra::get())
            .then(|| era_validators(session_idx, true))
    }
    fn start_session(_: u32) {}
    fn end_session(_: u32) {}
}

impl pallet_session::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type ValidatorId = <Self as frame_system::Config>::AccountId;
    type ValidatorIdOf = ();
    type ShouldEndSession = TestSessionRotator;
    type NextSessionRotation = ();
    type SessionManager = TestSessionManager;
    type SessionHandler = VaraSessionHandler;
    type Keys = SessionKeys;
    type WeightInfo = pallet_session::weights::SubstrateWeight<Test>;
}

parameter_types! {
    pub const GearEthBridgePalletId: PalletId = PalletId(*b"py/gethb");
    pub MockBridgeBuiltinAddress: AccountId = mock_builtin_id().cast();

    pub MockBridgeAdminAccount: AccountId = GearEthBridgePalletId::get().into_sub_account_truncating("bridge_admin");
    pub MockBridgePauserAccount: AccountId = GearEthBridgePalletId::get().into_sub_account_truncating("bridge_pauser");
}

pub struct MockBridgeAdminAccounts;
impl SortedMembers<AccountId> for MockBridgeAdminAccounts {
    fn sorted_members() -> Vec<AccountId> {
        vec![MockBridgeAdminAccount::get()]
    }
}

impl pallet_gear_eth_bridge::Config for Test {
    type AdminOrigin = EnsureSignedBy<MockBridgeAdminAccounts, AccountId>;
    type PalletId = GearEthBridgePalletId;
    type BuiltinAddress = MockBridgeBuiltinAddress;
    type RuntimeEvent = RuntimeEvent;
    type MaxPayloadSize = ConstU32<1024>;
    type QueueCapacity = ConstU32<32>;
    type SessionsPerEra = SessionsPerEra;
    type BridgeAdmin = MockBridgeAdminAccount;
    type BridgePauser = MockBridgePauserAccount;
    type WeightInfo = ();
}

// Build genesis storage according to the mock runtime.
#[derive(Default)]
pub struct ExtBuilder {
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

        let keys = era_validators(0, false)
            .into_iter()
            .map(|i| {
                let grandpa = account_into_grandpa_key(i);

                (i, i, SessionKeys { grandpa })
            })
            .collect();

        pallet_session::GenesisConfig::<Test> {
            keys,
            ..Default::default()
        }
        .assimilate_storage(&mut storage)
        .unwrap();

        let mut ext: sp_io::TestExternalities = storage.into();

        ext.execute_with(|| {
            on_initialize(1);
        });
        ext
    }
}

pub(crate) fn run_to_block(n: u64) {
    while System::block_number() < n {
        let current_blk = System::block_number();

        Gear::run(RuntimeOrigin::none(), None).unwrap();
        on_finalize(current_blk);

        let new_block_number = current_blk + 1;
        on_initialize(new_block_number);
    }
}

pub(crate) fn run_to_next_block() {
    run_for_n_blocks(1)
}

pub(crate) fn run_for_n_blocks(n: u64) {
    run_to_block(System::block_number() + n);
}

// Run on_initialize hooks in order as they appear in AllPalletsWithSystem.
pub(crate) fn on_initialize(new: BlockNumberFor<Test>) {
    System::set_block_number(new);
    Timestamp::set_timestamp(new.saturating_mul(MILLISECS_PER_BLOCK));
    Authorship::on_initialize(new);
    Grandpa::on_initialize(new);
    Balances::on_initialize(new);
    Session::on_initialize(new);

    GearProgram::on_initialize(new);
    GearMessenger::on_initialize(new);
    GearScheduler::on_initialize(new);
    GearBank::on_initialize(new);
    Gear::on_initialize(new);
    GearGas::on_initialize(new);
    GearBuiltin::on_initialize(new);
    GearEthBridge::on_initialize(new);
}

// Run on_finalize hooks (in pallets reverse order, as they appear in AllPalletsWithSystem)
pub(crate) fn on_finalize(bn: BlockNumberFor<Test>) {
    GearEthBridge::on_finalize(bn);
    GearBuiltin::on_finalize(bn);
    GearGas::on_finalize(bn);
    Gear::on_finalize(bn);
    GearBank::on_finalize(bn);
    GearScheduler::on_finalize(bn);
    GearMessenger::on_finalize(bn);
    GearProgram::on_finalize(bn);

    Session::on_finalize(bn);
    Balances::on_finalize(bn);
    Grandpa::on_finalize(bn);
    Authorship::on_finalize(bn);

    assert!(!System::events().iter().any(|e| {
        matches!(
            e.event,
            RuntimeEvent::Gear(pallet_gear::Event::QueueNotProcessed)
        )
    }))
}

pub(crate) fn on_finalize_gear_block(bn: BlockNumberFor<Test>) {
    Gear::run(frame_support::dispatch::RawOrigin::None.into(), None).unwrap();
    on_finalize(bn);
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

pub(crate) fn init_logger() {
    let _ = tracing_subscriber::fmt::try_init();
}
