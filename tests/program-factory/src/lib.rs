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

#[cfg(not(feature = "std"))]
mod wasm {
    use gstd::{debug, msg, prog, CodeHash};

    use super::CreateProgram;

    static mut COUNTER: i32 = 0;

    fn increase() {
        unsafe {
            COUNTER += 1;
        }
    }

    fn get() -> i32 {
        unsafe { COUNTER }
    }

    #[no_mangle]
    pub unsafe extern "C" fn handle() {
        match msg::load().expect("provided invalid payload") {
            CreateProgram::Default => {
                let submitted_code = hex_literal::hex!(
                    "abf3746e72a6e8740bd9e12b879fbdd59e052cb390f116454e9116c22021ae4a"
                )
                .into();
                let new_program_id = prog::create_program_with_gas(
                    submitted_code,
                    get().to_le_bytes(),
                    [],
                    100_000,
                    0,
                );
                msg::send_with_gas(new_program_id, b"", 100_001, 0);

                increase();
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
