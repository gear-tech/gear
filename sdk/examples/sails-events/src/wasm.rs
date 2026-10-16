// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use gstd::{ActorId, msg, prelude::*};

#[unsafe(no_mangle)]
extern "C" fn init() {}

#[unsafe(no_mangle)]
extern "C" fn handle() {
    let payload = msg::load_bytes().expect("Failed to load payload");
    let event_type = payload[0];

    match event_type {
        0x00 => {
            msg::send_bytes(ActorId::gear_sails_event(), &payload[1..], msg::value())
                .expect("Failed to send event");
        }
        0xff => {
            msg::send_bytes(ActorId::eth_sails_event(), &payload[1..], msg::value())
                .expect("Failed to send event");
        }
        _ => panic!("Invalid event type: {event_type}"),
    }
}
