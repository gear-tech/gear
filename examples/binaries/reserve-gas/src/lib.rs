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

use codec::{Decode, Encode};
use gstd::{exec, msg, prelude::*, MessageId, ReservationId};

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

static mut RESERVATION_ID: Option<ReservationId> = None;
static mut INIT_MSG: MessageId = MessageId::new([0; 32]);
static mut WAKE_STATE: WakeState = WakeState::FirstExecution;

const RESERVATION_AMOUNT: u64 = 50_000_000;

#[derive(Debug, Eq, PartialEq)]
enum WakeState {
    FirstExecution,
    SecondExecution,
}

#[derive(Debug, Encode, Decode)]
pub enum InitAction {
    Normal,
    Wait,
}

#[derive(Debug, Encode, Decode)]
pub enum HandleAction {
    Unreserve,
    Exit,
}

#[no_mangle]
unsafe extern "C" fn init() {
    INIT_MSG = msg::id();

    let action: InitAction = msg::load().unwrap();

    match action {
        InitAction::Normal => {
            // will be removed automatically
            let _orphan_reservation = ReservationId::reserve(50_000, 3);

            // must be cleared during `gr_exit`
            let _exit_reservation = ReservationId::reserve(25_000, 5);

            // no actual reservation and unreservation is occurred
            let noop_reservation = ReservationId::reserve(50_000, 10).unwrap();
            let unreserved_amount = noop_reservation.unreserve().unwrap();
            assert_eq!(unreserved_amount, 50_000);

            RESERVATION_ID = Some(ReservationId::reserve(RESERVATION_AMOUNT, 5).unwrap());
        }
        InitAction::Wait => {
            if WAKE_STATE == WakeState::SecondExecution {
                panic!();
            }

            let _reservation = ReservationId::reserve(50_000, 10);
            // to find message to reply to in test
            msg::send(msg::source(), (), 0).unwrap();
            exec::wait();
        }
    }
}

#[no_mangle]
unsafe extern "C" fn handle() {
    let action: HandleAction = msg::load().unwrap();
    match action {
        HandleAction::Unreserve => {
            let id = RESERVATION_ID.take().unwrap();
            id.unreserve().unwrap();
        }
        HandleAction::Exit => {
            exec::exit(msg::source());
        }
    }
}

// must be called after `InitAction::Wait`
#[no_mangle]
unsafe extern "C" fn handle_reply() {
    WAKE_STATE = WakeState::SecondExecution;
    exec::wake(INIT_MSG).unwrap();
}

#[cfg(test)]
mod tests {
    extern crate std;

    use crate::InitAction;
    use gtest::{Program, System};

    #[test]
    fn program_can_be_initialized() {
        let system = System::new();
        system.init_logger();

        let program = Program::current(&system);

        let res = program.send(0, InitAction::Normal);
        assert!(!res.main_failed());
    }
}
