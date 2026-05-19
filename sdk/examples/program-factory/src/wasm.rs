// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! An example of `create_program_with_gas` syscall.
//!
//! The program is mainly used for testing the syscall logic in pallet `gear` tests.
//! It works as a program factory: depending on input type it sends program creation
//! request (message).

use crate::{CHILD_CODE_HASH, CreateProgram};
use gstd::{ActorId, msg, prog};

static mut COUNTER: i32 = 0;
static mut ORIGIN: Option<ActorId> = None;

#[unsafe(no_mangle)]
extern "C" fn init() {
    unsafe { ORIGIN = Some(msg::source()) };
}

#[unsafe(no_mangle)]
extern "C" fn handle() {
    match msg::load().expect("provided invalid payload") {
        CreateProgram::Default => {
            let submitted_code = CHILD_CODE_HASH.into();
            let (_message_id, new_program_id) = prog::create_program_bytes_with_gas(
                submitted_code,
                unsafe { COUNTER.to_le_bytes() },
                [],
                10_000_000_000,
                0,
            )
            .unwrap();
            msg::send_bytes(new_program_id, [], 0).unwrap();

            unsafe { COUNTER += 1 };
        }
        CreateProgram::Custom(custom_child_data) => {
            for (code_hash, salt, gas_limit) in custom_child_data {
                let submitted_code = code_hash.into();
                let (_message_id, new_program_id) =
                    prog::create_program_bytes_with_gas(submitted_code, &salt, [], gas_limit, 0)
                        .unwrap();
                msg::send_bytes(new_program_id, [], 0).expect("Failed to send message");
            }
        }
    };
}

#[unsafe(no_mangle)]
extern "C" fn handle_reply() {
    if !msg::reply_code().unwrap().is_success() {
        let origin = unsafe { ORIGIN.unwrap() };
        msg::send_bytes(origin, [], 0).unwrap();
    }
}
