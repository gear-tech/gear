// This file is part of Gear.

// Copyright (C) 2023-2025 Gear Technologies Inc.
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

use crate::Kind;
use gstd::{
    errors::{ReplyCode, SignalCode, SimpleExecutionError},
    exec, format,
    msg::{self, MessageHandle},
    prelude::*,
    prog, ActorId, CodeId, MessageId, ReservationId, Vec,
};
use parity_scale_codec::Encode;

static mut CODE_ID: CodeId = CodeId::new([0u8; 32]);
static mut SIGNAL_DETAILS: (MessageId, SignalCode, ActorId) = (
    MessageId::new([0; 32]),
    SignalCode::Execution(SimpleExecutionError::Unsupported),
    ActorId::zero(),
);
static mut DO_PANIC: bool = false;

#[unsafe(no_mangle)]
extern "C" fn init() {
    let code_id_bytes: [u8; 32] = msg::load().expect("internal error: invalid payload");

    unsafe { CODE_ID = code_id_bytes.into() };
}

#[unsafe(no_mangle)]
extern "C" fn handle() {
    let syscall_kinds: Vec<Kind> = msg::load().expect("internal error: invalid payload");
    for syscall_kind in syscall_kinds {
        process(syscall_kind);
    }

    // Report test executed successfully
    msg::send_delayed(msg::source(), b"ok", 0, 0).expect("internal error: report send failed");
}

fn process(syscall_kind: Kind) {
    match syscall_kind {
        Kind::CreateProgram(salt, gas_opt, (expected_mid, expected_pid)) => {
            let salt = salt.to_le_bytes();
            let res = match gas_opt {
                Some(gas) => prog::create_program_bytes_with_gas_delayed(
                    unsafe { CODE_ID },
                    salt,
                    "payload",
                    gas,
                    0,
                    0,
                ),
                None => {
                    prog::create_program_bytes_delayed(unsafe { CODE_ID }, salt, "payload", 0, 0)
                }
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
                Some(gas) => {
                    msg::send_input_with_gas_delayed(msg::source(), gas, 0, ..msg::size(), 0)
                }
                None => msg::send_input_delayed(msg::source(), 0, ..msg::size(), 0),
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
                .push_input(0..msg::size())
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
        Kind::ActorId(expected_pid) => {
            let actual_pid: [u8; 32] = exec::program_id().into();
            assert_eq!(expected_pid, actual_pid, "Kind::ActorId: pid test failed");
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
            let _ = msg::send_delayed(msg::source(), b"payload", 10_000_000_000_000, 0);
            let actual_value = exec::value_available();
            assert_eq!(
                expected_value, actual_value,
                "Kind::ValueAvailable: value test failed"
            );
        }
        Kind::Reply(gas_opt, expected_mid) => {
            let actual_mid_res = match gas_opt {
                Some(gas) => msg::reply_with_gas(b"payload", gas, 0),
                None => msg::reply(b"payload", 0),
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
                Some(gas) => msg::reply_commit_with_gas(gas, 0),
                None => msg::reply_commit(0),
            };
            assert_eq!(
                Ok(expected_mid.into()),
                actual_mid_res,
                "Kind::ReplyRaw: mid test failed"
            );
        }
        Kind::ReplyInput(gas_opt, expected_mid) => {
            let actual_mid_res = match gas_opt {
                Some(gas) => msg::reply_input_with_gas(gas, 0, ..msg::size()),
                None => msg::reply_input(0, ..msg::size()),
            };
            assert_eq!(
                Ok(expected_mid.into()),
                actual_mid_res,
                "Kind::ReplyInput: mid test failed"
            );
        }
        Kind::ReplyPushInput(expected_mid) => {
            msg::reply_push_input(..msg::size()).expect("internal error: reply_push_input failed");
            let actual_mid_res = msg::reply_commit(0);
            assert_eq!(
                Ok(expected_mid.into()),
                actual_mid_res,
                "Kind::ReplyPushInput: mid test failed"
            );
        }
        Kind::ReplyDetails(..) => {
            // Actual test in handle reply, here just sends a reply
            let _ = msg::send_delayed(msg::source(), b"payload", 0, 0);
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
                        SignalCode::Execution(SimpleExecutionError::UserspacePanic),
                        msg::source(),
                    );
                    DO_PANIC = true;
                }
                exec::system_reserve_gas(1_000_000_000).unwrap();
                let _ = msg::send_delayed(msg::source(), b"payload", 0, 0);
                exec::wait_for(2);
            }
        }
        Kind::SignalDetailsWake => {
            panic!("must be called in handle_reply");
        }
        Kind::EnvVars {
            performance_multiplier: expected_performance_multiplier_percent,
            existential_deposit: expected_existential_deposit,
            mailbox_threshold: expected_mailbox_threshold,
            gas_to_value_multiplier: expected_gas_to_value_multiplier,
        } => {
            let env_vars = exec::env_vars();
            let actual_performance_multiplier = env_vars.performance_multiplier;
            assert_eq!(
                actual_performance_multiplier.value(),
                expected_performance_multiplier_percent,
                "Kind::EnvVars: performance_multiplier test failed"
            );
            let actual_existential_deposit = env_vars.existential_deposit;
            assert_eq!(
                actual_existential_deposit, expected_existential_deposit,
                "Kind::EnvVars: existential_deposit test failed"
            );
            let actual_mailbox_threshold = env_vars.mailbox_threshold;
            assert_eq!(
                actual_mailbox_threshold, expected_mailbox_threshold,
                "Kind::EnvVars: mailbox_threshold test failed"
            );
            let actual_gas_multiplier = env_vars.gas_multiplier;
            assert_eq!(
                actual_gas_multiplier.gas_to_value(1),
                expected_gas_to_value_multiplier,
                "Kind::EnvVars: gas_to_value_multiplier test failed"
            );
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
            let actual_mid =
                msg::send_bytes_delayed_from_reservation(reservation_id, msg::source(), b"", 0, 0);
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
            let actual_mid = msg::reply_bytes_from_reservation(reservation_id, b"", 0);
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
            let actual_mid = msg::reply_commit_from_reservation(reservation_id, 0);
            assert_eq!(
                Ok(expected_mid.into()),
                actual_mid,
                "Kind::ReservationReplyCommit: mid test failed"
            );
        }
        Kind::SystemReserveGas(amount) => {
            exec::system_reserve_gas(amount).expect("Kind::SystemReserveGas: call test failed");
            // The only case with wait, so we send report before ending execution, instead of
            // waking the message
            msg::send_delayed(msg::source(), b"ok", 0, 0)
                .expect("internal error: report send failed");
            exec::wait_for(2);
        }
        Kind::ReplyDeposit(amount) => {
            let mid = msg::send_bytes(ActorId::zero(), [], 0)
                .expect("internal error: failed to send message");

            exec::reply_deposit(mid, amount).expect("Kind::ReplyDeposit: call test failed");
        }
    }
}

#[unsafe(no_mangle)]
extern "C" fn handle_reply() {
    match msg::load() {
        Ok(Kind::ReplyDetails(expected_reply_to, expected_reply_code_bytes)) => {
            let expected_reply_code = ReplyCode::from_bytes(expected_reply_code_bytes);
            let actual_reply_to = msg::reply_to();
            assert_eq!(
                Ok(expected_reply_to.into()),
                actual_reply_to,
                "Kind::ReplyDetails: reply_to test failed"
            );
            let actual_reply_code = msg::reply_code();
            assert_eq!(
                Ok(expected_reply_code),
                actual_reply_code,
                "Kind::ReplyDetails: reply code test failed"
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

#[unsafe(no_mangle)]
extern "C" fn handle_signal() {
    let (signal_from, signal_code, source) = unsafe { SIGNAL_DETAILS };

    assert_eq!(
        msg::signal_code(),
        Ok(Some(signal_code)),
        "Kind::SignalDetails: status code test failed"
    );
    assert_eq!(
        msg::signal_from(),
        Ok(signal_from),
        "Kind::SignalDetails: signal_from test failed"
    );

    msg::send_delayed(source, b"ok", 0, 0).expect("internal error: report send failed");
}
