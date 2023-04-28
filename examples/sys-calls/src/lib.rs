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
    CreateProgram(u64, Option<u64>, (MessageId, ActorId)),
    // Params(value), Expected(error message)
    Error(u128, String),
    // Params(gas), Expected(message id)
    Send(Option<u64>, MessageId),
    // Params(payload, gas), Expected(message id)
    SendRaw(Vec<u8>, Option<u64>, MessageId),
    // Params(gas), Expected(message id)
    SendInput(Option<u64>, MessageId),
    // Expected(message id)
    SendPushInput(MessageId),
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
    Reply(Option<u64>, MessageId),
    // Params(payload, gas), Expected(message id)
    ReplyRaw(Vec<u8>, Option<u64>, MessageId),
    // Params(gas), Expected(message id)
    ReplyInput(Option<u64>, MessageId),
    // Expected(message id)
    ReplyPushInput(MessageId),
    // Expected(reply to id, exit code)
    ReplyDetails(MessageId, i32),
    SignalDetails,
    SignalDetailsWake,
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
    Random([u8; 32], ([u8; 32], u32)),
    // Expected(lower bound, upper bound )-> estimated gas level
    GasAvailable(u64, u64),
    // Expected(message id)
    ReservationSend(MessageId),
    // Param(payload), Expected(message id)
    ReservationSendRaw(Vec<u8>, MessageId),
    // Expected(message id)
    ReservationReply(MessageId),
    // Param(payload), Expected(message id)
    ReservationReplyCommit(Vec<u8>, MessageId),
    // Param(reserve amount)
    SystemReserveGas(u64),
}

#[cfg(not(feature = "wasm-wrapper"))]
mod wasm {
    use super::Kind;
    use codec::Encode;
    use gstd::{
        errors::{SimpleCodec, SimpleExecutionError, SimpleSignalError},
        exec, format,
        msg::{self, MessageHandle},
        prog, ActorId, CodeId, MessageId, ReservationId,
    };

    static mut CODE_ID: CodeId = CodeId::new([0u8; 32]);
    static mut ORIGIN: Option<ActorId> = None;
    static mut SIGNAL_DETAILS: (MessageId, SimpleSignalError, ActorId) = (
        MessageId::new([0; 32]),
        SimpleSignalError::Execution(SimpleExecutionError::Unknown),
        ActorId::zero(),
    );
    static mut DO_PANIC: bool = false;

    #[no_mangle]
    extern "C" fn init() {
        let code_id_bytes: [u8; 32] = msg::load().expect("internal error: invalid payload");

        unsafe { CODE_ID = code_id_bytes.into() };
    }

    #[no_mangle]
    extern "C" fn handle() {
        match msg::load().expect("internal error: invalid payload") {
            Kind::CreateProgram(salt, gas_opt, (expected_mid, expected_pid)) => {
                let salt = salt.to_le_bytes();
                let res = match gas_opt {
                    Some(gas) => prog::create_program_with_gas_delayed(
                        unsafe { CODE_ID },
                        salt,
                        "payload",
                        gas,
                        0,
                        0,
                    ),
                    None => prog::create_program_delayed(unsafe { CODE_ID }, salt, "payload", 0, 0),
                };
                let (actual_mid, actual_pid) = res.expect("internal error: create program failed");
                let actual_mid: [u8; 32] = actual_mid.into();
                let actual_pid: [u8; 32] = actual_pid.into();
                assert_eq!(
                    expected_mid, actual_mid,
                    "Kind::CreateProgram: mid test failed"
                );
                assert_eq!(
                    expected_pid, actual_pid,
                    "Kind::CreateProgram: pid test failed"
                );
            }
            Kind::Error(message_value, expected_err) => {
                let actual_err = msg::reply(b"", message_value).expect_err("not enough balance");
                assert_eq!(
                    expected_err,
                    format!("{actual_err}"),
                    "Kind::Error: test failed"
                );
            }
            Kind::Send(gas_opt, expected_mid) => {
                let actual_mid_res = match gas_opt {
                    Some(gas) => msg::send_with_gas_delayed(msg::source(), b"payload", gas, 0, 0),
                    None => msg::send_delayed(msg::source(), b"payload", 0, 0),
                };
                assert_eq!(
                    Ok(expected_mid.into()),
                    actual_mid_res,
                    "Kind::Send: mid test failed"
                );
            }
            Kind::SendInput(gas_opt, expected_mid) => {
                let actual_mid_res = match gas_opt {
                    Some(gas) => msg::send_input_with_gas_delayed(msg::source(), gas, 0, .., 0),
                    None => msg::send_input_delayed(msg::source(), 0, .., 0),
                };
                assert_eq!(
                    Ok(expected_mid.into()),
                    actual_mid_res,
                    "Kind::SendInput: mid test failed"
                );
            }
            Kind::SendPushInput(expected_mid) => {
                // Sending these 2 to increase internal handler returned by `send_init`.
                let _ = msg::send_delayed(msg::source(), b"payload", 0, 0);
                let _ = msg::send_delayed(msg::source(), b"payload", 0, 0);

                let handle = MessageHandle::init().expect("internal error: failed send init");

                // check handle
                handle
                    .push_input(0..)
                    .expect("internal error: push_input failed");

                let actual_mid_res = handle.commit_delayed(msg::source(), 0, 0);
                assert_eq!(
                    Ok(expected_mid.into()),
                    actual_mid_res,
                    "Kind::SendPushInput: mid test failed"
                );
            }
            Kind::SendRaw(payload, gas_opt, expected_mid) => {
                // Sending these 2 to increase internal handler returned by `send_init`.
                let _ = msg::send_delayed(msg::source(), b"payload", 0, 0);
                let _ = msg::send_delayed(msg::source(), b"payload", 0, 0);

                let handle = MessageHandle::init().expect("internal error: failed send init");
                // check handle
                handle
                    .push(payload)
                    .expect("internal error: failed send_push");
                let actual_mid_res = match gas_opt {
                    Some(gas) => handle.commit_with_gas_delayed(msg::source(), gas, 0, 0),
                    None => handle.commit_delayed(msg::source(), 0, 0),
                };
                assert_eq!(
                    Ok(expected_mid.into()),
                    actual_mid_res,
                    "Kind::SendRaw: mid test failed"
                );
            }
            Kind::Size(expected_size) => {
                let actual_size = msg::size();
                assert_eq!(
                    expected_size as usize, actual_size,
                    "Kind::Size: size test failed"
                );
            }
            Kind::MessageId(expected_mid) => {
                let actual_mid: [u8; 32] = msg::id().into();
                assert_eq!(expected_mid, actual_mid, "Kind::MessageId: mid test failed");
            }
            Kind::ProgramId(expected_pid) => {
                let actual_pid: [u8; 32] = exec::program_id().into();
                assert_eq!(expected_pid, actual_pid, "Kind::ProgramId: pid test failed");
            }
            Kind::Source(expected_actor) => {
                let actual_actor: [u8; 32] = msg::source().into();
                assert_eq!(
                    expected_actor, actual_actor,
                    "Kind::Source: actor test failed"
                );
            }
            Kind::Value(expected_value) => {
                let actual_value = msg::value();
                assert_eq!(
                    expected_value, actual_value,
                    "Kind::Value: value test failed"
                );
            }
            Kind::ValueAvailable(expected_value) => {
                let _ = msg::send_delayed(msg::source(), b"payload", 2000, 0);
                let actual_value = exec::value_available();
                assert_eq!(
                    expected_value, actual_value,
                    "Kind::ValueAvailable: value test failed"
                );
            }
            Kind::Reply(gas_opt, expected_mid) => {
                let actual_mid_res = match gas_opt {
                    Some(gas) => msg::reply_with_gas_delayed(b"payload", gas, 0, 0),
                    None => msg::reply_delayed(b"payload", 0, 0),
                };
                assert_eq!(
                    Ok(expected_mid.into()),
                    actual_mid_res,
                    "Kind::Reply: mid test failed"
                );
            }
            Kind::ReplyRaw(payload, gas_opt, expected_mid) => {
                msg::reply_push(payload).expect("internal error: failed reply push");
                let actual_mid_res = match gas_opt {
                    Some(gas) => msg::reply_commit_with_gas_delayed(gas, 0, 0),
                    None => msg::reply_commit_delayed(0, 0),
                };
                assert_eq!(
                    Ok(expected_mid.into()),
                    actual_mid_res,
                    "Kind::ReplyRaw: mid test failed"
                );
            }
            Kind::ReplyInput(gas_opt, expected_mid) => {
                let actual_mid_res = match gas_opt {
                    Some(gas) => msg::reply_input_with_gas_delayed(gas, 0, .., 0),
                    None => msg::reply_input_delayed(0, .., 0),
                };
                assert_eq!(
                    Ok(expected_mid.into()),
                    actual_mid_res,
                    "Kind::ReplyInput: mid test failed"
                );
            }
            Kind::ReplyPushInput(expected_mid) => {
                msg::reply_push_input(..).expect("internal error: reply_push_input failed");
                let actual_mid_res = msg::reply_commit_delayed(0, 0);
                assert_eq!(
                    Ok(expected_mid.into()),
                    actual_mid_res,
                    "Kind::ReplyPushInput: mid test failed"
                );
            }
            Kind::ReplyDetails(..) => {
                // Actual test in handle reply, here just sends a reply
                let _ = msg::reply_delayed(b"payload", 0, 0);
                // To prevent from sending to mailbox "ok" message
                exec::leave();
            }
            Kind::SignalDetails => {
                if unsafe { DO_PANIC } {
                    // issue a signal
                    panic!();
                } else {
                    unsafe {
                        SIGNAL_DETAILS = (
                            msg::id(),
                            SimpleSignalError::Execution(SimpleExecutionError::Panic),
                            msg::source(),
                        );
                        DO_PANIC = true;
                    }
                    exec::system_reserve_gas(1_000_000_000).unwrap();
                    let _ = msg::reply_delayed(b"payload", 0, 0);
                    exec::wait_for(2);
                }
            }
            Kind::SignalDetailsWake => {
                panic!("must be called in handle_reply");
            }
            Kind::BlockHeight(expected_height) => {
                let actual_height = exec::block_height();
                assert_eq!(
                    expected_height, actual_height,
                    "Kind::BlockHeight:: block height test failed"
                );
            }
            Kind::BlockTimestamp(expected_timestamp) => {
                let actual_timestamp = exec::block_timestamp();
                assert_eq!(
                    expected_timestamp, actual_timestamp,
                    "Kind::BlockTimestamp:: block timestamp test failed"
                );
            }
            Kind::Origin(expected_actor) => {
                // The origin is set by the first call and then checked with the second
                if unsafe { ORIGIN.is_some() } {
                    // is ser, perform check
                    let actual_actor: [u8; 32] = exec::origin().into();
                    assert_eq!(
                        expected_actor, actual_actor,
                        "Kind::Origin: actor test failed"
                    );
                } else {
                    unsafe { ORIGIN = Some(exec::origin()) };
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
                    "Kind::Reserve: reserve gas test failed"
                );
            }
            Kind::Unreserve(expected_amount) => {
                let reservation = ReservationId::reserve(expected_amount, 3)
                    .expect("internal error: reservation failed");
                let actual_amount = reservation.unreserve();
                assert_eq!(
                    Ok(expected_amount),
                    actual_amount,
                    "Kind::Unreserve: unreserve gas test failed"
                );
            }
            Kind::Random(salt, (expected_hash, expected_bn)) => {
                let (actual_hash, actual_bn) =
                    exec::random(salt).expect("internal error: random call failed");
                assert_eq!(expected_hash, actual_hash, "Kind::Random: hash test failed");
                assert_eq!(expected_bn, actual_bn, "Kind::Random: bn test failed");
            }
            Kind::GasAvailable(lower, upper) => {
                let gas_available = exec::gas_available();
                assert!(
                    gas_available >= lower,
                    "Kind::GasAvailable: lower bound test failed"
                );
                assert!(
                    gas_available <= upper,
                    "Kind::GasAvailable: upper bound test failed"
                );
            }
            Kind::ReservationSend(expected_mid) => {
                let reservation_id =
                    ReservationId::reserve(25_000_000_000, 1).expect("reservation failed");
                let actual_mid = msg::send_bytes_delayed_from_reservation(
                    reservation_id,
                    msg::source(),
                    b"",
                    0,
                    0,
                );
                assert_eq!(
                    Ok(expected_mid.into()),
                    actual_mid,
                    "Kind::ReservationSend: mid test failed"
                );
            }
            Kind::ReservationSendRaw(payload, expected_mid) => {
                let _ = msg::send_delayed(msg::source(), b"payload", 0, 0);
                let reservation_id =
                    ReservationId::reserve(25_000_000_000, 1).expect("reservation failed");

                let handle = MessageHandle::init().expect("internal error: failed send init");
                // check handle
                handle
                    .push(payload)
                    .expect("internal error: failed send_push");
                let actual_mid =
                    handle.commit_delayed_from_reservation(reservation_id, msg::source(), 0, 0);
                assert_eq!(
                    Ok(expected_mid.into()),
                    actual_mid,
                    "Kind::ReservationSendRaw: mid test failed"
                );
            }
            Kind::ReservationReply(expected_mid) => {
                let reservation_id =
                    ReservationId::reserve(25_000_000_000, 1).expect("reservation failed");
                let actual_mid =
                    msg::reply_bytes_delayed_from_reservation(reservation_id, b"", 0, 0);
                assert_eq!(
                    Ok(expected_mid.into()),
                    actual_mid,
                    "Kind::ReservationReply: mid test failed"
                );
            }
            Kind::ReservationReplyCommit(payload, expected_mid) => {
                let reservation_id =
                    ReservationId::reserve(25_000_000_000, 1).expect("reservation failed");
                msg::reply_push(payload).expect("internal error: failed reply push");
                let actual_mid = msg::reply_commit_delayed_from_reservation(reservation_id, 0, 0);
                assert_eq!(
                    Ok(expected_mid.into()),
                    actual_mid,
                    "Kind::ReservationReplyCommit: mid test failed"
                );
            }
            Kind::SystemReserveGas(amount) => {
                let _ = exec::system_reserve_gas(amount)
                    .expect("Kind::SystemReserveGas: call test failed");
                // The only case with wait, so we send report before ending execution, instead of
                // waking the message
                msg::send_delayed(msg::source(), b"ok", 0, 0)
                    .expect("internal error: report send failed");
                exec::wait_for(2);
            }
        }
        // Report test executed successfully
        msg::send_delayed(msg::source(), b"ok", 0, 0).expect("internal error: report send failed");
    }

    #[no_mangle]
    extern "C" fn handle_reply() {
        match msg::load() {
            Ok(Kind::ReplyDetails(expected_reply_to, expected_status_code)) => {
                let actual_reply_to = msg::reply_to();
                assert_eq!(
                    Ok(expected_reply_to.into()),
                    actual_reply_to,
                    "Kind::ReplyDetails: reply_to test failed"
                );
                let actual_status_code = msg::status_code();
                assert_eq!(
                    Ok(expected_status_code),
                    actual_status_code,
                    "Kind::ReplyDetails: status test failed"
                );

                // Report test executed successfully
                msg::send_delayed(msg::source(), b"ok", 0, 0)
                    .expect("internal error: report send failed");
            }
            Ok(Kind::SignalDetailsWake) => unsafe {
                exec::wake(SIGNAL_DETAILS.0).unwrap();
            },
            _ => panic!("internal error: invalid payload for `handle_reply`"),
        }
    }

    #[no_mangle]
    extern "C" fn handle_signal() {
        let (signal_from, status_code, source) = unsafe { SIGNAL_DETAILS };

        assert_eq!(
            <_>::from_status_code(msg::status_code().unwrap()),
            Some(status_code),
            "Kind::SignalDetails: status code test failed"
        );
        assert_eq!(
            msg::signal_from(),
            Ok(signal_from),
            "Kind::SignalDetails: signal_from test failed"
        );

        msg::send_delayed(source, b"ok", 0, 0).expect("internal error: report send failed");
    }
}
