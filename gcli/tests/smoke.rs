// This file is part of Gear.
//
// Copyright (C) 2021-2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

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
