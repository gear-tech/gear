// This file is part of Gear.

// Copyright (C) 2021-2024 Gear Technologies Inc.
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

use frame_support::assert_ok;
use gprimitives::ActorId;

use sp_staking::StakingAccount;
use util::*;

#[test]
fn bonding_works() {
    init_logger();

    new_test_ext().execute_with(|| {
        let contract_id = ProgramId::generate_from_user(CodeId::generate(WASM_BINARY), b"contract");
        let contract_account_id = AccountId::from_origin(contract_id.into_origin());

        // This pours the ED onto the contract's account
        deploy_broker_contract();
        run_to_next_block();

        let signer_current_balance_at_blk_1 = Balances::free_balance(SIGNER);

        // Measure necessary gas in a transaction
        let gas_info = |bonded: u128, value: Option<u128>| {
            start_transaction();
            let res = Gear::calculate_gas_info(
                SIGNER.into_origin(),
                pallet_gear::manager::HandleKind::Handle(contract_id),
                Request::Bond {
                    value: bonded,
                    payee: RewardAccount::Program,
                }
                .encode(),
                value.unwrap_or(bonded),
                true,
                None,
                None,
            )
            .expect("calculate_gas_info failed");
            rollback_transaction();
            res
        };
        let gas_burned = gas_info(100 * UNITS, None).burned;

        // Ensure the state hasn't changed
        assert_eq!(
            Balances::free_balance(SIGNER),
            signer_current_balance_at_blk_1
        );

        // Asserting success
        send_bond_message(contract_id, 100 * UNITS, None);
        run_to_next_block();

        let signer_current_balance_at_blk_2 = Balances::free_balance(SIGNER);
        let contract_account_data = System::account(contract_account_id).data;

        // SIGNER has spent in current block:
        // - 100 UNITS sent as value to the contract
        // - paid for the burned gas
        assert_eq!(
            signer_current_balance_at_blk_2,
            signer_current_balance_at_blk_1 - 100 * UNITS - gas_price(gas_burned)
        );

        // The contract's account has 10 * UNITS of the ED and 100 * UNITS of the bonded funds
        assert_eq!(contract_account_data.free, 110 * UNITS);
        // and all of it is frozen as bonded or locked
        assert_eq!(contract_account_data.frozen, 100 * UNITS);

        // Asserting the expected events are present
        assert_staking_events(contract_account_id, 100 * UNITS, EventType::Bonded);

        System::reset_events();

        // Measure necessary gas again as underlying runtime call should be different this time:
        // - `bond_extra` instead of `bond`
        let gas_burned = gas_info(50 * UNITS, Some(100 * UNITS)).burned;

        // Asserting success again (the contract should be able to figure out that `bond_extra`
        // should be called instead).
        // Note: the actual added amount is limited by the message `value` field, that is
        // it's going to be 50 UNITS, not 100 UNITS as encoded in the message payload.
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(SIGNER),
            contract_id,
            Request::Bond {
                value: 50 * UNITS,
                payee: RewardAccount::Program
            }
            .encode(),
            10_000_000_000,
            100 * UNITS,
            false,
        ));

        run_to_next_block();

        // SIGNER has spent since last time:
        // - 50 UNITS sent as value to the contract
        // - paid for gas
        assert_eq!(
            Balances::free_balance(SIGNER),
            signer_current_balance_at_blk_2 - 100 * UNITS - gas_price(gas_burned)
        );
        // Another 50 * UNITS added to locked balance
        assert_eq!(
            System::account(contract_account_id).data.frozen,
            150 * UNITS
        );

        // Asserting the expected events are present
        assert_staking_events(contract_account_id, 50 * UNITS, EventType::Bonded);
    });
}

#[test]
fn unbonding_works() {
    init_logger();

    new_test_ext().execute_with(|| {
        let contract_id = ProgramId::generate_from_user(CodeId::generate(WASM_BINARY), b"contract");
        let contract_account_id = AccountId::from_origin(contract_id.into_origin());

        deploy_broker_contract();
        run_to_next_block();

        send_bond_message(contract_id, 100 * UNITS, None);
        run_to_next_block();

        // Asserting the expected events are present
        assert_staking_events(contract_account_id, 100 * UNITS, EventType::Bonded);

        System::reset_events();

        // Measure necessary gas in a transaction for `unbond` message
        start_transaction();
        let _gas_info = Gear::calculate_gas_info(
            SIGNER.into_origin(),
            pallet_gear::manager::HandleKind::Handle(contract_id),
            Request::Unbond { value: 200 * UNITS }.encode(),
            0,
            true,
            None,
            None,
        )
        .expect("calculate_gas_info failed");
        rollback_transaction();

        // Sending `unbond` message
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(SIGNER),
            contract_id,
            // expecting to unbond only 100 UNITS despite 200 UNITS are being requested
            Request::Unbond { value: 200 * UNITS }.encode(),
            10_000_000_000,
            0,
            false,
        ));

        run_to_next_block();

        // Asserting the expected events are present
        assert_staking_events(contract_account_id, 100 * UNITS, EventType::Unbonded);
    });
}

#[test]
fn payload_size_matters() {
    init_logger();

    new_test_ext().execute_with(|| {
        let contract_id = ProgramId::generate_from_user(CodeId::generate(WASM_BINARY), b"contract");

        deploy_broker_contract();
        run_to_next_block();

        // Prepare large payload
        let mut targets = Vec::<ActorId>::new();
        for i in 100_u64..200_u64 {
            targets.push(i.cast());
        }

        System::reset_events();
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(SIGNER),
            contract_id,
            Request::Nominate {
                targets: targets.clone()
            }
            .encode(),
            10_000_000_000,
            0,
            false,
        ));

        run_to_next_block();
        // No staking-related events should have been emitted
        assert_no_staking_events();

        // Error message has been sent to the user
        assert_error_message_sent();

        // User message payload indicates the error
        assert_payload_contains("Message decoding error");
    });
}

#[test]
fn nominating_works() {
    init_logger();

    new_test_ext().execute_with(|| {
        let contract_id = ProgramId::generate_from_user(CodeId::generate(WASM_BINARY), b"contract");
        let contract_account_id = AccountId::from_origin(contract_id.into_origin());

        deploy_broker_contract();
        run_to_next_block();

        let targets: Vec<ActorId> = vec![VAL_1_STASH, VAL_2_STASH]
            .into_iter()
            .map(|x| x.cast())
            .collect();

        // Doesn't work without bonding first
        System::reset_events();
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(SIGNER),
            contract_id,
            Request::Nominate {
                targets: targets.clone()
            }
            .encode(),
            10_000_000_000,
            0,
            false,
        ));

        run_to_next_block();
        // No staking-related events should have been emitted
        assert_no_staking_events();
        // Error message has been sent to the user
        assert_error_message_sent();

        // Bond some funds on behalf of the contract first
        System::reset_events();
        send_bond_message(contract_id, 100 * UNITS, None);
        run_to_next_block();
        assert_staking_events(contract_account_id, 100 * UNITS, EventType::Bonded);

        let targets_before = pallet_staking::Nominators::<Test>::get(contract_account_id)
            .map_or_else(Vec::new, |x| x.targets.into_inner());
        assert_eq!(targets_before.len(), 0);

        // Now expecting nominating to work
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(SIGNER),
            contract_id,
            Request::Nominate {
                targets: targets.clone()
            }
            .encode(),
            10_000_000_000,
            0,
            false,
        ));

        run_to_next_block();

        let targets_after = pallet_staking::Nominators::<Test>::get(contract_account_id)
            .map_or_else(Vec::new, |x| x.targets.into_inner());
        assert_eq!(targets_after.len(), targets.len());
    });
}

#[test]
fn withdraw_unbonded_works() {
    init_logger();

    new_test_ext().execute_with(|| {
        let contract_id = ProgramId::generate_from_user(CodeId::generate(WASM_BINARY), b"contract");
        let contract_account_id = AccountId::from_origin(contract_id.into_origin());

        deploy_broker_contract();
        run_to_next_block();

        send_bond_message(contract_id, 500 * UNITS, None);
        run_to_next_block();
        assert_staking_events(contract_account_id, 500 * UNITS, EventType::Bonded);

        let contract_account_data = System::account(contract_account_id).data;

        // Locked 500 * UNITS as bonded on contracts's account
        assert_eq!(contract_account_data.frozen, 500 * UNITS);

        System::reset_events();

        // Sending `unbond` message
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(SIGNER),
            contract_id,
            Request::Unbond { value: 200 * UNITS }.encode(),
            10_000_000_000,
            0,
            false,
        ));

        run_to_next_block();
        assert_staking_events(contract_account_id, 200 * UNITS, EventType::Unbonded);

        // The funds are still locked
        assert_eq!(
            System::account(contract_account_id).data.frozen,
            500 * UNITS
        );

        // Pretend we have run the chain for at least the `unbonding period` number of eras
        pallet_staking::CurrentEra::<Test>::put(
            <Test as pallet_staking::Config>::BondingDuration::get() + 1_u32,
        );

        // Sending `withdraw_unbonded` message
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(SIGNER),
            contract_id,
            Request::WithdrawUnbonded {
                num_slashing_spans: 0
            }
            .encode(),
            10_000_000_000,
            0,
            false,
        ));

        run_to_next_block();

        // 200 * UNITS have been released, 300 * UNITS remain locked
        assert_eq!(
            System::account(contract_account_id).data.frozen,
            300 * UNITS
        );
        assert_staking_events(contract_account_id, 200 * UNITS, EventType::Withdrawn);
        let ledger =
            pallet_staking::Pallet::<Test>::ledger(StakingAccount::Stash(contract_account_id))
                .unwrap();
        assert_eq!(ledger.active, 300 * UNITS);
    });
}

#[test]
fn set_payee_works() {
    init_logger();

    new_test_ext().execute_with(|| {
        let contract_id = ProgramId::generate_from_user(CodeId::generate(WASM_BINARY), b"contract");
        let contract_account_id = AccountId::from_origin(contract_id.into_origin());

        deploy_broker_contract();
        run_to_next_block();

        // Bond funds with the `payee`` set to contract's stash (default)
        send_bond_message(contract_id, 100 * UNITS, None);
        run_to_next_block();
        assert_staking_events(contract_account_id, 100 * UNITS, EventType::Bonded);

        // Assert the `payee` is set to contract's stash
        let payee =
            pallet_staking::Pallet::<Test>::payee(StakingAccount::Stash(contract_account_id));
        assert_eq!(payee, pallet_staking::RewardDestination::Stash);

        // Set the `payee` to SIGNER
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(SIGNER),
            contract_id,
            Request::SetPayee {
                payee: RewardAccount::Custom(REWARD_PAYEE.into_origin().into())
            }
            .encode(),
            10_000_000_000,
            0,
            false,
        ));

        run_to_next_block();

        // Assert the `payee` is now set to SIGNER
        let payee =
            pallet_staking::Pallet::<Test>::payee(StakingAccount::Stash(contract_account_id));
        assert_eq!(
            payee,
            pallet_staking::RewardDestination::Account(REWARD_PAYEE)
        );
    });
}

#[test]
fn rebond_works() {
    init_logger();

    new_test_ext().execute_with(|| {
        let contract_id = ProgramId::generate_from_user(CodeId::generate(WASM_BINARY), b"contract");
        let contract_account_id = AccountId::from_origin(contract_id.into_origin());

        deploy_broker_contract();
        run_to_next_block();

        send_bond_message(contract_id, 500 * UNITS, None);
        run_to_next_block();
        assert_staking_events(contract_account_id, 500 * UNITS, EventType::Bonded);

        let contract_account_data = System::account(contract_account_id).data;

        // Locked 500 * UNITS as bonded on contracts's account
        assert_eq!(contract_account_data.frozen, 500 * UNITS);

        System::reset_events();

        // Sending `unbond` message
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(SIGNER),
            contract_id,
            Request::Unbond { value: 400 * UNITS }.encode(),
            10_000_000_000,
            0,
            false,
        ));

        run_to_next_block();
        assert_staking_events(contract_account_id, 400 * UNITS, EventType::Unbonded);

        // All the bonded funds are still locked
        assert_eq!(
            System::account(contract_account_id).data.frozen,
            500 * UNITS
        );

        // However, the ledger has been updated
        let ledger =
            pallet_staking::Pallet::<Test>::ledger(StakingAccount::Stash(contract_account_id))
                .unwrap();
        assert_eq!(ledger.active, 100 * UNITS);
        assert_eq!(ledger.unlocking.len(), 1);

        // Sending `rebond` message
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(SIGNER),
            contract_id,
            Request::Rebond { value: 200 * UNITS }.encode(),
            10_000_000_000,
            0,
            false,
        ));

        run_to_next_block();

        // All the bonded funds are still locked
        assert_eq!(
            System::account(contract_account_id).data.frozen,
            500 * UNITS
        );

        // However, the ledger has been updated again
        let ledger =
            pallet_staking::Pallet::<Test>::ledger(StakingAccount::Stash(contract_account_id))
                .unwrap();
        assert_eq!(ledger.active, 300 * UNITS);
        assert_eq!(ledger.unlocking.len(), 1);

        // Sending another `rebond` message, with `value` exceeding the unlocking amount
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(SIGNER),
            contract_id,
            Request::Rebond { value: 300 * UNITS }.encode(),
            10_000_000_000,
            0,
            false,
        ));

        run_to_next_block();

        // All the bonded funds are still locked
        assert_eq!(
            System::account(contract_account_id).data.frozen,
            500 * UNITS
        );

        // The ledger has been updated again, however, the rebonded amount was limited
        // by the actual unlocking amount - not the `value` sent in the message.
        let ledger =
            pallet_staking::Pallet::<Test>::ledger(StakingAccount::Stash(contract_account_id))
                .unwrap();
        assert_eq!(ledger.active, 500 * UNITS);
        assert_eq!(ledger.unlocking.len(), 0);
    });
}

#[test]
fn payout_stakers_works() {
    init_logger();

    new_test_ext().execute_with(|| {
        let contract_id = ProgramId::generate_from_user(CodeId::generate(WASM_BINARY), b"contract");
        let contract_account_id = AccountId::from_origin(contract_id.into_origin());

        deploy_broker_contract();
        run_to_next_block();

        // Only nominating one target
        let targets: Vec<ActorId> = vec![VAL_1_STASH.cast()];

        // Bonding a quarter of validator's stake for easier calculations
        send_bond_message(
            contract_id,
            250 * UNITS,
            Some(RewardAccount::Custom(REWARD_PAYEE.into_origin().into())),
        );
        run_to_next_block();
        assert_staking_events(contract_account_id, 250 * UNITS, EventType::Bonded);

        // Nomintate the validator
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(SIGNER),
            contract_id,
            Request::Nominate {
                targets: targets.clone()
            }
            .encode(),
            10_000_000_000,
            0,
            false,
        ));

        run_to_next_block();

        let targets = pallet_staking::Nominators::<Test>::get(contract_account_id)
            .unwrap()
            .targets
            .into_inner();
        assert_eq!(targets, vec![VAL_1_STASH]);

        let rewards_payee_initial_balance = Balances::free_balance(REWARD_PAYEE);
        assert_eq!(rewards_payee_initial_balance, ENDOWMENT);

        // Actually run the chain for a few eras (5) to accumulate some rewards
        run_for_n_blocks(
            5 * SESSION_DURATION * <Test as pallet_staking::Config>::SessionsPerEra::get() as u64,
        );

        // Send `payout_stakers` message for an era for which the rewards should have been earned
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(SIGNER),
            contract_id,
            Request::PayoutStakers {
                validator_stash: VAL_1_STASH.into_origin().into(),
                era: 1
            }
            .encode(),
            300_000_000_000,
            0,
            false,
        ));

        run_to_next_block();

        // Expecting the nominator to have received 1/5 of the rewards
        let rewards_payee_final_balance = Balances::free_balance(REWARD_PAYEE);
        assert_eq!(
            rewards_payee_final_balance,
            rewards_payee_initial_balance + (100 * UNITS) / 5
        );
    });
}

mod util {
    pub(super) use crate::mock::{
        BLOCK_AUTHOR, ENDOWMENT, EXISTENTIAL_DEPOSIT, MILLISECS_PER_BLOCK, SIGNER, UNITS,
        VAL_1_STASH, VAL_2_STASH, VAL_3_STASH,
    };
    use crate::{self as pallet_gear_builtin, staking::Actor as StakingBuiltin};
    pub(super) use common::Origin;
    pub(super) use demo_staking_broker::WASM_BINARY;
    use frame_election_provider_support::{
        bounds::{ElectionBounds, ElectionBoundsBuilder},
        onchain, SequentialPhragmen,
    };
    use frame_support::{
        assert_ok, construct_runtime, parameter_types,
        traits::{ConstBool, ConstU64, FindAuthor, OnFinalize, OnInitialize},
    };
    use frame_support_test::TestRandomness;
    use frame_system::{self as system, pallet_prelude::BlockNumberFor};
    pub(super) use gbuiltin_staking::{Request, RewardAccount};
    pub(super) use gear_core::ids::{prelude::*, CodeId, ProgramId};
    use gear_core_errors::{ErrorReplyReason, ReplyCode, SimpleExecutionError};
    use pallet_session::historical::{self as pallet_session_historical};
    pub(super) use parity_scale_codec::Encode;
    use sp_core::{crypto::key_types, H256};
    use sp_runtime::{
        testing::UintAuthorityId,
        traits::{BlakeTwo256, ConstU32, IdentityLookup, OpaqueKeys},
        BuildStorage, KeyTypeId, Perbill, Permill,
    };
    use sp_std::convert::{TryFrom, TryInto};

    pub(super) const SESSION_DURATION: u64 = 250;
    pub(super) const REWARD_PAYEE: AccountId = 2;
    const VAL_1_AUTH_ID: UintAuthorityId = UintAuthorityId(11);
    const VAL_2_AUTH_ID: UintAuthorityId = UintAuthorityId(21);
    const VAL_3_AUTH_ID: UintAuthorityId = UintAuthorityId(31);

    pub(super) type AccountId = u64;
    type BlockNumber = u64;
    type Balance = u128;
    type Block = frame_system::mocking::MockBlock<Test>;

    // Configure a mock runtime to test the pallet.
    construct_runtime!(
        pub enum Test
        {
            System: system,
            Balances: pallet_balances,
            Authorship: pallet_authorship,
            Timestamp: pallet_timestamp,
            Session: pallet_session,
            Historical: pallet_session_historical,
            Staking: pallet_staking,
            GearProgram: pallet_gear_program,
            GearMessenger: pallet_gear_messenger,
            GearScheduler: pallet_gear_scheduler,
            GearBank: pallet_gear_bank,
            Gear: pallet_gear,
            GearGas: pallet_gear_gas,
            GearBuiltin: pallet_gear_builtin,
        }
    );

    parameter_types! {
        pub const BlockHashCount: u64 = 250;
        pub const ExistentialDeposit: Balance = EXISTENTIAL_DEPOSIT;
        pub ElectionBoundsOnChain: ElectionBounds = ElectionBoundsBuilder::default().build();
    }

    common::impl_pallet_system!(Test);
    common::impl_pallet_balances!(Test);
    common::impl_pallet_authorship!(Test, EventHandler = Staking);
    common::impl_pallet_timestamp!(Test);

    // Fixed payout for each era
    pub struct FixedEraPayout<const PAYOUT: u128>;
    impl<const PAYOUT: u128> pallet_staking::EraPayout<u128> for FixedEraPayout<PAYOUT> {
        fn era_payout(
            _total_staked: u128,
            _total_issuance: u128,
            _era_duration_millis: u64,
        ) -> (u128, u128) {
            (PAYOUT, 0)
        }
    }

    pub struct OnChainSeqPhragmen;
    impl onchain::Config for OnChainSeqPhragmen {
        type System = Test;
        type Solver = SequentialPhragmen<AccountId, Perbill>;
        type DataProvider = Staking;
        type WeightInfo = ();
        type MaxWinners = ConstU32<100>;
        type Bounds = ElectionBoundsOnChain;
    }

    common::impl_pallet_staking!(
        Test,
        EraPayout = FixedEraPayout::<{ 100 * UNITS }>,
        NextNewSession = Session,
        ElectionProvider = onchain::OnChainExecution<OnChainSeqPhragmen>,
        GenesisElectionProvider = onchain::OnChainExecution<OnChainSeqPhragmen>,
    );

    parameter_types! {
        pub const BlockGasLimit: u64 = 350_000_000_000;
        pub const OutgoingLimit: u32 = 1024;
        pub const OutgoingBytesLimit: u32 = 64 * 1024 * 1024;
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

    pub struct TestSessionHandler;
    impl pallet_session::SessionHandler<AccountId> for TestSessionHandler {
        const KEY_TYPE_IDS: &'static [KeyTypeId] = &[key_types::DUMMY];

        fn on_new_session<Ks: OpaqueKeys>(
            _changed: bool,
            _validators: &[(AccountId, Ks)],
            _queued_validators: &[(AccountId, Ks)],
        ) {
        }

        fn on_disabled(_validator_index: u32) {}

        fn on_genesis_session<Ks: OpaqueKeys>(_validators: &[(AccountId, Ks)]) {}
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

    impl pallet_gear_builtin::Config for Test {
        type RuntimeCall = RuntimeCall;
        type Builtins = (StakingBuiltin<Self>,);
        type WeightInfo = ();
    }

    // Build genesis storage according to the mock runtime.
    #[derive(Default)]
    pub struct ExtBuilder {
        initial_authorities: Vec<(AccountId, UintAuthorityId)>,
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

        pub fn initial_authorities(
            mut self,
            authorities: Vec<(AccountId, UintAuthorityId)>,
        ) -> Self {
            self.initial_authorities = authorities;
            self
        }

        pub fn build(self) -> sp_io::TestExternalities {
            let mut storage = system::GenesisConfig::<Test>::default()
                .build_storage()
                .unwrap();

            pallet_balances::GenesisConfig::<Test> {
                balances: self
                    .initial_authorities
                    .iter()
                    .map(|x| (x.0, self.endowment))
                    .chain(self.endowed_accounts.iter().map(|k| (*k, self.endowment)))
                    .collect(),
            }
            .assimilate_storage(&mut storage)
            .unwrap();

            pallet_session::GenesisConfig::<Test> {
                keys: self
                    .initial_authorities
                    .iter()
                    .map(|x| (x.0, x.0, x.1.clone()))
                    .collect(),
            }
            .assimilate_storage(&mut storage)
            .unwrap();

            pallet_staking::GenesisConfig::<Test> {
                validator_count: self.initial_authorities.len() as u32,
                minimum_validator_count: self.initial_authorities.len() as u32,
                stakers: self
                    .initial_authorities
                    .iter()
                    .map(|x| {
                        (
                            x.0,
                            x.0,
                            self.endowment,
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

            let mut ext: sp_io::TestExternalities = storage.into();

            ext.execute_with(|| {
                let new_blk = 1;
                System::set_block_number(new_blk);
                on_initialize(new_blk);
            });
            ext
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
        Authorship::on_finalize(current_blk);
        Staking::on_finalize(current_blk);
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
    }

    pub(crate) fn rollback_transaction() {
        sp_externalities::with_externalities(|ext| {
            ext.storage_rollback_transaction()
                .expect("ongoing transaction must be there");
        })
        .expect("externalities should be set");
    }

    pub(crate) fn new_test_ext() -> sp_io::TestExternalities {
        let bank_address = <Test as pallet_gear_bank::Config>::BankAddress::get();

        ExtBuilder::default()
            .initial_authorities(vec![
                (VAL_1_STASH, VAL_1_AUTH_ID),
                (VAL_2_STASH, VAL_2_AUTH_ID),
                (VAL_3_STASH, VAL_3_AUTH_ID),
            ])
            .endowment(ENDOWMENT)
            .endowed_accounts(vec![bank_address, SIGNER, REWARD_PAYEE])
            .build()
    }

    pub(super) fn init_logger() {
        let _ = env_logger::Builder::from_default_env()
            .format_module_path(false)
            .format_level(true)
            .try_init();
    }

    pub(super) fn deploy_broker_contract() {
        assert_ok!(Gear::upload_program(
            RuntimeOrigin::signed(SIGNER),
            WASM_BINARY.to_vec(),
            b"contract".to_vec(),
            Default::default(),
            10_000_000_000,
            0,
            false,
        ));
    }

    pub(super) fn send_bond_message(
        contract_id: ProgramId,
        amount: Balance,
        payee: Option<RewardAccount>,
    ) {
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(SIGNER),
            contract_id,
            Request::Bond {
                value: amount,
                payee: payee.unwrap_or(RewardAccount::Program)
            }
            .encode(),
            10_000_000_000,
            amount,
            false,
        ));
    }

    #[derive(PartialEq)]
    pub(super) enum EventType {
        Bonded,
        Unbonded,
        Withdrawn,
    }

    pub(super) fn assert_staking_events(contract_id: AccountId, balance: Balance, t: EventType) {
        assert!(System::events().into_iter().any(|e| {
            match e.event {
                RuntimeEvent::Staking(pallet_staking::Event::<Test>::Bonded { stash, amount }) => {
                    t == EventType::Bonded && stash == contract_id && balance == amount
                }
                RuntimeEvent::Staking(pallet_staking::Event::<Test>::Unbonded {
                    stash,
                    amount,
                }) => t == EventType::Unbonded && stash == contract_id && balance == amount,
                RuntimeEvent::Staking(pallet_staking::Event::<Test>::Withdrawn {
                    stash,
                    amount,
                }) => t == EventType::Withdrawn && stash == contract_id && balance == amount,
                _ => false,
            }
        }))
    }

    pub(super) fn assert_no_staking_events() {
        assert!(System::events()
            .into_iter()
            .all(|e| { !matches!(e.event, RuntimeEvent::Staking(_)) }))
    }

    pub(super) fn assert_error_message_sent() {
        assert!(System::events().into_iter().any(|e| {
            match e.event {
                RuntimeEvent::Gear(pallet_gear::Event::UserMessageSent { message, .. }) => {
                    match message.details() {
                        Some(details) => {
                            details.to_reply_code()
                                == ReplyCode::Error(ErrorReplyReason::Execution(
                                    SimpleExecutionError::UserspacePanic,
                                ))
                        }
                        _ => false,
                    }
                }
                _ => false,
            }
        }))
    }

    pub(super) fn assert_payload_contains(s: &'static str) {
        assert!(System::events().into_iter().any(|e| {
            match e.event {
                RuntimeEvent::Gear(pallet_gear::Event::UserMessageSent { message, .. }) => {
                    let s_bytes = s.as_bytes();
                    message
                        .payload_bytes()
                        .windows(s_bytes.len())
                        .any(|window| window == s_bytes)
                }
                _ => false,
            }
        }))
    }
}
