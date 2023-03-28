// This file is part of Gear.
//
// Copyright (C) 2021-2022 Gear Technologies Inc.
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
use crate::common::{self, logs, traits::Convert, Args, Result};

#[cfg(not(feature = "vara-testing"))]
const EXPECTED_BALANCE: &str = r#"
AccountInfo {
    nonce: 0,
    consumers: 1,
    providers: 1,
    sufficients: 0,
    data: AccountData {
        free: 1152921504606846976,
        reserved: 0,
        misc_frozen: 0,
        fee_frozen: 0,
    },
}
"#;

#[cfg(feature = "vara-testing")]
const EXPECTED_BALANCE: &str = r#"
AccountInfo {
    nonce: 0,
    consumers: 0,
    providers: 1,
    sufficients: 0,
    data: AccountData {
        free: 1000000000000000000,
        reserved: 0,
        misc_frozen: 0,
        fee_frozen: 0,
    },
}
"#;

#[cfg(feature = "vara-testing")]
const EXPECTED_MAILBOX: &str = r#"
    destination: "0xd43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d",
    payload: "0x",
    value: 0,
    details: None,
    interval: Interval {
        start: 3,
        finish: 32,
    },
}
"#;

#[cfg(not(feature = "vara-testing"))]
const EXPECTED_MAILBOX: &str = r#"
    destination: "0xd43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d",
    payload: "0x",
    value: 5000,
    details: None,
    interval: Interval {
        start: 3,
        finish: 32,
    },
}
"#;

#[tokio::test]
async fn test_action_balance_works() -> Result<()> {
    common::login_as_alice().expect("login failed");
    let mut node = common::Node::dev()?;
    node.wait(logs::gear_node::IMPORTING_BLOCKS)?;

    let output = node.run(Args::new("info").address("//Alice").action("balance"))?;
    assert_eq!(EXPECTED_BALANCE.trim(), output.stdout.convert().trim());
    Ok(())
}

#[tokio::test]
async fn test_action_mailbox_works() -> Result<()> {
    let node = common::create_messager().await?;
    let output = node.run(Args::new("info").address("//Alice").action("mailbox"))?;

    let stdout = output.stdout.convert();
    if !stdout.contains(EXPECTED_MAILBOX.trim()) {
        panic!("{stdout}")
    }
    Ok(())
}
