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

fn assert_bonding_events(contract_id: impl Origin + Copy, bonded: BalanceOf<Test>) {
    assert!(System::events().into_iter().any(|e| {
        match e.event {
            RuntimeEvent::GearBuiltinActor(Event::MessageExecuted { result }) => result.is_ok(),
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
            StakingMessage::Bond { value: 100 * UNITS }.encode(),
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
        assert_bonding_events(SIGNER, 100 * UNITS);
    });
}

#[test]
fn bonding_works() {
    init_logger();

    new_test_ext().execute_with(|| {
        let contract_id = ProgramId::generate_from_user(CodeId::generate(WASM_BINARY), b"contract");
        let contract_account_id = AccountIdOf::<Test>::from_origin(contract_id.into_origin());

        let _builtin_actor_id = pallet::Pallet::<Test>::staking_proxy_actor_id();

        deploy_contract();
        run_to_next_block();

        let signer_current_balance_at_blk_1 = Balances::free_balance(SIGNER);

        // Measure necessary gas in a transaction
        let gas_info = |bonded: u128, value: Option<u128>| {
            start_transaction();
            let res = Gear::calculate_gas_info(
                SIGNER.into_origin(),
                pallet_gear::manager::HandleKind::Handle(contract_id),
                StakingMessage::Bond { value: bonded }.encode(),
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
        send_bond_message(contract_id, 100 * UNITS);
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
        assert_bonding_events(contract_id, 100 * UNITS);

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
            StakingMessage::Bond { value: 50 * UNITS }.encode(),
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
        assert_bonding_events(contract_id, 50 * UNITS);
    });
}

#[test]
fn unbonding_works() {
    init_logger();

    new_test_ext().execute_with(|| {
        let contract_id = ProgramId::generate_from_user(CodeId::generate(WASM_BINARY), b"contract");
        let _builtin_actor_id = pallet::Pallet::<Test>::staking_proxy_actor_id();

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
                RuntimeEvent::GearBuiltinActor(Event::MessageExecuted { result }) => result.is_ok(),
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

    new_test_ext().execute_with(|| {
        let contract_id = ProgramId::generate_from_user(CodeId::generate(WASM_BINARY), b"contract");
        let _builtin_actor_id = pallet::Pallet::<Test>::staking_proxy_actor_id();
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
