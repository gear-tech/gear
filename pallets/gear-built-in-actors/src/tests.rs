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
use frame_support::{assert_noop, assert_ok};
use gear_built_in_actor_common::staking::*;
use gear_core::ids::{CodeId, ProgramId};

type AccountIdOf<T> = <T as frame_system::Config>::AccountId;

macro_rules! assert_approx_eq {
    ($left:expr, $right:expr, $tol:expr) => {{
        assert!(
            $left <= $right + $tol && $right <= $left + $tol,
            "{} != {} with tolerance {}",
            $left,
            $right,
            $tol
        );
    }};
}

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

fn send_bond_message(contract_id: ProgramId, amount: BalanceOf<Test>) {
    assert_ok!(Gear::send_message(
        RuntimeOrigin::signed(SIGNER),
        contract_id,
        StakingMessage::Bond { value: amount }.encode(),
        10_000_000_000,
        amount,
        false,
    ));
}

fn assert_bonding_events(contract_id: ProgramId, bonded: BalanceOf<Test>) {
    assert!(System::events().into_iter().any(|e| {
        match e.event {
            RuntimeEvent::GearBuiltInActor(Event::MessageExecuted { result }) => result.is_ok(),
            RuntimeEvent::Staking(pallet_staking::Event::<Test>::Bonded { stash, amount }) => {
                stash.into_origin() == contract_id.into_origin() && bonded == amount
            }
            _ => false,
        }
    }))
}

fn assert_execution_with_error() {
    assert!(System::events().into_iter().any(|e| {
        match e.event {
            RuntimeEvent::GearBuiltInActor(Event::MessageExecuted { result }) => result.is_err(),
            _ => false,
        }
    }))
}

#[test]
fn user_messages_to_built_in_actor_yield_error() {
    init_logger();

    let bank_address = <Test as pallet_gear_bank::Config>::BankAddress::get();

    ExtBuilder::default()
        .initial_authorities(vec![
            (VAL_1_STASH, VAL_1_CONTROLLER, VAL_1_AUTH_ID),
            (VAL_2_STASH, VAL_2_CONTROLLER, VAL_2_AUTH_ID),
            (VAL_3_STASH, VAL_3_CONTROLLER, VAL_3_AUTH_ID),
        ])
        .stash(VALIDATOR_STAKE)
        .endowment(ENDOWMENT)
        .endowed_accounts(vec![bank_address, SIGNER])
        .total_supply(INITIAL_TOTAL_TOKEN_SUPPLY)
        .build()
        .execute_with(|| {
            let assert_issuance =
                |balance: BalanceOf<Test>| assert_eq!(Balances::total_issuance(), balance);

            // Asserting initial parameters.
            assert_issuance(INITIAL_TOTAL_TOKEN_SUPPLY);

            let built_in_actor_id = pallet::Pallet::<Test>::staking_proxy_actor_id();

            // Asserting bad destination.
            assert_noop!(
                Gear::send_message(
                    RuntimeOrigin::signed(SIGNER),
                    built_in_actor_id,
                    vec![],
                    10_000_000_000,
                    0,
                    false,
                ),
                pallet_gear::Error::<Test>::IllegalDestination
            );
        });
}

#[test]
fn bonding_works() {
    init_logger();

    let bank_address = <Test as pallet_gear_bank::Config>::BankAddress::get();

    ExtBuilder::default()
        .initial_authorities(vec![
            (VAL_1_STASH, VAL_1_CONTROLLER, VAL_1_AUTH_ID),
            (VAL_2_STASH, VAL_2_CONTROLLER, VAL_2_AUTH_ID),
            (VAL_3_STASH, VAL_3_CONTROLLER, VAL_3_AUTH_ID),
        ])
        .stash(VALIDATOR_STAKE)
        .endowment(ENDOWMENT)
        .endowed_accounts(vec![bank_address, SIGNER])
        .build()
        .execute_with(|| {
            let contract_id =
                ProgramId::generate_from_user(CodeId::generate(WASM_BINARY), b"contract");
            let contract_account_id = AccountIdOf::<Test>::from_origin(contract_id.into_origin());

            let _built_in_actor_id = pallet::Pallet::<Test>::staking_proxy_actor_id();

            deploy_contract();
            run_to_next_block();

            // Measure necessary gas in a transaction
            start_transaction();
            let gas_info = Gear::calculate_gas_info(
                SIGNER.into_origin(),
                pallet_gear::manager::HandleKind::Handle(contract_id),
                StakingMessage::Bond { value: 100 * UNITS }.encode(),
                100 * UNITS,
                true,
                None,
                None,
            )
            .expect("calculate_gas_info failed");
            rollback_transaction();

            // Asserting success
            send_bond_message(contract_id, 100 * UNITS);
            run_to_next_block();

            let signer_current_balance = Balances::free_balance(SIGNER);
            let contract_account_data = System::account(contract_account_id).data;

            // SIGNER has spent so far:
            // - EXISTENTIAL_DEPOSIT at the time of the contract deployment
            // - 100 UNITS sent as value to the contract
            // - paid for some burned gas
            // TODO: what's the unacounted amount?
            assert_approx_eq!(
                signer_current_balance,
                ENDOWMENT - EXISTENTIAL_DEPOSIT - 100 * UNITS - gas_price(gas_info.burned),
                UNITS / 10
            );

            // The contract's account has the same 10 * UNITS of free balance (the ED)
            assert_eq!(contract_account_data.free, 110 * UNITS);
            // and 100 * UNITS of it is frozen as bonded
            assert_eq!(contract_account_data.misc_frozen, 100 * UNITS);

            // Asserting the expected events are present
            assert_bonding_events(contract_id, 100 * UNITS);

            System::reset_events();

            // Asserting success again (the contract should be able to figure out that `bond_extra`
            // should be called instead).
            // Note: the actual added amount is limited by the message `value` field, that is
            // it's going to be 50 UNITS, not 100 UNITS as encoded in the message payload.
            assert_ok!(Gear::send_message(
                RuntimeOrigin::signed(SIGNER),
                contract_id,
                StakingMessage::Bond { value: 100 * UNITS }.encode(),
                10_000_000_000,
                50 * UNITS,
                false,
            ));

            run_to_next_block();

            // SIGNER has spent since last time:
            // - 50 UNITS sent as value to the contract
            // - paid for gas
            // TODO: what's the unacounted amount?
            assert_approx_eq!(
                Balances::free_balance(SIGNER),
                signer_current_balance - 50 * UNITS - gas_price(gas_info.burned),
                UNITS / 10
            );
            // Another 50 * UNITS added to locked balance
            assert_eq!(
                System::account(contract_account_id).data.misc_frozen,
                150 * UNITS
            );

            // Asserting the expected events are present
            assert_bonding_events(contract_id, 50 * UNITS);
        });
}

#[test]
fn unbonding_works() {
    init_logger();

    let bank_address = <Test as pallet_gear_bank::Config>::BankAddress::get();

    ExtBuilder::default()
        .initial_authorities(vec![
            (VAL_1_STASH, VAL_1_CONTROLLER, VAL_1_AUTH_ID),
            (VAL_2_STASH, VAL_2_CONTROLLER, VAL_2_AUTH_ID),
            (VAL_3_STASH, VAL_3_CONTROLLER, VAL_3_AUTH_ID),
        ])
        .stash(VALIDATOR_STAKE)
        .endowment(ENDOWMENT)
        .endowed_accounts(vec![bank_address, SIGNER])
        .build()
        .execute_with(|| {
            let contract_id =
                ProgramId::generate_from_user(CodeId::generate(WASM_BINARY), b"contract");
            let _built_in_actor_id = pallet::Pallet::<Test>::staking_proxy_actor_id();

            deploy_contract();
            run_to_next_block();

            send_bond_message(contract_id, 100 * UNITS);
            run_to_next_block();

            // Asserting the expected events are present
            assert_bonding_events(contract_id, 100 * UNITS);

            System::reset_events();

            // Measure necessary gas in a transaction for `unbond` message
            start_transaction();
            let _gas_info = Gear::calculate_gas_info(
                SIGNER.into_origin(),
                pallet_gear::manager::HandleKind::Handle(contract_id),
                StakingMessage::Unbond { value: 200 * UNITS }.encode(),
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
                StakingMessage::Unbond { value: 200 * UNITS }.encode(),
                10_000_000_000,
                0,
                false,
            ));

            run_to_next_block();

            // Asserting the expected events are present
            assert!(System::events().into_iter().any(|e| {
                match e.event {
                    RuntimeEvent::GearBuiltInActor(Event::MessageExecuted { result }) => {
                        result.is_ok()
                    }
                    RuntimeEvent::Staking(pallet_staking::Event::<Test>::Unbonded {
                        stash,
                        amount,
                    }) => stash.into_origin() == contract_id.into_origin() && amount == 100 * UNITS,
                    _ => false,
                }
            }));
        });
}

#[test]
fn nominating_works() {
    init_logger();

    let bank_address = <Test as pallet_gear_bank::Config>::BankAddress::get();

    ExtBuilder::default()
        .initial_authorities(vec![
            (VAL_1_STASH, VAL_1_CONTROLLER, VAL_1_AUTH_ID),
            (VAL_2_STASH, VAL_2_CONTROLLER, VAL_2_AUTH_ID),
            (VAL_3_STASH, VAL_3_CONTROLLER, VAL_3_AUTH_ID),
        ])
        .stash(VALIDATOR_STAKE)
        .endowment(ENDOWMENT)
        .endowed_accounts(vec![bank_address, SIGNER])
        .build()
        .execute_with(|| {
            let contract_id =
                ProgramId::generate_from_user(CodeId::generate(WASM_BINARY), b"contract");
            let _built_in_actor_id = pallet::Pallet::<Test>::staking_proxy_actor_id();
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
                StakingMessage::Nominate {
                    targets: targets.clone()
                }
                .encode(),
                10_000_000_000,
                0,
                false,
            ));

            run_to_next_block();
            // Make sure we have an "upstream" error packaged as an event
            assert_execution_with_error();

            System::reset_events();

            send_bond_message(contract_id, 100 * UNITS);
            run_to_next_block();
            assert_bonding_events(contract_id, 100 * UNITS);

            let targets_before = pallet_staking::Nominators::<Test>::get(contract_account_id)
                .map_or_else(Vec::new, |x| x.targets.into_inner());
            assert_eq!(targets_before.len(), 0);

            // Now expecting nominating to work
            assert_ok!(Gear::send_message(
                RuntimeOrigin::signed(SIGNER),
                contract_id,
                StakingMessage::Nominate {
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
