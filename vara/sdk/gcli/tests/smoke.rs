// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Smoke test for `gcli`.

use anyhow::Result;
use indoc::{formatdoc, indoc};

mod util;

#[tokio::test]
async fn smoke_test() -> Result<()> {
    let (node, gcli) = util::init_node()?;

    let node_port = node.address.port();

    gcli()
        .args(["config", "set", "endpoint", &node.ws()])
        .assert()
        .success()
        .stdout_eq(formatdoc!(
            "
            Successfully updated the configuration

            RPC URL: ws://127.0.0.1:{node_port}/
            "
        ));
    gcli().args(["wallet", "dev"]).assert().success();
    gcli().args(["info", "balance"]).assert().stdout_eq(indoc!(
        "
        Free balance: 1000000000000000000000
        "
    ));

    Ok(())
}
