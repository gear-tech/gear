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

//! Integration tests for command `deploy`

use crate::common::{NodeExec, create_messenger, dev};
use anyhow::Result;

#[tokio::test]
async fn test_action_balance_works() -> Result<()> {
    let node = dev().await?;

    let output = node.gcli(["info", "//Alice", "balance"]).await?;
    let stdout = String::from_utf8(output.stdout)?;

    const EXPECTED_BALANCE: &str = "Free balance: 1000000000000000000";
    assert!(
        stdout.contains(EXPECTED_BALANCE),
        "Wrong balance. Expected contains:\n{EXPECTED_BALANCE}\nGot:\n{stdout}",
    );
    Ok(())
}

#[tokio::test]
async fn test_action_mailbox_works() -> Result<()> {
    let node = create_messenger().await?;
    let output = node.gcli(["info", "//Alice", "mailbox"]).await?;

    let expected_mailbox: String = [
        "destination: 0xd43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d",
        "payload: 0x",
        "value: 0",
    ]
    .join("\n");
    if !str::from_utf8(&output.stdout)?.contains(&expected_mailbox) {
        panic!(
            "Wrong mailbox response. Expected:\n{expected_mailbox}\nGot:\n{}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
    Ok(())
}
