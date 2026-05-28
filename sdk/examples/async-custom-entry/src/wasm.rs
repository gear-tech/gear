// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use gstd::{ActorId, msg};

static mut USER: ActorId = ActorId::zero();

#[gstd::async_init(handle_reply = my_handle_reply, handle_signal = my_handle_signal)]
async fn init() {
    gstd::Config::set_system_reserve(10_000_000_000).expect("Failed to set system reserve");

    unsafe { USER = msg::source() }
}

#[gstd::async_main]
async fn main() {
    #[allow(clippy::empty_loop)]
    loop {}
}

fn my_handle_reply() {
    unsafe {
        msg::send_bytes(USER, b"my_handle_reply", 0).unwrap();
    }
}

fn my_handle_signal() {
    unsafe {
        msg::send_bytes(USER, b"my_handle_signal", 0).unwrap();
    }
}
