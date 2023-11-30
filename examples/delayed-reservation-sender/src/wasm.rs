// This file is part of Gear.

// Copyright (C) 2023 Gear Technologies Inc.
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

//! This contract calls [`send_bytes_delayed_from_reservation()`] with an empty payload and a
//! reservation id created by the input provided to the contract. It can either instantly call
//! the send syscall, or it can be asked to [`wait_for()`](exec::wait_for) a given amount of blocks
//! before calling it.
//!
//! [`send_bytes_delayed_from_reservation()`]: msg::send_bytes_delayed_from_reservation

use crate::{ReservationSendingShowcase, SENDING_EXPECT};
use gstd::{exec, msg, prelude::*, ReservationId};

static mut CALLED_BEFORE: bool = false;
static mut RESERVATION_ID: Option<ReservationId> = None;

#[no_mangle]
extern "C" fn handle() {
    let showcase = msg::load().expect("Failed to load request");

    match showcase {
        ReservationSendingShowcase::ToSourceInPlace {
            reservation_amount,
            reservation_delay,
            sending_delay,
        } => {
            let reservation_id = ReservationId::reserve(reservation_amount, reservation_delay)
                .expect("Failed to reserve gas");

            msg::send_bytes_delayed_from_reservation(
                reservation_id,
                msg::source(),
                [],
                0,
                sending_delay,
            )
            .expect(SENDING_EXPECT);
        }
        ReservationSendingShowcase::ToSourceAfterWait {
            reservation_amount,
            reservation_delay,
            wait_for,
            sending_delay,
        } => {
            if unsafe { !CALLED_BEFORE } {
                let reservation_id = ReservationId::reserve(reservation_amount, reservation_delay)
                    .expect("Failed to reserve gas");

                unsafe {
                    CALLED_BEFORE = true;
                    RESERVATION_ID = Some(reservation_id);
                }

                exec::wait_for(wait_for);
            }

            msg::send_bytes_delayed_from_reservation(
                unsafe { RESERVATION_ID.expect("Unset") },
                msg::source(),
                [],
                0,
                sending_delay,
            )
            .expect(SENDING_EXPECT);
        }
    }
}
