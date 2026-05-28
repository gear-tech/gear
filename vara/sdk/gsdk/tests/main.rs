// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use gsdk::Api;
use std::time::Duration;

#[tokio::test]
async fn timeout() {
    Api::builder()
        .timeout(Duration::ZERO)
        .build()
        .await
        .expect_err("connection to RPC node with zero timeout must fail");
}
