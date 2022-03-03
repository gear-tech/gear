//! A simple example of `create_program` sys-call.
//!
//! The program is mainly used for testing the sys-call logic in pallet `gear` tests.

#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode};
#[cfg(not(feature = "std"))]
use gstd::prelude::*;

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

#[derive(Debug, Clone, Encode, Decode, PartialEq, Eq)]
pub enum CreateProgram {
    Default,
    // code hash, salt, gas limit
    Custom(Vec<([u8; 32], Vec<u8>, u64)>),
}

#[allow(unused)]
const CHILD_CODE_HASH: [u8; 32] = hex_literal::hex!(
    "abf3746e72a6e8740bd9e12b879fbdd59e052cb390f116454e9116c22021ae4a"
);

#[cfg(not(feature = "std"))]
mod wasm {
    use gstd::{debug, msg, prog, CodeHash};

    use super::{CreateProgram, CHILD_CODE_HASH};

    static mut COUNTER: i32 = 0;

    #[no_mangle]
    pub unsafe extern "C" fn handle() {
        match msg::load().expect("provided invalid payload") {
            CreateProgram::Default => {
                let submitted_code = CHILD_CODE_HASH.into();
                let new_program_id = prog::create_program_with_gas(
                    submitted_code,
                    COUNTER.to_le_bytes(),
                    [],
                    100_000,
                    0,
                );
                msg::send_with_gas(new_program_id, b"", 100_001, 0);

                COUNTER += 1;
            }
            CreateProgram::Custom(custom_child_data) => {
                for (code_hash, salt, gas_limit) in custom_child_data {
                    let submitted_code = code_hash.into();
                    let new_program_id =
                        prog::create_program_with_gas(submitted_code, &salt, [], gas_limit, 0);
                    let msg_id = msg::send_with_gas(new_program_id, b"", 100_001, 0);
                }
            }
        };
    }
}

#[cfg(test)]
mod tests {
    use std::panic::catch_unwind;

    use gtest::{System, Program, Log};
    use gear_core::message::{Payload, Message, MessageId};

    use super::*;

    #[test]
    fn test_create_program_miscellaneous() {
        env_logger::init();
        let sys = System::new();

        // Store child
        let code_hash_stored = sys.submit_code("./child_contract.wasm");
        assert_eq!(code_hash_stored.inner(), CHILD_CODE_HASH);
        let first_call_salt = 0i32.to_le_bytes();
        let new_actor_id_expected = Program::calculate_program_id(code_hash_stored, &first_call_salt);

        // Create factory
        let factory = Program::current_with_id(&sys, 100);
        // init function
        let res = factory.send_bytes(10001, "EMPTY");
        assert!(!res.main_failed());
        assert_eq!(res.initialized_programs().len(), 1);

        let payload = CreateProgram::Default;

        // handle function
        let res = factory.send_bytes(10001, payload.encode());
        assert!(!res.main_failed());
        assert!(!res.others_failed());
        assert_eq!(res.initialized_programs().len(), 2);

        let (new_actor_id_actual, new_actor_code_hash) = res.initialized_programs().last().copied().unwrap();
        assert_eq!(new_actor_id_expected, new_actor_id_actual);
        assert_eq!(Some(code_hash_stored), new_actor_code_hash);

        let child_program = sys.get_program(new_actor_id_expected);

        // child is alive
        let res = child_program.send_bytes(10001, "EMPTY");
        assert!(!res.main_failed());
        assert!(!res.others_failed());
        
        // duplicate
        let payload = CreateProgram::Custom(vec![(CHILD_CODE_HASH, first_call_salt.to_vec(), 100_000)]);
        let res = factory.send_bytes(10001, payload.encode());
        assert!(!res.main_failed());
        assert!(!res.others_failed());
        // No new programs!
        assert_eq!(res.initialized_programs().len(), 2);

        // non existing code hash provided
        let non_existing_code_hash = [10u8; 32];
        let salt = b"some_salt";
        let fictional_program_id = Program::calculate_program_id(non_existing_code_hash.into(), salt);
        let payload = CreateProgram::Custom(
            vec![(non_existing_code_hash, salt.to_vec(), 100_000)]
        );
        let res = factory.send_bytes(10001, payload.encode());
        assert!(!res.main_failed());
        // No new programs!
        assert_eq!(res.initialized_programs().len(), 2);
        assert!(!res.initialized_programs().iter().any(|(p_id, _)| p_id == &fictional_program_id));
    }  
}
