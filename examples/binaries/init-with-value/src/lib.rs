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
use gstd::errors::MessageError;
#[cfg(not(feature = "std"))]
use gstd::prelude::*;

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}
#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

#[derive(Debug, Clone, Encode, Decode, PartialEq, Eq)]
pub enum ExpectedError {
    NotEnoughValue,
    InsufficientValue,
}

impl ExpectedError {
    pub fn into_message_error(self) -> MessageError {
        match self {
            ExpectedError::NotEnoughValue => MessageError::NotEnoughValue {
                message_value: 1001,
                value_left: 1000,
            },
            ExpectedError::InsufficientValue => MessageError::InsufficientValue {
                message_value: 499,
                existential_deposit: 500,
            },
        }
    }
}

#[derive(Debug, Clone, Encode, Decode, PartialEq, Eq)]
pub enum SendMessage {
    Init {
        expected_error: Option<ExpectedError>,
        value: u128,
    },
    Handle {
        custom_destination_id: u64,
        value: u128,
    },
}

#[cfg(not(feature = "std"))]
mod wasm {
    use crate::ExpectedError;
    use gstd::{
        errors::{ContractError, ExtError},
        msg, prog,
    };

    use super::SendMessage;

    static mut COUNTER: i32 = 0;

    #[allow(unused)]
    const CHILD_CODE_HASH: [u8; 32] =
        hex_literal::hex!("abf3746e72a6e8740bd9e12b879fbdd59e052cb390f116454e9116c22021ae4a");

    #[no_mangle]
    pub unsafe extern "C" fn init() {
        let data: gstd::Vec<SendMessage> = msg::load().expect("provided invalid payload");
        for msg_data in data {
            match msg_data {
                SendMessage::Init {
                    value,
                    expected_error,
                } => {
                    let submitted_code = CHILD_CODE_HASH.into();
                    let res = prog::create_program_with_gas(
                        submitted_code,
                        COUNTER.to_le_bytes(),
                        [],
                        1_000_001,
                        value,
                    );

                    match res {
                        Ok(_) => {
                            COUNTER += 1;
                        }
                        Err(err) => {
                            gstd::debug!("Error occurred during program creation: {}", err);
                            if let Some(expected_error) = expected_error {
                                assert_eq!(
                                    err,
                                    ContractError::Ext(ExtError::Message(
                                        expected_error.into_message_error()
                                    ))
                                )
                            }
                        }
                    }
                }
                SendMessage::Handle {
                    custom_destination_id,
                    value,
                } => {
                    let _ = msg::send(custom_destination_id.into(), b"", value);
                }
            }
        }
    }
}
