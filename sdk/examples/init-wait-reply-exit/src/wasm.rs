// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use gstd::{exec, msg, prelude::vec};

#[gstd::async_init]
async fn init() {
    msg::send_bytes_for_reply(msg::source(), vec![], 0, 0)
        .expect("send message failed")
        .await
        .expect("ran into error-reply");
    exec::exit(msg::source());
}
