// This file is part of Gear.

// Copyright (C) 2025 Gear Technologies Inc.
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

#![cfg(test)]

use crate::{
    BuiltinActorError, BuiltinContext,
    mock::{
        DUMMY_ACTUAL_WEIGHT, DUMMY_CALL_ACTOR_ID, DUMMY_DECLARED_WEIGHT, Gear, GearBuiltin,
        RuntimeOrigin, SIGNER, new_test_ext, rollback_transaction, start_transaction,
    },
};
use common::Origin;
use frame_support::assert_ok;
use gear_core::{
    gas::{GasAllowanceCounter, GasCounter},
    ids::ActorId,
};
use pallet_gear::manager::HandleKind;

#[test]
fn calculate_gas_info_keeps_precharged_weight_for_builtin_dispatch_call() {
    new_test_ext().execute_with(|| {
        let builtin_actor: ActorId = GearBuiltin::generate_actor_id(DUMMY_CALL_ACTOR_ID);

        let gas_info = {
            start_transaction();
            let res = Gear::calculate_gas_info(
                SIGNER.into_origin(),
                HandleKind::Handle(builtin_actor),
                Vec::new(),
                0,
                true,
                None,
                None,
            )
            .expect("calculate_gas_info failed");
            rollback_transaction();
            res
        };

        assert_eq!(gas_info.burned, DUMMY_ACTUAL_WEIGHT);
        assert_eq!(gas_info.min_limit, DUMMY_DECLARED_WEIGHT);

        // Sanity check: providing the returned limit should allow the message to execute.
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(SIGNER),
            builtin_actor,
            Vec::new(),
            gas_info.min_limit,
            0,
            false,
        ));
    });
}

#[test]
fn precharge_reserves_highest_requested_weight() {
    let initial_gas = 120_u64;
    let mut context = BuiltinContext {
        gas_counter: GasCounter::new(initial_gas),
        gas_allowance_counter: GasAllowanceCounter::new(initial_gas),
        pending_precharges: Vec::new(),
        max_precharge: 0,
    };

    let first_required = 80_u64;
    let first_actual = 40_u64;

    context
        .can_charge_gas(first_required)
        .expect("precharge should succeed");
    context
        .try_charge_gas(first_actual)
        .expect("charging actual gas should succeed");

    assert_eq!(
        context.gas_counter.left(),
        initial_gas - first_required,
        "remaining gas must reflect the largest pre-validated amount"
    );

    let second_required = 70_u64;
    let err = context
        .can_charge_gas(second_required)
        .expect_err("second precharge must fail once the limit is exhausted");
    assert!(matches!(err, BuiltinActorError::InsufficientGas));
}
