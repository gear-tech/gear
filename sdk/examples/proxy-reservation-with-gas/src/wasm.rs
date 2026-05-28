// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

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
