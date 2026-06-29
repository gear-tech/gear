// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use gstd::{ActorId, msg, prelude::*};

const GEAR_SAILS_EVENT: ActorId = ActorId::new([0; 32]);

const ETH_SAILS_EVENT: ActorId = ActorId::new([
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
]);

#[unsafe(no_mangle)]
extern "C" fn init() {}

#[unsafe(no_mangle)]
extern "C" fn handle() {
    let payload = msg::load_bytes().expect("Failed to load payload");
    let event_type = payload[0];

    match event_type {
        0x00 => {
            msg::send_bytes(GEAR_SAILS_EVENT, &payload[1..], msg::value())
                .expect("Failed to send event");
        }
        0xff => {
            msg::send_bytes(ETH_SAILS_EVENT, &payload[1..], msg::value())
                .expect("Failed to send event");
        }
        _ => panic!("Invalid event type: {event_type}"),
    }
}
