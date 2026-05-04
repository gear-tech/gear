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
