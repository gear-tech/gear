// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use gstd::{ActorId, msg};

#[gstd::async_init]
async fn init() {
    let value_receiver: ActorId = msg::load().unwrap();

    msg::send_bytes_with_gas(value_receiver, [], 50_000, 1_000).unwrap();
    msg::send_bytes_with_gas_for_reply(msg::source(), [], 30_000, 0, 0)
        .unwrap()
        .exactly(Some(super::reply_duration()))
        .unwrap()
        .await
        .expect("Failed to send message");
    panic!();
}
