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

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use codec::{Decode, Encode};

#[cfg(feature = "wasm-wrapper")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

type MessageId = [u8; 32];
type ActorId = [u8; 32];

#[cfg(feature = "wasm-wrapper")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

use alloc::{string::String, vec::Vec};

// Instead of proper gstd primitives we use their raw versions to make this contract
// compilable as a dependency for the build of the `gear` with `runtime-benchmarking` feature.
#[derive(Debug, Encode, Decode)]
pub enum Kind {
    // Params(salt, gas), Expected(message id, actor id)
    CreateProgram(u64, u64, (MessageId, ActorId)),
    // Params(value), Expected(error message)
    Error(u128, String),
    // Params(gas), Expected(message id)
    Send(u64, MessageId),
    // Params(payload, gas), Expected(message id)
    SendRaw(Vec<u8>, u64, MessageId),
    // Expected(payload size)
    Size(u32),
    // Expected(message id)
    MessageId(MessageId),
    // Expected(program id)
    ProgramId(ActorId),
    // Expected(message sender)
    Source(ActorId),
    // Expected(message value)
    Value(u128),
    // Expected(this program's balance)
    ValueAvailable(u128),
    // Params(gas), Expected(message id)
    Reply(u64, MessageId),
    // Params(payload, gas), Expected(message id)
    ReplyRaw(Vec<u8>, u64, MessageId),
    // Expected(reply to id, exit code)
    ReplyDetails(MessageId, i32),
    // Expected(block height)
    BlockHeight(u32),
    // Expected(block timestamp)
    BlockTimestamp(u64),
    // Expected(msg origin)
    Origin(ActorId),
    // Expected(id)
    Reserve(Vec<u8>),
    // Expected(amount)
    Unreserve(u64),
    // Param(salt), Expected(hash, block number)
    Random(Vec<u8>, ([u8; 32], u32)),
    // Expected(lower bound, upper bound )-> estimated gas level
    GasAvailable(u64, u64),
}

#[cfg(not(feature = "wasm-wrapper"))]
mod wasm {
    use super::Kind;
    use codec::Encode;
    use gstd::{
        errors::{ContractError, ExtError, MessageError},
        exec, format,
        msg::{self, MessageHandle},
        prog, ActorId, CodeId, ReservationId,
    };

    static mut CODE_ID: CodeId = CodeId::new([0u8; 32]);
    static mut ORIGIN: Option<ActorId> = None;

    #[no_mangle]
    unsafe extern "C" fn init() {
        let code_id_bytes: [u8; 32] = msg::load().expect("internal error: invalid payload");

        CODE_ID = code_id_bytes.into();
    }

    #[no_mangle]
    unsafe extern "C" fn handle() {
        match msg::load().expect("internal error: invalid payload") {
            Kind::CreateProgram(salt, gas, (expected_mid, expected_pid)) => {
                let salt = salt.to_le_bytes();
                let res = if gas == 0 {
                    prog::create_program_delayed(CODE_ID, salt, "payload", 0, 0)
                } else {
                    prog::create_program_with_gas_delayed(CODE_ID, salt, "payload", gas, 0, 0)
                };
                let (actual_mid, actual_pid) = res.expect("internal error: create program failed");
                let actual_mid: [u8; 32] = actual_mid.into();
                let actual_pid: [u8; 32] = actual_pid.into();
                assert_eq!(
                    expected_mid, actual_mid,
                    "SysCall::CreateProgram: mid test failed"
                );
                assert_eq!(
                    expected_pid, actual_pid,
                    "SysCall::CreateProgram: pid test failed"
                );
            }
            Kind::Error(message_value, expected_err) => {
                let actual_err = msg::reply(b"", message_value).expect_err("not enough balance");
                assert_eq!(
                    expected_err,
                    format!("{actual_err}"),
                    "SysCall::Error: test failed"
                );
            }
            Kind::Send(gas, expected_mid) => {
                let actual_mid_res = if gas == 0 {
                    msg::send_delayed(msg::source(), b"payload", 0, 0)
                } else {
                    msg::send_with_gas_delayed(msg::source(), b"payload", gas, 0, 0)
                };
                assert_eq!(
                    Ok(expected_mid.into()),
                    actual_mid_res,
                    "SysCall::Send: mid test failed"
                );
            }
            Kind::SendRaw(payload, gas, expected_mid) => {
                // Sending these 2 to increase internal handler returned by `send_init`.
                let _ = msg::send_delayed(msg::source(), b"payload", 0, 0);
                let _ = msg::send_delayed(msg::source(), b"payload", 0, 0);

                let handle = MessageHandle::init().expect("internal error: failed send init");
                // check handle
                handle
                    .push(payload)
                    .expect("internal error: failed send_push");
                let actual_mid_res = if gas == 0 {
                    handle.commit_delayed(msg::source(), 0, 0)
                } else {
                    handle.commit_with_gas_delayed(msg::source(), gas, 0, 0)
                };
                assert_eq!(
                    Ok(expected_mid.into()),
                    actual_mid_res,
                    "SysCall::SendRaw: mid test failed"
                );
            }
            Kind::Size(expected_size) => {
                let actual_size = msg::size();
                assert_eq!(
                    expected_size, actual_size,
                    "SysCall::Size: size test failed"
                );
            }
            Kind::MessageId(expected_mid) => {
                let actual_mid: [u8; 32] = msg::id().into();
                assert_eq!(
                    expected_mid, actual_mid,
                    "SysCall::MessageId: mid test failed"
                );
            }
            Kind::ProgramId(expected_pid) => {
                let actual_pid: [u8; 32] = exec::program_id().into();
                assert_eq!(
                    expected_pid, actual_pid,
                    "SysCall::ProgramId: pid test failed"
                );
            }
            Kind::Source(expected_actor) => {
                let actual_actor: [u8; 32] = msg::source().into();
                assert_eq!(
                    expected_actor, actual_actor,
                    "SysCall::Source: actor test failed"
                );
            }
            Kind::Value(expected_value) => {
                let actual_value = msg::value();
                assert_eq!(
                    expected_value, actual_value,
                    "SysCall::Value: value test failed"
                );
            }
            Kind::ValueAvailable(expected_value) => {
                let _ = msg::send_delayed(msg::source(), b"payload", 2000, 0);
                let actual_value = exec::value_available();
                assert_eq!(
                    expected_value, actual_value,
                    "SysCall::ValueAvailable: value test failed"
                );
            }
            Kind::Reply(gas, expected_mid) => {
                let actual_mid_res = if gas == 0 {
                    msg::reply_delayed(b"payload", 0, 0)
                } else {
                    msg::reply_with_gas_delayed(b"payload", gas, 0, 0)
                };
                assert_eq!(
                    Ok(expected_mid.into()),
                    actual_mid_res,
                    "SysCall::Reply: mid test failed"
                );
            }
            Kind::ReplyRaw(payload, gas, expected_mid) => {
                msg::reply_push(payload).expect("internal error: failed reply push");
                let actual_mid_res = if gas == 0 {
                    msg::reply_commit_delayed(0, 0)
                } else {
                    msg::reply_commit_with_gas_delayed(gas, 0, 0)
                };
                assert_eq!(
                    Ok(expected_mid.into()),
                    actual_mid_res,
                    "SysCall::ReplyRaw: mid test failed"
                );
            }
            Kind::ReplyDetails(..) => {
                // Actual test in handle reply, here just sends a reply
                let _ = msg::reply_delayed(b"payload", 0, 0);
                // To prevent from sending to mailbox "ok" message
                exec::leave();
            }
            Kind::BlockHeight(expected_height) => {
                let actual_height = exec::block_height();
                assert_eq!(
                    expected_height, actual_height,
                    "SysCall::BlockHeight:: block height test failed"
                );
            }
            Kind::BlockTimestamp(expected_timestamp) => {
                let actual_timestamp = exec::block_timestamp();
                assert_eq!(
                    expected_timestamp, actual_timestamp,
                    "SysCall::BlockTimestamp:: block timestamp test failed"
                );
            }
            Kind::Origin(expected_actor) => {
                // The origin is set by the first call and then checked with the second
                if ORIGIN.is_some() {
                    // is ser, perform check
                    let actual_actor: [u8; 32] = exec::origin().into();
                    assert_eq!(
                        expected_actor, actual_actor,
                        "SysCall::Origin: actor test failed"
                    );
                } else {
                    ORIGIN = Some(exec::origin());
                    // To prevent from sending to mailbox "ok" message
                    exec::leave();
                }
            }
            Kind::Reserve(expected_id) => {
                // do 2 reservations to increase internal nonce
                let _ = ReservationId::reserve(10_000, 3);
                let _ = ReservationId::reserve(20_000, 5);
                let actual_id =
                    ReservationId::reserve(30_000, 7).expect("internal error: reservation failed");
                assert_eq!(
                    expected_id,
                    actual_id.encode(),
                    "SysCall::Reserve: reserve gas test failed"
                );
            }
            Kind::Unreserve(expected_amount) => {
                let reservation = ReservationId::reserve(expected_amount, 3)
                    .expect("internal error: reservation failed");
                let actual_amount = reservation.unreserve();
                assert_eq!(
                    Ok(expected_amount),
                    actual_amount,
                    "SysCall::Unreserve: unreserve gas test failed"
                );
            }
            Kind::Random(salt, (expected_hash, expected_bn)) => {
                let (actual_hash, actual_bn) =
                    exec::random(&salt).expect("internal error: random call failed");
                assert_eq!(
                    expected_hash, actual_hash,
                    "SysCall::Random: hash test failed"
                );
                assert_eq!(expected_bn, actual_bn, "SysCall::Random: bn test failed");
            }
            Kind::GasAvailable(lower, upper) => {
                let gas_available = exec::gas_available();
                assert!(
                    gas_available >= lower,
                    "SysCall::GasAvailable: lower bound test failed"
                );
                assert!(
                    gas_available <= upper,
                    "SysCall::GasAvailable: upper bound test failed"
                );
            }
        }
        // Report test executed successfully
        msg::send_delayed(msg::source(), b"ok", 0, 0).expect("internal error: report send failed");
    }

    #[no_mangle]
    extern "C" fn handle_reply() {
        if let Ok(Kind::ReplyDetails(expected_reply_to, expected_status_code)) = msg::load() {
            let actual_reply_to = msg::reply_to();
            assert_eq!(
                Ok(expected_reply_to.into()),
                actual_reply_to,
                "SysCall::ReplyDetails: reply_to test failed"
            );
            let actual_status_code = msg::status_code();
            assert_eq!(
                Ok(expected_status_code),
                actual_status_code,
                "SysCall::ReplyDetails: status test failed"
            );

            // Report test executed successfully
            msg::send_delayed(msg::source(), b"ok", 0, 0)
                .expect("internal error: report send failed");
        } else {
            panic!("internal error: invalid payload for `handle_reply`")
        }
    }
}
