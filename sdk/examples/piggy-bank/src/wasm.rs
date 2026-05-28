// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use gstd::{debug, exec, msg};

#[unsafe(no_mangle)]
extern "C" fn handle() {
    msg::with_read_on_stack_or_heap(|msg| {
        let available_value = exec::value_available();
        let value = msg::value();
        let payload = msg.expect("Failed to load payload bytes");
        debug!("inserted: {value}, total: {available_value}");

        if payload == b"smash" {
            debug!("smashing, total: {available_value}");
            msg::send(msg::source(), b"send", available_value).unwrap();
        } else if payload == b"smash_with_reply" {
            debug!("smashing with reply, total: {available_value}");
            msg::reply(b"reply", available_value).unwrap();
        }
    });
}
