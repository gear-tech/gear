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
		common::set_program(
			H256::from_low_u64_be(1),
			Program {
				static_pages: Vec::new(),
				// just puts empty message in the log
				code: parse_wat(wat),
			}
		);

		common::queue_message(
			Message {
				source: H256::zero(),
				dest: H256::from_low_u64_be(1),
				payload: Vec::new(),
				gas_limit: u64::max_value(),
			}
		);

		crate::Call::<Test>::process_queue();

		assert_eq!(
			System::events(),
			vec![],
		)
	})
}
