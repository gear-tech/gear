// This file is part of Gear.

// Copyright (C) 2021 Gear Technologies Inc.
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

use super::*;
use crate::mock::*;
use codec::Encode;
use common::{self, IntermediateMessage, MessageOrigin, MessageRoute, Origin as _};
use frame_support::{assert_noop, assert_ok};
use frame_system::RawOrigin;
use sp_core::H256;

pub(crate) fn init_logger() {
	let _ = env_logger::Builder::from_default_env()
		.format_module_path(false)
		.format_level(true)
		.try_init();
}

fn parse_wat(source: &str) -> Vec<u8> {
	wabt::Wat2Wasm::new()
		.validate(false)
		.convert(source)
		.expect("failed to parse module")
		.as_ref()
		.to_vec()
}

#[test]
fn submit_program_enqueues_message() {
	let wat = r#"
	(module
	)"#;

	init_logger();
	new_test_ext().execute_with(|| {
		let code = parse_wat(wat);

		assert_ok!(Pallet::<Test>::submit_program(
			Origin::signed(1).into(),
			code.clone(),
			b"salt".to_vec(),
			Vec::new(),
			10_000u64,
			0_u128
		));

		let messages: Vec<IntermediateMessage> = MessageQueue::<Test>::get().unwrap();
		assert_eq!(messages.len(), 1);

		let (msg_origin, msg_code, _id) = match &messages[0] {
			IntermediateMessage::InitProgram {
				external_origin,
				code,
				program_id,
				..
			} => (*external_origin, code.to_vec(), *program_id),
			_ => (Default::default(), Vec::new(), Default::default()),
		};
		assert_eq!(msg_origin, 1_u64.into_origin());
		assert_eq!(msg_code, code);

		assert_eq!(System::events(), vec![]);
	})
}

#[test]
fn submit_program_fails_with_insufficient_balance() {
    let wat = r#"
	(module
	)"#;

	init_logger();
	new_test_ext().execute_with(|| {
		let code = parse_wat(wat);

		assert_noop!(
			Pallet::<Test>::submit_program(
				Origin::signed(2),
				code.clone(),
				b"salt".to_vec(),
				Vec::new(),
				10_000u64,
				10_u128
			),
			Error::<Test>::NotEnoughBalanceForReserve
		);
	})
}

#[test]
fn send_message_adds_to_queue() {
	init_logger();
	new_test_ext().execute_with(|| {
		assert_ok!(Pallet::<Test>::send_message(
			Origin::signed(1).into(),
			H256::from_low_u64_be(255),
			b"payload".to_vec(),
			10_000u64,
			0_u128
		));

		let messages: Vec<IntermediateMessage> = MessageQueue::<Test>::get().unwrap();
		assert_eq!(messages.len(), 1);

		let mut id = b"payload".to_vec().encode();
		id.extend_from_slice(&0_u128.to_le_bytes());
		let id: H256 = sp_io::hashing::blake2_256(&id).into();

		let msg_id = match &messages[0] {
			IntermediateMessage::DispatchMessage { id, .. } => *id,
			_ => Default::default(),
		};
		assert_eq!(msg_id, id);

		assert_eq!(System::events(), vec![]);
	})
}

#[test]
fn send_message_fails_with_insufficient_balance() {
	init_logger();
	new_test_ext().execute_with(|| {
		assert_noop!(
			Pallet::<Test>::send_message(
				Origin::signed(2).into(),
				H256::from_low_u64_be(255),
				b"payload".to_vec(),
				10_000u64,
				0_u128
			),
			Error::<Test>::NotEnoughBalanceForReserve
		);
	})
}

#[test]
fn messages_processing_works() {
	let wat = r#"
	(module
		(import "env" "send"  (func $send (param i32 i32 i32 i64)))
		(import "env" "memory" (memory 1))
		(export "handle" (func $handle))
		(export "init" (func $init))
		(func $handle
			i32.const 0
			i32.const 32
			i32.const 32
			i64.const 1000000000
			call $send
		)
		(func $init)
	)
	"#;

	init_logger();
	new_test_ext().execute_with(|| {
		let code = parse_wat(wat);
		let program_id = H256::from_low_u64_be(1001);

		// Inject messages to MQ
		let messages = vec![
			IntermediateMessage::InitProgram {
				external_origin: 1.into_origin(),
				code,
				program_id,
				payload: Vec::new(),
				gas_limit: 10000,
				value: 0,
			},
			IntermediateMessage::DispatchMessage {
				id: H256::from_low_u64_be(102),
				route: MessageRoute {
					origin: MessageOrigin::External(1.into_origin()),
					destination: program_id,
				},
				payload: Vec::new(),
				gas_limit: 10000,
				value: 0,
			},
		];
		MessageQueue::<Test>::put(messages);
		assert_eq!(
			MessageQueue::<Test>::get()
				.expect("Failed to get messages from queue")
				.len(),
			2
		);

		let none_origin: <Test as frame_system::Config>::Origin = RawOrigin::None.into();
		crate::Pallet::<Test>::process_queue(none_origin).expect("Failed to process queue");

		assert_eq!(System::events(), vec![]);
	})
}
