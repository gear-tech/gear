// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::Action;
use gstd::{Reservations, msg, prelude::*};

static mut RESERVATIONS: Reservations = Reservations::new();

#[unsafe(no_mangle)]
extern "C" fn handle() {
    let action: Action = msg::load().expect("Failed to load message");

    unsafe {
        match action {
            Action::Reserve { amount, duration } => {
                static_mut!(RESERVATIONS)
                    .reserve(amount, duration)
                    .expect("Failed to reserve gas");
            }
            Action::SendMessageFromReservation { gas_amount } => {
                let reservation = static_mut!(RESERVATIONS).try_take_reservation(gas_amount);
                if let Some(reservation) = reservation {
                    msg::send_bytes_from_reservation(reservation.id(), msg::source(), [], 0)
                        .expect("Failed to send message from reservation");
                } else {
                    panic!("Reservation not found");
                }
            }
        }
    }
}
