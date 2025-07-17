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

//! Builtin actor pallet tests.

#![cfg(test)]

use crate::mock::*;
use common::Origin;
use demo_waiting_proxy::WASM_BINARY;
use frame_support::assert_ok;
use gear_core::ids::{ActorId, CodeId, prelude::*};
use gear_core_errors::{ErrorReplyReason, ReplyCode, SimpleExecutionError};
use parity_scale_codec::Encode;
use primitive_types::H256;

pub(crate) fn init_logger() {
    let _ = tracing_subscriber::fmt::try_init();
}

const SUCCESS_ACTOR_ID: [u8; 32] =
    hex_literal::hex!("e9391bc9ccbc52c944f5e8c957d36393b7b96abc144e436731198eb650c5b874");
const ERROR_ACTOR_ID: [u8; 32] =
    hex_literal::hex!("eff0acac338350c3ca8adcbbd7be7552f980c28670ab03ff7224d5d5b012e4f9");
const HONEST_ACTOR_ID: [u8; 32] =
    hex_literal::hex!("7eb026f54b3ee698d933fbbbb0b22665acd4b435a7272ead5dc9152c97cbf032");

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

fn send_message(contract_id: ActorId, payload: Vec<u8>) {
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
        let builtin_actor_id: ActorId = H256::from(SUCCESS_ACTOR_ID).cast();

        assert_eq!(current_stack(), vec![]);

        // Asserting success
        send_message(builtin_actor_id, Default::default());

        // Message is in the queue and a gas node has been created.
        assert!(!message_queue_empty());
        assert!(!gas_tree_empty());

        run_to_next_block();

        // A builtin contract has been called
        assert_eq!(current_stack().len(), 1);
        assert!(current_stack()[0].is_success);
        // No more messages in the queue
        assert!(message_queue_empty());
        // No more nodes in gas tree
        assert!(gas_tree_empty());

        // Asserting error
        let builtin_actor_id: ActorId = H256::from(ERROR_ACTOR_ID).cast();
        send_message(builtin_actor_id, Default::default());
        run_to_next_block();

        // A builtin contract has been called
        assert_eq!(current_stack().len(), 2);
        assert!(!current_stack()[1].is_success);
        // No more messages in the queue
        assert!(message_queue_empty());
        // No more nodes in gas tree
        assert!(gas_tree_empty());
    });
}

#[test]
fn invoking_builtin_from_program_works() {
    init_logger();

    new_test_ext().execute_with(|| {
        let contract_id = ActorId::generate_from_user(CodeId::generate(WASM_BINARY), b"salt");

        assert_eq!(current_stack(), vec![]);

        deploy_contract((HONEST_ACTOR_ID, 0u64).encode());
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
        // Message is in the queue and a gas node has been created.
        assert!(!message_queue_empty());
        assert!(!gas_tree_empty());

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
        // No more messages in the queue
        assert!(message_queue_empty());
        // No more nodes in gas tree
        assert!(gas_tree_empty());
    });
}

#[test]
fn calculate_gas_info_works() {
    init_logger();

    new_test_ext().execute_with(|| {
        let builtin_actor_id: ActorId = H256::from(SUCCESS_ACTOR_ID).cast();

        assert_eq!(current_stack(), vec![]);

        // Estimate the amount of gas a call to builtin actor would take.
        let get_gas_info = |builtin_id, payload: Vec<u8>| {
            start_transaction();
            let res = Gear::calculate_gas_info(
                SIGNER.into_origin(),
                pallet_gear::manager::HandleKind::Handle(builtin_id),
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
        let gas_info = get_gas_info(builtin_actor_id, Default::default());

        // Success builtin actor always reports success even if gas is insufficient.
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(SIGNER),
            builtin_actor_id,
            Default::default(),
            gas_info.min_limit - 100,
            0,
            false,
        ));

        // Message is in the queue and a gas node has been created.
        assert!(!message_queue_empty());
        assert!(!gas_tree_empty());

        run_to_next_block();

        assert_eq!(current_stack().len(), 1);
        // Importantly, the gas tree is consistent, even though the validator has done more work
        // than the user paid for by providing less gas.
        assert!(current_stack()[0].is_success);

        // No more messages in the queue and all gas nodes have been consumed.
        assert!(message_queue_empty());
        assert!(gas_tree_empty());

        // Honest actor runs gas limit check and respects its outcome.
        let builtin_actor_id: ActorId = H256::from(HONEST_ACTOR_ID).cast();
        let gas_info = get_gas_info(builtin_actor_id, Default::default());
        assert_ok!(Gear::send_message(
            RuntimeOrigin::signed(SIGNER),
            builtin_actor_id,
            Default::default(),
            gas_info.min_limit - 1_000,
            0,
            false,
        ));
        run_to_next_block();

        assert_eq!(current_stack().len(), 2);
        // Failure is reported.
        assert!(!current_stack()[1].is_success);

        // An error reply should have been sent.
        assert!(System::events().into_iter().any(|e| match e.event {
            RuntimeEvent::Gear(pallet_gear::Event::<Test>::UserMessageSent { message, .. }) => {
                message.destination() == SIGNER.cast() && message.details().is_some() && {
                    let details = message.details().expect("Value checked above");
                    details.to_reply_code()
                        == ReplyCode::Error(ErrorReplyReason::Execution(
                            SimpleExecutionError::RanOutOfGas,
                        ))
                }
            }
            _ => false,
        }));

        // No more messages in the queue
        assert!(message_queue_empty());
        // No more nodes in gas tree
        assert!(gas_tree_empty());
    });
}
