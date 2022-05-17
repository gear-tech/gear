// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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
pub enum SendMessages {
    Init(u64),
}

#[cfg(not(feature = "std"))]
mod wasm {
    use gstd::{debug, msg, prog, CodeHash};

    use super::SendMessages;

    static mut COUNTER: i32 = 0;

    #[allow(unused)]
    const CHILD_CODE_HASH: [u8; 32] =
        hex_literal::hex!("abf3746e72a6e8740bd9e12b879fbdd59e052cb390f116454e9116c22021ae4a");

    #[no_mangle]
    pub unsafe extern "C" fn init() {
        match msg::load().expect("provided invalid payload") {
            SendMessages::Init(value) => {
                let submitted_code = CHILD_CODE_HASH.into();
                let _ = prog::create_program_with_gas(
                    submitted_code,
                    COUNTER.to_le_bytes(),
                    [],
                    100_000,
                    value,
                );

                COUNTER += 1;
            }
        }
    }

    // #[no_mangle]
    // pub unsafe extern "C" fn handle() {
    //     match msg::load().expect("provided invalid payload") {
    //         CreateProgram::Default => {
                // let submitted_code = CHILD_CODE_HASH.into();
                // let new_program_id = prog::create_program_with_gas(
                //     submitted_code,
                //     COUNTER.to_le_bytes(),
                //     [],
                //     100_000,
                //     0,
                // );
    //             msg::send_with_gas(new_program_id, b"", 100_001, 0).unwrap();

    //             COUNTER += 1;
    //         }
    //         CreateProgram::Custom(custom_child_data) => {
    //             for (code_hash, salt, gas_limit) in custom_child_data {
    //                 let submitted_code = code_hash.into();
    //                 let new_program_id =
    //                     prog::create_program_with_gas(submitted_code, &salt, [], gas_limit, 0);
    //                 let msg_id = msg::send_with_gas(new_program_id, b"", 100_001, 0).unwrap();
    //             }
    //         }
    //     };
    // }
}