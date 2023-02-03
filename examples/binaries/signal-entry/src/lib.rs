// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
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

use gstd::{Decode, Encode};

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

#[derive(Debug, Encode, Decode)]
pub enum HandleAction {
    Simple,
    Wait,
    WaitAndPanic,
    WaitAndReserveWithPanic,
    WaitAndExit,
    WaitWithReserveAmountAndPanic(u64),
    Panic,
    Exit,
    Accumulate,
    OutOfGas,
    PanicInSignal,
    AcrossWaits,
    ZeroReserve,
    ForbiddenCallInSignal([u8; 32]),
}

#[cfg(not(feature = "std"))]
mod wasm {
    use super::*;
    use gstd::{
        errors::{
            ExtError, ReservationError, SimpleCodec, SimpleExecutionError, SimpleSignalError,
        },
        exec, msg,
        prelude::*,
        ActorId, MessageId,
    };

    static mut INITIATOR: ActorId = ActorId::zero();
    static mut HANDLE_MSG: Option<MessageId> = None;
    static mut DO_PANIC: bool = false;
    static mut DO_EXIT: bool = false;
    static mut HANDLE_SIGNAL_STATE: HandleSignalState = HandleSignalState::Normal;

    enum HandleSignalState {
        Normal,
        Panic,
        ForbiddenCall([u8; 32]),
    }

    #[no_mangle]
    extern "C" fn init() {
        unsafe { INITIATOR = msg::source() };
    }

    #[no_mangle]
    extern "C" fn handle() {
        unsafe { HANDLE_MSG = Some(msg::id()) };
        let do_panic = unsafe { &mut DO_PANIC };

        let action: HandleAction = msg::load().unwrap();
        match action {
            HandleAction::Simple => {
                // must be unreserved as unused
                exec::system_reserve_gas(100).unwrap();
            }
            HandleAction::Wait => {
                exec::system_reserve_gas(1_000_000_000).unwrap();
                // used to found message id in test
                msg::reply(0, 0).unwrap();
                exec::wait();
            }
            HandleAction::WaitAndPanic => {
                if *do_panic {
                    panic!();
                }

                *do_panic = !*do_panic;

                exec::system_reserve_gas(200).unwrap();
                // used to found message id in test
                msg::reply(0, 0).unwrap();
                exec::wait();
            }
            HandleAction::WaitAndReserveWithPanic => {
                if *do_panic {
                    exec::system_reserve_gas(1_000_000_000).unwrap();
                    panic!();
                }

                *do_panic = !*do_panic;

                exec::system_reserve_gas(2_000_000_000).unwrap();
                // used to found message id in test
                msg::reply(0, 0).unwrap();
                exec::wait();
            }
            HandleAction::WaitAndExit => {
                if unsafe { DO_EXIT } {
                    msg::send_bytes(msg::source(), b"wait_and_exit", 0).unwrap();
                    exec::exit(msg::source());
                }

                unsafe { DO_EXIT = !DO_EXIT };

                exec::system_reserve_gas(900).unwrap();
                // used to found message id in test
                msg::reply(0, 0).unwrap();
                exec::wait();
            }
            HandleAction::Panic => {
                exec::system_reserve_gas(5_000_000_000).unwrap();
                panic!();
            }
            HandleAction::WaitWithReserveAmountAndPanic(gas_amount) => {
                if *do_panic {
                    panic!();
                }

                *do_panic = !*do_panic;

                exec::system_reserve_gas(gas_amount).unwrap();
                // used to found message id in test
                msg::reply(0, 0).unwrap();
                exec::wait();
            }
            HandleAction::Exit => {
                exec::system_reserve_gas(4_000_000_000).unwrap();
                msg::reply_bytes(b"exit", 0).unwrap();
                exec::exit(msg::source());
            }
            HandleAction::Accumulate => {
                exec::system_reserve_gas(1000).unwrap();
                exec::system_reserve_gas(234).unwrap();
                exec::wait();
            }
            HandleAction::OutOfGas => {
                exec::system_reserve_gas(5_000_000_000).unwrap();
                // used to found message id in test
                msg::reply(0, 0).unwrap();
                loop {}
            }
            HandleAction::AcrossWaits => {
                exec::system_reserve_gas(1_000_000_000).unwrap();
                // used to found message id in test
                // we use send instead of reply to avoid duplicated reply error.
                msg::send(msg::source(), 0, 0).unwrap();
                exec::wait();
            }
            HandleAction::PanicInSignal => {
                unsafe { HANDLE_SIGNAL_STATE = HandleSignalState::Panic };
                exec::system_reserve_gas(5_000_000_000).unwrap();
                exec::wait();
            }
            HandleAction::ZeroReserve => {
                let res = exec::system_reserve_gas(0);
                assert_eq!(
                    res,
                    Err(ExtError::Reservation(
                        ReservationError::ZeroReservationAmount
                    ))
                );
            }
            HandleAction::ForbiddenCallInSignal(user) => {
                unsafe { HANDLE_SIGNAL_STATE = HandleSignalState::ForbiddenCall(user) };
                exec::system_reserve_gas(1_000_000_000).unwrap();
                exec::wait();
            }
        }
    }

    #[no_mangle]
    extern "C" fn handle_signal() {
        match unsafe { &HANDLE_SIGNAL_STATE } {
            HandleSignalState::Normal => {
                msg::send(unsafe { INITIATOR }, b"handle_signal", 0).unwrap();
                let status_code = msg::status_code().unwrap();
                let err = SimpleSignalError::from_status_code(status_code).expect("Known error");
                assert!(matches!(
                    err,
                    SimpleSignalError::Execution(SimpleExecutionError::Panic)
                        | SimpleSignalError::Execution(SimpleExecutionError::GasLimitExceeded)
                ));

                if let Some(handle_msg) = unsafe { HANDLE_MSG } {
                    assert_eq!(msg::signal_from(), Ok(handle_msg));
                }

                // TODO: check gas limit (#1796)
                // assert_eq!(msg::gas_limit(), 5_000_000_000);
            }
            HandleSignalState::Panic => {
                // to be sure state rolls back so this message won't appear in mailbox in test
                msg::send(unsafe { INITIATOR }, b"handle_signal_panic", 0).unwrap();
                panic!();
            }
            HandleSignalState::ForbiddenCall(user) => {
                msg::send_bytes((*user).into(), b"handle_signal_forbidden_call", 0).unwrap();
                let _ = msg::source();
            }
        }
    }

    #[no_mangle]
    extern "C" fn handle_reply() {
        let handle_msg = unsafe { HANDLE_MSG.unwrap() };
        exec::wake(handle_msg).unwrap();
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use gstd::errors::{SimpleExecutionError, SimpleSignalError};
    use gtest::{Program, System};

    #[test]
    fn signal_can_be_sent() {
        let system = System::new();
        system.init_logger();

        let program = Program::current(&system);

        let res = program.send_signal(0, SimpleSignalError::Execution(SimpleExecutionError::Panic));
        assert!(!res.main_failed());
    }
}
