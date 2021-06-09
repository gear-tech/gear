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

use crate::mock::*;
use sp_core::H256;
use common::{self, Program, Message};

pub(crate) fn init_logger() {
	let _ = env_logger::Builder::from_default_env()
		.format_module_path(false)
		.format_level(true)
		.try_init();
}

fn parse_wat(source: &str) -> Vec<u8> {
	let module_bytes = wabt::Wat2Wasm::new()
		.validate(false)
		.convert(source)
		.expect("failed to parse module")
		.as_ref()
		.to_vec();
	module_bytes
}

#[test]
fn it_processes_messages() {

	// just sends empty message to log
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
	  )"#;

	init_logger();
	new_test_ext().execute_with(|| {
		let code = parse_wat(wat);
		let code_hash: H256 = sp_io::hashing::blake2_256(&code[..]).into();
		common::set_code(code_hash, &code);
		common::set_program(
			H256::from_low_u64_be(1),
			Program {
				static_pages: Vec::new(),
				// just puts empty message in the log
				code_hash,
			}
		);

		common::queue_message(
			Message {
				source: H256::zero(),
				dest: H256::from_low_u64_be(1),
				payload: Vec::new(),
				gas_limit: u64::max_value(),
				value: 0,
			},
			H256::default()
		);

		crate::Call::<Test>::process_queue();

		assert_eq!(
			System::events(),
			vec![],
		)
	})
}
