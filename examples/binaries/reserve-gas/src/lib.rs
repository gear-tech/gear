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
use gstd::{
    errors::{ContractError, ExtError},
    exec, msg,
    prelude::*,
    MessageId, ReservationId,
};

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;
use gstd::errors::ReservationError;

static mut RESERVATION_ID: Option<ReservationId> = None;
static mut INIT_MSG: MessageId = MessageId::new([0; 32]);
static mut WAKE_STATE: WakeState = WakeState::Initial;

pub const RESERVATION_AMOUNT: u64 = 50_000_000;
pub const REPLY_FROM_RESERVATION_PAYLOAD: &[u8; 5] = b"Hello";

#[derive(Debug, Eq, PartialEq)]
enum WakeState {
    Initial,
    Panic,
    Exit,
}

#[derive(Debug, Encode, Decode)]
pub enum InitAction {
    Normal,
    Wait,
    CheckArgs { mailbox_threshold: u64 },
}

#[derive(Debug, Encode, Decode)]
pub enum HandleAction {
    Unreserve,
    Exit,
    ReplyFromReservation,
}

#[derive(Debug, Encode, Decode)]
pub enum ReplyAction {
    Panic,
    Exit,
}

#[no_mangle]
extern "C" fn init() {
    unsafe { INIT_MSG = msg::id() };

    let action: InitAction = msg::load().unwrap();

    match action {
        InitAction::Normal => {
            // will be removed automatically
            let _orphan_reservation =
                ReservationId::reserve(50_000, 3).expect("orphan reservation");

            // must be cleared during `gr_exit`
            let _exit_reservation = ReservationId::reserve(25_000, 5).expect("exit reservation");

            // no actual reservation and unreservation is occurred
            let noop_reservation = ReservationId::reserve(50_000, 10).expect("noop reservation");
            let unreserved_amount = noop_reservation.unreserve().expect("noop unreservation");
            assert_eq!(unreserved_amount, 50_000);

            unsafe {
                RESERVATION_ID = Some(
                    ReservationId::reserve(RESERVATION_AMOUNT, 5)
                        .expect("reservation across executions"),
                )
            };
        }
        InitAction::Wait => match unsafe { &WAKE_STATE } {
            WakeState::Initial => {
                let _reservation = ReservationId::reserve(50_000, 10);
                // to find message to reply to in test
                msg::send(msg::source(), (), 0).unwrap();
                exec::wait();
            }
            WakeState::Panic => {
                panic!()
            }
            WakeState::Exit => {
                exec::exit(msg::source());
            }
        },
        InitAction::CheckArgs { mailbox_threshold } => {
            assert_eq!(
                ReservationId::reserve(0, 10),
                Err(ContractError::Ext(ExtError::Reservation(
                    ReservationError::ReservationBelowMailboxThreshold
                )))
            );

            assert_eq!(
                ReservationId::reserve(50_000, 0),
                Err(ContractError::Ext(ExtError::Reservation(
                    ReservationError::ZeroReservationDuration
                )))
            );

            assert_eq!(
                ReservationId::reserve(mailbox_threshold - 1, 1),
                Err(ContractError::Ext(ExtError::Reservation(
                    ReservationError::ReservationBelowMailboxThreshold
                )))
            );

            assert_eq!(
                ReservationId::reserve(mailbox_threshold, u32::MAX),
                Err(ContractError::Ext(ExtError::Reservation(
                    ReservationError::InsufficientGasForReservation
                )))
            );

            assert_eq!(
                ReservationId::reserve(u64::MAX, 1),
                Err(ContractError::Ext(ExtError::Reservation(
                    ReservationError::InsufficientGasForReservation
                )))
            );
        }
    }
}

#[no_mangle]
extern "C" fn handle() {
    let action: HandleAction = msg::load().unwrap();
    match action {
        HandleAction::Unreserve => {
            let id = unsafe { RESERVATION_ID.take().unwrap() };
            id.unreserve().expect("unreservation across executions");
        }
        HandleAction::Exit => {
            exec::exit(msg::source());
        }
        HandleAction::ReplyFromReservation => {
            let id = unsafe { RESERVATION_ID.take().unwrap() };
            msg::reply_from_reservation(id, REPLY_FROM_RESERVATION_PAYLOAD, 0)
                .expect("unable to reply from reservation");
        }
    }
}

// must be called after `InitAction::Wait`
#[no_mangle]
extern "C" fn handle_reply() {
    let action: ReplyAction = msg::load().unwrap();
    unsafe {
        WAKE_STATE = match action {
            ReplyAction::Panic => WakeState::Panic,
            ReplyAction::Exit => WakeState::Exit,
        };
        exec::wake(INIT_MSG).unwrap();
    }
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
