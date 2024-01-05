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

//! Builtin actor pallet tests.

#![cfg(test)]

use crate::mock::*;
use common::Origin;
use demo_waiting_proxy::WASM_BINARY;
use frame_support::assert_ok;
use gear_core::ids::{CodeId, ProgramId};
use gear_core_errors::{ErrorReplyReason, ReplyCode, SimpleExecutionError};
use parity_scale_codec::Encode;
use primitive_types::H256;

pub(crate) fn init_logger() {
    let _ = env_logger::Builder::from_default_env()
        .format_module_path(false)
        .format_level(true)
        .try_init();
}

const SUCCESS_ACTOR_ID: [u8; 32] =
    hex_literal::hex!("1f81dd2c95c0006c335530c3f1b32d8b1314e08bc940ea26afdbe2af88b0400d");
const ERROR_ACTOR_ID: [u8; 32] =
    hex_literal::hex!("983ebefef8810a41a6a8d9dafa6d8d1016841d04e57f4a3ff87a9053a8616cf8");

fn deploy_contract(init_payload: Vec<u8>) {
    assert_ok!(Gear::upload_program(
        RuntimeOrigin::signed(SIGNER),
        WASM_BINARY.to_vec(),
        b"salt".to_vec(),
        init_payload,
        10_000_000_000,
        EXISTENTIAL_DEPOSIT, // keep the contract's account "providing"
        false,
    ));
}

fn send_message(contract_id: ProgramId, payload: Vec<u8>) {
    assert_ok!(Gear::send_message(
        RuntimeOrigin::signed(SIGNER),
        contract_id,
        payload,
        10_000_000_000,
        0,
        false,
    ));
}

#[test]
fn user_message_to_builtin_actor_works() {
    init_logger();

    new_test_ext().execute_with(|| {
        let builtin_actor_id: ProgramId = H256::from(SUCCESS_ACTOR_ID).cast();

        assert_eq!(current_stack(), vec![]);

        // Asserting success
        send_message(builtin_actor_id, Default::default());
        run_to_next_block();

        // A builtin contract has been called
        assert_eq!(current_stack().len(), 1);
        assert!(current_stack()[0].is_success);

        // Asserting error
        let builtin_actor_id: ProgramId = H256::from(ERROR_ACTOR_ID).cast();
        send_message(builtin_actor_id, Default::default());
        run_to_next_block();

        // A builtin contract has been called
        assert_eq!(current_stack().len(), 2);
        assert!(!current_stack()[1].is_success);
    });
}

#[test]
fn invoking_builtin_from_program_works() {
    init_logger();

    new_test_ext().execute_with(|| {
        let contract_id = ProgramId::generate_from_user(CodeId::generate(WASM_BINARY), b"salt");

        assert_eq!(current_stack(), vec![]);

        deploy_contract((SUCCESS_ACTOR_ID, 0u64).encode());
        run_to_next_block();

        let signer_current_balance_at_blk_1 = Balances::free_balance(SIGNER);

        // Measure necessary gas in a transaction
        let gas_info = |payload: Vec<u8>| {
            start_transaction();
            let res = Gear::calculate_gas_info(
                SIGNER.into_origin(),
                pallet_gear::manager::HandleKind::Handle(contract_id),
                payload,
                0,
                true,
                None,
                None,
            )
            .expect("calculate_gas_info failed");
            rollback_transaction();
            res
        };
        let gas_burned = gas_info(Default::default()).burned;

        send_message(contract_id, Default::default());
        run_to_next_block();

        let signer_current_balance_at_blk_2 = Balances::free_balance(SIGNER);

        // SIGNER has spent in current block:
        // - paid for the burned gas
        assert_eq!(
            signer_current_balance_at_blk_2,
            signer_current_balance_at_blk_1 - gas_price(gas_burned)
        );

        // Assert builtin contract invocation
        assert_eq!(current_stack().len(), 1);
        assert!(current_stack()[0].is_success);
    });
}

#[test]
fn calculate_gas_info_may_not_work() {
    init_logger();

    new_test_ext().execute_with(|| {
        let builtin_actor_id: ProgramId = H256::from(SUCCESS_ACTOR_ID).cast();

        assert_eq!(current_stack(), vec![]);

        // Estimate gas a call would take
        let gas_info = |payload: Vec<u8>| {
            start_transaction();
            let res = Gear::calculate_gas_info(
                SIGNER.into_origin(),
                pallet_gear::manager::HandleKind::Handle(builtin_actor_id),
                payload,
                0,
                true,
                None,
                None,
            )
            .expect("calculate_gas_info failed");
            rollback_transaction();
            res
        };
        let gas_burned = gas_info(Default::default()).burned;

        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(SIGNER),
            builtin_actor_id,
            Default::default(),
            gas_burned + 1000,
            0,
            false,
        ));
        run_to_next_block();

        // We expect the builtin actor's `handle()` method not have been called due to
        // insufficient gas, because for builtin actors we require the maximum possible
        // gas a message handling can incur to be provided with a message.
        assert_eq!(current_stack().len(), 0);

        // Expecting an error reply to have been sent.
        assert!(System::events().into_iter().any(|e| match e.event {
            RuntimeEvent::Gear(pallet_gear::Event::<Test>::UserMessageSent { message, .. }) => {
                message.destination() == ProgramId::from(SIGNER) && message.details().is_some() && {
                    let details = message.details().expect("Value checked above");
                    details.to_reply_code()
                        == ReplyCode::Error(ErrorReplyReason::Execution(
                            SimpleExecutionError::RanOutOfGas,
                        ))
                }
            }
            _ => false,
        }));
    });
}
