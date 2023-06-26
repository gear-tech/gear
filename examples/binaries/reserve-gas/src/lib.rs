// This file is part of Gear.

// Copyright (C) 2022-2023 Gear Technologies Inc.
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

use alloc::vec::Vec;
use parity_scale_codec::{Decode, Encode};

#[cfg(feature = "wasm-wrapper")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "wasm-wrapper")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

pub const RESERVATION_AMOUNT: u64 = 50_000_000;
pub const REPLY_FROM_RESERVATION_PAYLOAD: &[u8; 5] = b"Hello";

#[derive(Debug, Encode, Decode)]
pub enum InitAction {
    Normal(Vec<(u64, u32)>),
    Wait,
    CheckArgs { mailbox_threshold: u64 },
    FreshReserveUnreserve,
}

#[derive(Debug, Encode, Decode)]
pub enum HandleAction {
    Unreserve,
    Exit,
    ReplyFromReservation,
    AddReservationToList(GasAmount, BlockCount),
    ConsumeReservationsFromList,
    RunInifitely,
    SendFromReservationAndUnreserve,
}

#[derive(Debug, Encode, Decode)]
pub enum ReplyAction {
    Panic,
    Exit,
}

pub type GasAmount = u64;
pub type BlockCount = u32;

#[cfg(not(feature = "wasm-wrapper"))]
mod wasm {
    use super::*;
    use gstd::{
        errors::{Error, ExecutionError, ExtError, ReservationError},
        exec, msg,
        prelude::*,
        MessageId, ReservationId,
    };

    static mut RESERVATION_ID: Option<ReservationId> = None;
    static mut RESERVATIONS: Vec<ReservationId> = Vec::new();
    static mut INIT_MSG: MessageId = MessageId::new([0; 32]);
    static mut WAKE_STATE: WakeState = WakeState::Initial;

    #[derive(Debug, Eq, PartialEq)]
    enum WakeState {
        Initial,
        Panic,
        Exit,
    }

    #[no_mangle]
    extern "C" fn init() {
        unsafe { INIT_MSG = msg::id() };

        let action: InitAction = msg::load().unwrap();

        match action {
            InitAction::Normal(ref reservations) => {
                for (amount, duration) in reservations {
                    let _ = ReservationId::reserve(*amount, *duration).expect("reservation");
                }

                // no actual reservation and unreservation is occurred
                let noop_reservation =
                    ReservationId::reserve(50_000, 10).expect("noop reservation");
                let unreserved_amount = noop_reservation.unreserve().expect("noop unreservation");
                assert_eq!(unreserved_amount, 50_000);

                unsafe {
                    RESERVATION_ID = Some(
                        ReservationId::reserve(RESERVATION_AMOUNT, 15)
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
                    Err(Error::Ext(ExtError::Reservation(
                        ReservationError::ReservationBelowMailboxThreshold
                    )))
                );

                assert_eq!(
                    ReservationId::reserve(50_000, 0),
                    Err(Error::Ext(ExtError::Reservation(
                        ReservationError::ZeroReservationDuration
                    )))
                );

                assert_eq!(
                    ReservationId::reserve(mailbox_threshold - 1, 1),
                    Err(Error::Ext(ExtError::Reservation(
                        ReservationError::ReservationBelowMailboxThreshold
                    )))
                );

                assert_eq!(
                    ReservationId::reserve(mailbox_threshold, u32::MAX),
                    Err(Error::Ext(ExtError::Execution(
                        ExecutionError::NotEnoughGas
                    )))
                );

                assert_eq!(
                    ReservationId::reserve(u64::MAX, 1),
                    Err(Error::Ext(ExtError::Execution(
                        ExecutionError::NotEnoughGas
                    )))
                );
            }
            InitAction::FreshReserveUnreserve => {
                let id = ReservationId::reserve(10_000, 10).unwrap();
                gstd::msg::send_from_reservation(
                    id.clone(),
                    msg::source(),
                    b"fresh_reserve_unreserve",
                    0,
                )
                .unwrap();
                assert_eq!(
                    id.unreserve(),
                    Err(Error::Ext(ExtError::Reservation(
                        ReservationError::InvalidReservationId
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
            HandleAction::AddReservationToList(amount, block_count) => {
                let reservation_id =
                    ReservationId::reserve(amount, block_count).expect("Unable to reserve gas");
                unsafe {
                    RESERVATIONS.push(reservation_id);
                }
            }
            HandleAction::ConsumeReservationsFromList => {
                let reservations = unsafe { mem::take(&mut RESERVATIONS) };
                for reservation_id in reservations {
                    msg::send_from_reservation(
                        reservation_id,
                        exec::program_id(),
                        HandleAction::RunInifitely,
                        0,
                    )
                    .expect("Unable to send using reservation");
                }
            }
            HandleAction::RunInifitely => {
                if msg::source() != exec::program_id() {
                    panic!(
                        "Invalid caller, this is a private method reserved for the program itself."
                    );
                }
                loop {
                    let _msg_source = msg::source();
                }
            }
            HandleAction::SendFromReservationAndUnreserve => {
                let id = unsafe { RESERVATION_ID.take().unwrap() };
                gstd::msg::send_from_reservation(
                    id.clone(),
                    msg::source(),
                    b"existing_reserve_unreserve",
                    0,
                )
                .unwrap();
                assert_eq!(
                    id.unreserve(),
                    Err(Error::Ext(ExtError::Reservation(
                        ReservationError::InvalidReservationId
                    )))
                );
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

        let res = program.send(
            0,
            InitAction::Normal(vec![
                // orphan reservation; will be removed automatically
                (50_000, 3),
                // must be cleared during `gr_exit`
                (25_000, 5),
            ]),
        );
        assert!(!res.main_failed());
    }
}
