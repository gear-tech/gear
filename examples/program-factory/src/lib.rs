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

//! An example of `create_program_with_gas` sys-call.
//!
//! The program is mainly used for testing the sys-call logic in pallet `gear` tests.
//! It works as a program factory: depending on input type it sends program creation
//! request (message).

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
use gstd::prelude::*;
use parity_scale_codec::{Decode, Encode};

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
const CHILD_CODE_HASH: [u8; 32] =
    hex_literal::hex!("abf3746e72a6e8740bd9e12b879fbdd59e052cb390f116454e9116c22021ae4a");

#[cfg(not(feature = "std"))]
mod wasm;

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use gtest::{calculate_program_id, Program, System};
    use std::io::Write;

    // Creates a new factory and initializes it.
    fn prepare_factory(sys: &System) -> Program {
        // Store child
        let code_hash_stored = sys.submit_code("./child_contract.wasm");
        assert_eq!(code_hash_stored, CHILD_CODE_HASH.into());

        // Instantiate factory
        let factory = Program::current_with_id(sys, 100);

        // Send `init` msg to factory
        let res = factory.send_bytes(10001, "EMPTY");
        assert!(!res.main_failed());
        assert!(sys.is_active_program(100));

        factory
    }

    fn create_tmp_file_with_data(data: &[u8]) -> std::path::PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push("tmp_test_file");

        let mut file =
            std::fs::File::create(dir.as_path()).expect("internal error: can't create tmp file");
        file.write_all(data)
            .expect("internal error: can't write to tmp");

        dir
    }

    #[test]
    fn test_simple() {
        let sys = System::new();
        sys.init_logger();
        let factory = prepare_factory(&sys);

        // Send `handle` msg to factory to create a new child
        let res = factory.send_bytes(10001, CreateProgram::Default.encode());
        let child_id_expected = calculate_program_id(
            CHILD_CODE_HASH.into(),
            &0i32.to_le_bytes(),
            Some(res.sent_message_id()),
        );
        assert!(!res.main_failed());
        assert!(sys.is_active_program(child_id_expected));
    }

    #[test]
    fn test_duplicate() {
        let sys = System::new();
        sys.init_logger();
        let factory = prepare_factory(&sys);

        let salt = 0i32.to_be_bytes();
        let payload = CreateProgram::Custom(vec![(CHILD_CODE_HASH, salt.to_vec(), 100_000_000)]);

        // Send `handle` msg to factory to create a new child
        let res = factory.send_bytes(10001, payload.encode());

        let child_id_expected =
            calculate_program_id(CHILD_CODE_HASH.into(), &salt, Some(res.sent_message_id()));

        assert!(!res.main_failed());
        assert!(sys.is_active_program(child_id_expected));

        // Send `handle` msg to create a duplicate
        let res = factory.send_bytes(10001, payload.encode());

        let child_id_expected =
            calculate_program_id(CHILD_CODE_HASH.into(), &salt, Some(res.sent_message_id()));

        assert!(!res.main_failed());
        assert!(sys.is_active_program(child_id_expected));

        assert_eq!(res.total_processed(), 3 + 1 + 1); // +1 for the original message, initiated by user +1 for auto generated replies
    }

    #[test]
    fn test_non_existing_code_hash() {
        let sys = System::new();
        sys.init_logger();
        let factory = prepare_factory(&sys);

        // Non existing code hash provided
        let non_existing_code_hash = [10u8; 32];
        let salt = b"some_salt";
        let payload = CreateProgram::Custom(vec![(non_existing_code_hash, salt.to_vec(), 100_000)]);
        let res = factory.send_bytes(10001, payload.encode());
        let fictional_program_id = calculate_program_id(
            non_existing_code_hash.into(),
            salt,
            Some(res.sent_message_id()),
        );
        assert!(!res.main_failed());
        // No new program with fictional id
        assert!(sys.is_active_program(fictional_program_id));
    }

    #[test]
    #[should_panic(expected = "Program can't be constructed with provided code")]
    fn test_invalid_wasm_child() {
        let sys = System::new();
        sys.init_logger();
        let factory = prepare_factory(&sys);

        let invalid_wasm = [10u8; 32];
        let invalid_wasm_path_buf = create_tmp_file_with_data(&invalid_wasm);
        let invalid_wasm_code_hash = sys.submit_code(invalid_wasm_path_buf);

        let payload = CreateProgram::Custom(vec![(
            invalid_wasm_code_hash.into(),
            b"some_salt".to_vec(),
            100_000,
        )]);
        factory.send_bytes(10001, payload.encode());
    }
}
