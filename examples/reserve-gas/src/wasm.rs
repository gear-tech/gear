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

use crate::{
    HandleAction, InitAction, REPLY_FROM_RESERVATION_PAYLOAD, RESERVATION_AMOUNT, ReplyAction,
};
use gstd::{
    MessageId, ReservationId,
    errors::{CoreError, ExecutionError, ExtError, ReservationError},
    exec, msg,
    prelude::*,
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

#[unsafe(no_mangle)]
extern "C" fn init() {
    unsafe { INIT_MSG = msg::id() };

    let action: InitAction = msg::load().unwrap();

    match action {
        InitAction::Normal(ref reservations) => {
            for (amount, duration) in reservations {
                let _ = ReservationId::reserve(*amount, *duration).expect("reservation");
            }

            // no actual reservation and unreservation is occurred
            let noop_reservation = ReservationId::reserve(50_000, 10).expect("noop reservation");
            let unreserved_amount = noop_reservation.unreserve().expect("noop unreservation");
            assert_eq!(unreserved_amount, 50_000);

            unsafe {
                RESERVATION_ID = Some(
                    ReservationId::reserve(RESERVATION_AMOUNT, 15)
                        .expect("reservation across executions"),
                )
            };
        }
        InitAction::Wait => match unsafe { static_ref!(WAKE_STATE) } {
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
                Err(CoreError::Ext(ExtError::Reservation(
                    ReservationError::ReservationBelowMailboxThreshold
                )))
            );

            assert_eq!(
                ReservationId::reserve(50_000, 0),
                Err(CoreError::Ext(ExtError::Reservation(
                    ReservationError::ZeroReservationDuration
                )))
            );

            assert_eq!(
                ReservationId::reserve(mailbox_threshold - 1, 1),
                Err(CoreError::Ext(ExtError::Reservation(
                    ReservationError::ReservationBelowMailboxThreshold
                )))
            );

            assert_eq!(
                ReservationId::reserve(mailbox_threshold, u32::MAX),
                Err(CoreError::Ext(ExtError::Execution(
                    ExecutionError::NotEnoughGas
                )))
            );

            assert_eq!(
                ReservationId::reserve(u64::MAX, 1),
                Err(CoreError::Ext(ExtError::Execution(
                    ExecutionError::NotEnoughGas
                )))
            );
        }
        InitAction::FreshReserveUnreserve => {
            let id = ReservationId::reserve(10_000, 10).unwrap();
            gstd::msg::send_from_reservation(id, msg::source(), b"fresh_reserve_unreserve", 0)
                .unwrap();
            assert_eq!(
                id.unreserve(),
                Err(CoreError::Ext(ExtError::Reservation(
                    ReservationError::InvalidReservationId
                )))
            );
        }
    }
}

#[unsafe(no_mangle)]
extern "C" fn handle() {
    let action: HandleAction = msg::load().unwrap();
    match action {
        HandleAction::Unreserve => {
            let id = unsafe { static_mut!(RESERVATION_ID).take().unwrap() };
            id.unreserve().expect("unreservation across executions");
        }
        HandleAction::Exit => {
            exec::exit(msg::source());
        }
        HandleAction::ReplyFromReservation => {
            let id = unsafe { static_mut!(RESERVATION_ID).take().unwrap() };
            msg::reply_from_reservation(id, REPLY_FROM_RESERVATION_PAYLOAD, 0)
                .expect("unable to reply from reservation");
        }
        HandleAction::AddReservationToList(amount, block_count) => {
            let reservation_id =
                ReservationId::reserve(amount, block_count).expect("Unable to reserve gas");
            unsafe {
                static_mut!(RESERVATIONS).push(reservation_id);
            }
        }
        HandleAction::ConsumeReservationsFromList => {
            let reservations = unsafe { mem::take(static_mut!(RESERVATIONS)) };
            for reservation_id in reservations {
                msg::send_from_reservation(
                    reservation_id,
                    exec::program_id(),
                    HandleAction::RunInfinitely,
                    0,
                )
                .expect("Unable to send using reservation");
            }
        }
        HandleAction::RunInfinitely => {
            if msg::source() != exec::program_id() {
                panic!("Invalid caller, this is a private method reserved for the program itself.");
            }
            loop {
                let _msg_source = msg::source();
            }
        }
        HandleAction::SendFromReservationAndUnreserve => {
            let id = unsafe { static_mut!(RESERVATION_ID).take().unwrap() };
            gstd::msg::send_from_reservation(id, msg::source(), b"existing_reserve_unreserve", 0)
                .unwrap();
            assert_eq!(
                id.unreserve(),
                Err(CoreError::Ext(ExtError::Reservation(
                    ReservationError::InvalidReservationId
                )))
            );
        }
    }
}

// must be called after `InitAction::Wait`
#[unsafe(no_mangle)]
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
