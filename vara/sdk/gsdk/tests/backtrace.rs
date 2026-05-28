// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use gsdk::{AccountKeyring, Result, backtrace::BacktraceStatus};
use utils::dev_node;

mod utils;

#[tokio::test]
async fn transfer_backtrace() -> Result<()> {
    let (_node, api) = dev_node().await;
    let bob = AccountKeyring::Bob.to_account_id();

    let tx = api.transfer_keep_alive(bob, 42).await?;

    let backtrace = api
        .backtrace()
        .get(tx.extrinsic_hash())
        .expect("Failed to get backtrace of transfer");

    assert!(
        matches!(
            backtrace.values().collect::<Vec<_>>()[..],
            [
                BacktraceStatus::InBestBlock { .. },
                BacktraceStatus::InFinalizedBlock { .. },
            ]
        ),
        "Event backtrace mismatched"
    );
    Ok(())
}
