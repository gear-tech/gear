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

use crate::{HandleAction, WAIT_AND_RESERVE_WITH_PANIC_GAS};
use gear_core::ids::prelude::*;
use gstd::{
    ActorId, MessageId, debug,
    errors::{ExtError, ReservationError, SignalCode, SimpleExecutionError},
    exec,
    ext::oom_panic,
    msg,
    prelude::*,
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
    Assert(SignalCode),
}

#[unsafe(no_mangle)]
extern "C" fn init() {
    unsafe { INITIATOR = msg::source() };
}

#[unsafe(no_mangle)]
extern "C" fn handle() {
    unsafe { HANDLE_MSG = Some(msg::id()) };
    let do_panic = unsafe { static_mut!(DO_PANIC) };

    let action: HandleAction = msg::load().unwrap();
    match action {
        HandleAction::Simple => {
            // must be unreserved as unused
            exec::system_reserve_gas(100).unwrap();
        }
        HandleAction::Wait => {
            exec::system_reserve_gas(1_000_000_000).unwrap();
            // used to found message id in test
            msg::send(msg::source(), 0, 0).unwrap();
            exec::wait();
        }
        HandleAction::WaitAndPanic => {
            if *do_panic {
                panic!();
            }

            *do_panic = !*do_panic;

            exec::system_reserve_gas(200).unwrap();
            // used to found message id in test
            msg::send(msg::source(), 0, 0).unwrap();
            exec::wait();
        }
        HandleAction::WaitAndReserveWithPanic => {
            if *do_panic {
                exec::system_reserve_gas(1_000_000_000).unwrap();
                panic!();
            }

            *do_panic = !*do_panic;

            exec::system_reserve_gas(WAIT_AND_RESERVE_WITH_PANIC_GAS).unwrap();
            // used to found message id in test
            msg::send(msg::source(), 0, 0).unwrap();
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
            msg::send(msg::source(), 0, 0).unwrap();
            exec::wait();
        }
        HandleAction::Panic => {
            exec::system_reserve_gas(12_000_000_000).unwrap();
            panic!();
        }
        HandleAction::WaitWithReserveAmountAndPanic(gas_amount) => {
            if *do_panic {
                panic!();
            }

            *do_panic = !*do_panic;

            exec::system_reserve_gas(gas_amount).unwrap();
            // used to found message id in test
            msg::send(msg::source(), 0, 0).unwrap();
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
            msg::send(msg::source(), 0, 0).unwrap();
            #[allow(clippy::empty_loop)]
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
                Err(ExtError::Reservation(ReservationError::ZeroReservationAmount).into())
            );
        }
        HandleAction::ForbiddenCallInSignal(user) => {
            unsafe { HANDLE_SIGNAL_STATE = HandleSignalState::ForbiddenCall(user) };
            exec::system_reserve_gas(1_000_000_000).unwrap();
            exec::wait();
        }
        HandleAction::ForbiddenAction => {
            exec::system_reserve_gas(1_000_000_000).unwrap();

            msg::send(ActorId::new(ActorId::SYSTEM.into()), "hello", 0)
                .expect("cannot send message");
        }
        HandleAction::SaveSignal(signal_received) => {
            debug!("handle: signal_received={:?}", signal_received);

            unsafe { HANDLE_SIGNAL_STATE = HandleSignalState::Assert(signal_received) };
        }
        HandleAction::ExceedMemory => {
            exec::system_reserve_gas(1_000_000_000).unwrap();

            oom_panic();
        }
        HandleAction::ExceedStackLimit => {
            exec::system_reserve_gas(1_000_000_000).unwrap();

            #[allow(unconditional_recursion, clippy::unit_arg)]
            fn f() {
                core::hint::black_box(f());
            }

            f();
        }
        HandleAction::UnreachableInstruction => {
            exec::system_reserve_gas(1_000_000_000).unwrap();

            #[cfg(target_arch = "wasm32")]
            core::arch::wasm32::unreachable();
        }
        HandleAction::InvalidDebugCall => {
            exec::system_reserve_gas(1_000_000_000).unwrap();

            #[allow(invalid_from_utf8_unchecked)]
            let invalid_string = unsafe { core::str::from_utf8_unchecked(&[0, 159, 146, 150]) };
            debug!("{}", invalid_string);
        }
        HandleAction::UnrecoverableExt => {
            exec::system_reserve_gas(1_000_000_000).unwrap();

            exec::wait_up_to(0);
        }
        HandleAction::IncorrectFree => {
            exec::system_reserve_gas(1_000_000_000).unwrap();

            unsafe extern "C" {
                fn free(ptr: *mut u8) -> *mut u8;
            }

            unsafe {
                free(usize::MAX as *mut u8);
            }
        }
        HandleAction::WaitWithoutSendingMessage => {
            exec::system_reserve_gas(1_000_000_000).unwrap();

            exec::wait();
        }
        HandleAction::MemoryAccess => {
            exec::system_reserve_gas(1_000_000_000).unwrap();

            const ARRAY_SIZE: usize = 1_000_000;
            let arr = [42u8; ARRAY_SIZE];

            #[allow(clippy::needless_range_loop)]
            for i in 0..ARRAY_SIZE {
                let _value = arr[i];
            }
        }
    }
}

#[unsafe(no_mangle)]
extern "C" fn handle_signal() {
    match unsafe { static_ref!(HANDLE_SIGNAL_STATE) } {
        HandleSignalState::Normal => {
            msg::send(unsafe { INITIATOR }, b"handle_signal", 0).unwrap();
            let signal_code = msg::signal_code()
                .expect("Incorrect call")
                .expect("Unsupported code");
            assert!(matches!(
                signal_code,
                SignalCode::Execution(
                    SimpleExecutionError::UserspacePanic | SimpleExecutionError::RanOutOfGas
                )
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
        HandleSignalState::Assert(signal_saved) => {
            let signal_received = msg::signal_code()
                .expect("Incorrect call")
                .expect("Unsupported code");

            if signal_received == *signal_saved {
                msg::send(unsafe { INITIATOR }, true, 0).unwrap();
            } else {
                msg::send(unsafe { INITIATOR }, false, 0).unwrap();
            }
        }
    }
}

#[unsafe(no_mangle)]
extern "C" fn handle_reply() {
    let handle_msg = unsafe { HANDLE_MSG.unwrap() };
    exec::wake(handle_msg).unwrap();
}
