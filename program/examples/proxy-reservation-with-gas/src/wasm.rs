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

use crate::InputArgs;
use gstd::{ActorId, ReservationId, msg, prelude::*};

static mut DESTINATION: ActorId = ActorId::new([0u8; 32]);
static mut DELAY: u32 = 0;
static mut RESERVATION_AMOUNT: u64 = 0;

#[unsafe(no_mangle)]
extern "C" fn handle() {
    let reservation_id =
        ReservationId::reserve(unsafe { RESERVATION_AMOUNT }, 80).expect("Failed to reserve gas");
    msg::send_delayed_from_reservation(
        reservation_id,
        unsafe { DESTINATION },
        b"proxied message",
        msg::value(),
        unsafe { DELAY },
    )
    .expect("Failed to proxy message");
}

#[unsafe(no_mangle)]
extern "C" fn init() {
    let args: InputArgs = msg::load().expect("Failed to decode `InputArgs'");
    unsafe {
        DESTINATION = args.destination;
        DELAY = args.delay;
        RESERVATION_AMOUNT = args.reservation_amount;
    }
}
