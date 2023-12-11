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

//! Staking proxy built-in actor tests.

#![cfg(test)]

use crate::{mock::*, *};
use demo_staking_broker::WASM_BINARY;
use frame_support::assert_ok;
use gear_builtin_actor_common::staking::*;
use gear_core::ids::{CodeId, ProgramId};

type AccountIdOf<T> = <T as frame_system::Config>::AccountId;

pub(crate) fn init_logger() {
    let _ = env_logger::Builder::from_default_env()
        .format_module_path(false)
        .format_level(true)
        .try_init();
}

fn deploy_contract() {
    assert_ok!(Gear::upload_program(
        RuntimeOrigin::signed(SIGNER),
        WASM_BINARY.to_vec(),
        b"contract".to_vec(),
        Default::default(),
        10_000_000_000,
        EXISTENTIAL_DEPOSIT, // keep the contract's account "providing"
        false,
    ));
}

fn send_bond_message(
    contract_id: ProgramId,
    amount: BalanceOf<Test>,
    payee: Option<RewardAccount>,
) {
    assert_ok!(Gear::send_message(
        RuntimeOrigin::signed(SIGNER),
        contract_id,
        Request::V1(RequestV1::Bond {
            value: amount,
            payee
        })
        .encode(),
        10_000_000_000,
        amount,
        false,
    ));
}

fn assert_message_executed() {
    assert!(System::events().into_iter().any(|e| {
        match e.event {
            RuntimeEvent::GearBuiltinActor(Event::MessageExecuted { result }) => result.is_ok(),
            _ => false,
        }
    }))
}

#[derive(PartialEq)]
enum EventType {
    Bonded,
    Unbonded,
    Withdrawn,
}

fn assert_staking_events(contract_id: AccountIdOf<Test>, balance: BalanceOf<Test>, t: EventType) {
    assert!(System::events().into_iter().any(|e| {
        match e.event {
            RuntimeEvent::Staking(pallet_staking::Event::<Test>::Bonded { stash, amount }) => {
                t == EventType::Bonded && stash == contract_id && balance == amount
            }
            RuntimeEvent::Staking(pallet_staking::Event::<Test>::Unbonded { stash, amount }) => {
                t == EventType::Unbonded && stash == contract_id && balance == amount
            }
            RuntimeEvent::Staking(pallet_staking::Event::<Test>::Withdrawn { stash, amount }) => {
                t == EventType::Withdrawn && stash == contract_id && balance == amount
            }
            _ => false,
        }
    }))
}

fn assert_execution_with_error() {
    assert!(System::events().into_iter().any(|e| {
        match e.event {
            RuntimeEvent::GearBuiltinActor(Event::MessageExecuted { result }) => result.is_err(),
            _ => false,
        }
    }))
}

#[test]
fn user_message_to_builtin_actor_works() {
    init_logger();

    new_test_ext().execute_with(|| {
        let builtin_actor_id = pallet::Pallet::<Test>::staking_proxy_actor_id();

        // Asserting success
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(SIGNER),
            builtin_actor_id,
            Request::V1(RequestV1::Bond {
                value: 100 * UNITS,
                payee: None
            })
            .encode(),
            10_000_000_000,
            0,
            false,
        ));
        run_to_next_block();

        let signer_account_data = System::account(SIGNER).data;

        let gas_burned = <Test as Config>::RuntimeCall::from(pallet_staking::Call::<Test>::bond {
            controller: <Test as frame_system::Config>::Lookup::unlookup(SIGNER),
            value: 100 * UNITS,
            payee: RewardDestination::Stash,
        })
        .get_dispatch_info()
        .weight
        .saturating_add(<Test as Config>::WeightInfo::base_handle_weight())
        .ref_time();

        // SIGNER has:
        // - paid for some burned gas
        assert_eq!(signer_account_data.free, ENDOWMENT - gas_price(gas_burned));
        // - locked 100 * UNITS as bonded
        assert_eq!(signer_account_data.misc_frozen, 100 * UNITS);

        // Asserting the expected events are present
        assert_message_executed();
        assert_staking_events(SIGNER, 100 * UNITS, EventType::Bonded);
    });
}

#[test]
fn bonding_works() {
    init_logger();

    new_test_ext().execute_with(|| {
        let contract_id = ProgramId::generate_from_user(CodeId::generate(WASM_BINARY), b"contract");
        let contract_account_id = AccountIdOf::<Test>::from_origin(contract_id.into_origin());

        deploy_contract();
        run_to_next_block();

        let signer_current_balance_at_blk_1 = Balances::free_balance(SIGNER);

        // Measure necessary gas in a transaction
        let gas_info = |bonded: u128, value: Option<u128>| {
            start_transaction();
            let res = Gear::calculate_gas_info(
                SIGNER.into_origin(),
                pallet_gear::manager::HandleKind::Handle(contract_id),
                Request::V1(RequestV1::Bond {
                    value: bonded,
                    payee: None,
                })
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

        // The contract's account has the same 10 * UNITS of free balance (the ED)
        assert_eq!(contract_account_data.free, 110 * UNITS);
        // and 100 * UNITS of it is frozen as bonded
        assert_eq!(contract_account_data.misc_frozen, 100 * UNITS);

        // Asserting the expected events are present
        assert_message_executed();
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
            Request::V1(RequestV1::Bond {
                value: 50 * UNITS,
                payee: None
            })
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
            System::account(contract_account_id).data.misc_frozen,
            150 * UNITS
        );

        // Asserting the expected events are present
        assert_message_executed();
        assert_staking_events(contract_account_id, 50 * UNITS, EventType::Bonded);
    });
}

#[test]
fn unbonding_works() {
    init_logger();

    new_test_ext().execute_with(|| {
        let contract_id = ProgramId::generate_from_user(CodeId::generate(WASM_BINARY), b"contract");
        let contract_account_id = AccountIdOf::<Test>::from_origin(contract_id.into_origin());

        deploy_contract();
        run_to_next_block();

        send_bond_message(contract_id, 100 * UNITS, None);
        run_to_next_block();

        // Asserting the expected events are present
        assert_message_executed();
        assert_staking_events(contract_account_id, 100 * UNITS, EventType::Bonded);

        System::reset_events();

        // Measure necessary gas in a transaction for `unbond` message
        start_transaction();
        let _gas_info = Gear::calculate_gas_info(
            SIGNER.into_origin(),
            pallet_gear::manager::HandleKind::Handle(contract_id),
            Request::V1(RequestV1::Unbond { value: 200 * UNITS }).encode(),
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
            Request::V1(RequestV1::Unbond { value: 200 * UNITS }).encode(),
            10_000_000_000,
            0,
            false,
        ));

        run_to_next_block();

        // Asserting the expected events are present
        assert_message_executed();
        assert_staking_events(contract_account_id, 100 * UNITS, EventType::Unbonded);
    });
}

#[test]
fn nominating_works() {
    init_logger();

    new_test_ext().execute_with(|| {
        let contract_id = ProgramId::generate_from_user(CodeId::generate(WASM_BINARY), b"contract");
        let contract_account_id = AccountIdOf::<Test>::from_origin(contract_id.into_origin());

        deploy_contract();
        run_to_next_block();

        let targets: Vec<[u8; 32]> = vec![VAL_1_STASH, VAL_2_STASH]
            .into_iter()
            .map(|x| x.into_origin().into())
            .collect();

        // Doesn't work without bonding first
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(SIGNER),
            contract_id,
            Request::V1(RequestV1::Nominate {
                targets: targets.clone()
            })
            .encode(),
            10_000_000_000,
            0,
            false,
        ));

        run_to_next_block();
        // Make sure we have an "upstream" error packaged as an event
        assert_execution_with_error();

        System::reset_events();

        send_bond_message(contract_id, 100 * UNITS, None);
        run_to_next_block();
        assert_message_executed();
        assert_staking_events(contract_account_id, 100 * UNITS, EventType::Bonded);

        let targets_before = pallet_staking::Nominators::<Test>::get(contract_account_id)
            .map_or_else(Vec::new, |x| x.targets.into_inner());
        assert_eq!(targets_before.len(), 0);

        // Now expecting nominating to work
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(SIGNER),
            contract_id,
            Request::V1(RequestV1::Nominate {
                targets: targets.clone()
            })
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
        let contract_account_id = AccountIdOf::<Test>::from_origin(contract_id.into_origin());

        deploy_contract();
        run_to_next_block();

        send_bond_message(contract_id, 500 * UNITS, None);
        run_to_next_block();
        assert_message_executed();
        assert_staking_events(contract_account_id, 500 * UNITS, EventType::Bonded);

        let contract_account_data = System::account(contract_account_id).data;

        // Locked 500 * UNITS as bonded on contracts's account
        assert_eq!(contract_account_data.misc_frozen, 500 * UNITS);

        System::reset_events();

        // Sending `unbond` message
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(SIGNER),
            contract_id,
            Request::V1(RequestV1::Unbond { value: 200 * UNITS }).encode(),
            10_000_000_000,
            0,
            false,
        ));

        run_to_next_block();
        assert_message_executed();
        assert_staking_events(contract_account_id, 200 * UNITS, EventType::Unbonded);

        // The funds are still locked
        assert_eq!(
            System::account(contract_account_id).data.misc_frozen,
            500 * UNITS
        );

        // Roll to the end of the unbonding period
        run_for_n_blocks(
            SESSION_DURATION
                * <Test as pallet_staking::Config>::SessionsPerEra::get() as u64
                * <Test as pallet_staking::Config>::BondingDuration::get() as u64,
        );

        // Sending `withdraw_unbonded` message
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(SIGNER),
            contract_id,
            Request::V1(RequestV1::WithdrawUnbonded {
                num_slashing_spans: 0
            })
            .encode(),
            10_000_000_000,
            0,
            false,
        ));

        run_to_next_block();

        // 200 * UNITS have been released, 300 * UNITS remain locked
        assert_eq!(
            System::account(contract_account_id).data.misc_frozen,
            300 * UNITS
        );
        assert_staking_events(contract_account_id, 200 * UNITS, EventType::Withdrawn);
        let ledger = pallet_staking::Pallet::<Test>::ledger(contract_account_id).unwrap();
        assert_eq!(ledger.active, 300 * UNITS);
    });
}

#[test]
fn set_payee_works() {
    init_logger();

    new_test_ext().execute_with(|| {
        let contract_id = ProgramId::generate_from_user(CodeId::generate(WASM_BINARY), b"contract");
        let contract_account_id = AccountIdOf::<Test>::from_origin(contract_id.into_origin());

        deploy_contract();
        run_to_next_block();

        // Bond funds with the `payee`` set to contract's stash (default)
        send_bond_message(contract_id, 100 * UNITS, None);
        run_to_next_block();
        assert_message_executed();
        assert_staking_events(contract_account_id, 100 * UNITS, EventType::Bonded);

        // Assert the `payee` is set to contract's stash
        let payee = pallet_staking::Pallet::<Test>::payee(contract_account_id);
        assert_eq!(payee, RewardDestination::Stash);

        // Set the `payee` to SIGNER
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(SIGNER),
            contract_id,
            Request::V1(RequestV1::SetPayee {
                payee: RewardAccount::Custom(REWARD_PAYEE.into_origin().into())
            })
            .encode(),
            10_000_000_000,
            0,
            false,
        ));

        run_to_next_block();

        // Assert the `payee` is now set to SIGNER
        let payee = pallet_staking::Pallet::<Test>::payee(contract_account_id);
        assert_eq!(payee, RewardDestination::Account(REWARD_PAYEE));
    });
}

#[test]
fn rebond_works() {
    init_logger();

    new_test_ext().execute_with(|| {
        let contract_id = ProgramId::generate_from_user(CodeId::generate(WASM_BINARY), b"contract");
        let contract_account_id = AccountIdOf::<Test>::from_origin(contract_id.into_origin());

        deploy_contract();
        run_to_next_block();

        send_bond_message(contract_id, 500 * UNITS, None);
        run_to_next_block();
        assert_message_executed();
        assert_staking_events(contract_account_id, 500 * UNITS, EventType::Bonded);

        let contract_account_data = System::account(contract_account_id).data;

        // Locked 500 * UNITS as bonded on contracts's account
        assert_eq!(contract_account_data.misc_frozen, 500 * UNITS);

        System::reset_events();

        // Sending `unbond` message
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(SIGNER),
            contract_id,
            Request::V1(RequestV1::Unbond { value: 400 * UNITS }).encode(),
            10_000_000_000,
            0,
            false,
        ));

        run_to_next_block();
        assert_message_executed();
        assert_staking_events(contract_account_id, 400 * UNITS, EventType::Unbonded);

        // All the bonded funds are still locked
        assert_eq!(
            System::account(contract_account_id).data.misc_frozen,
            500 * UNITS
        );

        // However, the ledger has been updated
        let ledger = pallet_staking::Pallet::<Test>::ledger(contract_account_id).unwrap();
        assert_eq!(ledger.active, 100 * UNITS);
        assert_eq!(ledger.unlocking.len(), 1);

        // Sending `rebond` message
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(SIGNER),
            contract_id,
            Request::V1(RequestV1::Rebond { value: 200 * UNITS }).encode(),
            10_000_000_000,
            0,
            false,
        ));

        run_to_next_block();

        // All the bonded funds are still locked
        assert_eq!(
            System::account(contract_account_id).data.misc_frozen,
            500 * UNITS
        );

        // However, the ledger has been updated again
        let ledger = pallet_staking::Pallet::<Test>::ledger(contract_account_id).unwrap();
        assert_eq!(ledger.active, 300 * UNITS);
        assert_eq!(ledger.unlocking.len(), 1);

        // Sending another `rebond` message, with `value` exceeding the unlocking amount
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(SIGNER),
            contract_id,
            Request::V1(RequestV1::Rebond { value: 300 * UNITS }).encode(),
            10_000_000_000,
            0,
            false,
        ));

        run_to_next_block();

        // All the bonded funds are still locked
        assert_eq!(
            System::account(contract_account_id).data.misc_frozen,
            500 * UNITS
        );

        // The ledger has been updated again, however, the rebonded amount was limited
        // by the actual unlocking amount - not the `value` sent in the message.
        let ledger = pallet_staking::Pallet::<Test>::ledger(contract_account_id).unwrap();
        assert_eq!(ledger.active, 500 * UNITS);
        assert_eq!(ledger.unlocking.len(), 0);
    });
}

#[test]
fn payout_stakers_works() {
    init_logger();

    new_test_ext().execute_with(|| {
        let contract_id = ProgramId::generate_from_user(CodeId::generate(WASM_BINARY), b"contract");
        let contract_account_id = AccountIdOf::<Test>::from_origin(contract_id.into_origin());

        deploy_contract();
        run_to_next_block();

        // Only nominating one target
        let targets: Vec<[u8; 32]> = vec![VAL_1_STASH.into_origin().into()];

        send_bond_message(
            contract_id,
            800 * UNITS,
            Some(RewardAccount::Custom(REWARD_PAYEE.into_origin().into())),
        );
        run_to_next_block();
        assert_message_executed();
        assert_staking_events(contract_account_id, 800 * UNITS, EventType::Bonded);

        // Nomintate the validator
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(SIGNER),
            contract_id,
            Request::V1(RequestV1::Nominate {
                targets: targets.clone()
            })
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

        // Run the chain for a few eras (5) to accumulate some rewards
        run_for_n_blocks(
            5 * SESSION_DURATION
                * <Test as pallet_staking::Config>::SessionsPerEra::get() as u64
                * <Test as pallet_staking::Config>::BondingDuration::get() as u64,
        );

        // Send `payout_stakers` message for an era for which the rewards should have been earned
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(SIGNER),
            contract_id,
            Request::V1(RequestV1::PayoutStakers {
                validator_stash: VAL_1_STASH.into_origin().into(),
                era: 4
            })
            .encode(),
            10_000_000_000,
            0,
            false,
        ));

        run_to_next_block();
    });
}
