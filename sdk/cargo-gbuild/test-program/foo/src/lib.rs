// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

#![no_std]

use gstd::msg;

#[unsafe(no_mangle)]
extern "C" fn init() {
    let payload = msg::load_bytes().expect("Failed to load payload");
    gstd::debug!("Received payload: {payload:?}");
    if payload == b"PING" {
        msg::reply_bytes("INIT_PONG", 0).expect("Failed to send reply");
    }
}

#[unsafe(no_mangle)]
extern "C" fn handle() {
    let payload = msg::load_bytes().expect("Failed to load payload");

    if payload == b"PING" {
        msg::reply_bytes("HANDLE_PONG", 0).expect("Failed to send reply");
    }
}
