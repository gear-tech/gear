// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use gstd::{msg, prelude::*};

#[unsafe(no_mangle)]
extern "C" fn init() {
    gstd::debug!("Init is live!");

    let payload = msg::load_bytes().expect("Failed to load payload");

    gstd::debug!("Received payload: {payload:?}");
    if payload == b"PING" {
        msg::reply_bytes("PONG", 0).expect("Failed to send reply");
    }
}

#[unsafe(no_mangle)]
extern "C" fn handle() {
    let payload = msg::load_bytes().expect("Failed to load payload");

    if payload == b"PING" {
        msg::reply_bytes("PONG", 0).expect("Failed to send reply");
    }
}
