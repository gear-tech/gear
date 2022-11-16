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
    Panic,
    Accumulate,
    OutOfGas,
    PanicInSignal,
    AcrossWaits,
    ZeroReserve,
    ForbiddenCallInSignal,
}

#[cfg(not(feature = "std"))]
mod wasm {
    use super::*;
    use gstd::{
        errors::{ExecutionError, ExtError},
        exec, msg,
        prelude::*,
        ActorId, MessageId,
    };

    static mut INITIATOR: ActorId = ActorId::zero();
    static mut HANDLE_MSG: MessageId = MessageId::new([0; 32]);
    static mut DO_PANIC: bool = false;
    static mut HANDLE_SIGNAL_STATE: HandleSignalState = HandleSignalState::Normal;

    enum HandleSignalState {
        Normal,
        Panic,
        ForbiddenCall,
    }

    #[no_mangle]
    unsafe extern "C" fn init() {
        INITIATOR = msg::source();
    }

    #[no_mangle]
    unsafe extern "C" fn handle() {
        HANDLE_MSG = msg::id();

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
                if DO_PANIC {
                    panic!();
                }

                DO_PANIC = !DO_PANIC;

                exec::system_reserve_gas(200).unwrap();
                // used to found message id in test
                msg::reply(0, 0).unwrap();
                exec::wait();
            }
            HandleAction::WaitAndReserveWithPanic => {
                if DO_PANIC {
                    exec::system_reserve_gas(1_000_000_000).unwrap();
                    panic!();
                }

                DO_PANIC = !DO_PANIC;

                exec::system_reserve_gas(2_000_000_000).unwrap();
                // used to found message id in test
                msg::reply(0, 0).unwrap();
                exec::wait();
            }
            HandleAction::Panic => {
                exec::system_reserve_gas(5_000_000_000).unwrap();
                panic!();
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
                HANDLE_SIGNAL_STATE = HandleSignalState::Panic;
                exec::system_reserve_gas(5_000_000_000).unwrap();
                exec::wait();
            }
            HandleAction::ZeroReserve => {
                let res = exec::system_reserve_gas(0);
                assert_eq!(
                    res,
                    Err(ExtError::Execution(
                        ExecutionError::ZeroSystemReservationAmount
                    ))
                );
            }
            HandleAction::ForbiddenCallInSignal => {
                HANDLE_SIGNAL_STATE = HandleSignalState::ForbiddenCall;
                exec::system_reserve_gas(1_000_000_000).unwrap();
                exec::wait();
            }
        }
    }

    #[no_mangle]
    unsafe extern "C" fn handle_signal() {
        match HANDLE_SIGNAL_STATE {
            HandleSignalState::Normal => {
                msg::send(INITIATOR, b"handle_signal", 0).unwrap();
                assert_eq!(msg::status_code().unwrap(), 1);

                // TODO: check gas limit (#1796)
                // assert_eq!(msg::gas_limit(), 5_000_000_000);
            }
            HandleSignalState::Panic => {
                // to be sure state rolls back so this message won't appear in mailbox in test
                msg::send(INITIATOR, b"handle_signal_panic", 0).unwrap();
                panic!();
            }
            HandleSignalState::ForbiddenCall => {
                let _ = msg::source();
            }
        }
    }

    #[no_mangle]
    unsafe extern "C" fn handle_reply() {
        exec::wake(HANDLE_MSG).unwrap();
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use gtest::{Program, System};

    #[test]
    fn signal_can_be_sent() {
        let system = System::new();
        system.init_logger();

        let program = Program::current(&system);

        let res = program.send_signal(0, 1);
        assert!(!res.main_failed());
    }
}
