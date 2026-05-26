// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::{REPLY_REPLY, SEND_REPLY};
use gstd::msg;

#[unsafe(no_mangle)]
extern "C" fn init() {
    let value = msg::load().unwrap_or(0);
    msg::send_bytes(msg::source(), [], value).expect("Failed to send message");
}

#[unsafe(no_mangle)]
extern "C" fn handle() {
    msg::send(msg::source(), SEND_REPLY, 0).unwrap();
}

#[unsafe(no_mangle)]
extern "C" fn handle_reply() {
    msg::send(msg::source(), REPLY_REPLY, 0).unwrap();
}
