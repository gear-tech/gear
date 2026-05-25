// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::InitAction;
use gstd::{exec, msg};

#[gstd::async_init]
async fn init() {
    let action = msg::load().unwrap();
    match action {
        InitAction::None => {}
        InitAction::Panic => {
            let _bytes = msg::send_for_reply(msg::source(), b"init", 0, 0)
                .unwrap()
                .await
                .unwrap();
            panic!();
        }
    }
}

#[gstd::async_main]
async fn main() {
    msg::send(msg::source(), b"handle_signal", 0).unwrap();
    exec::wait();
}
