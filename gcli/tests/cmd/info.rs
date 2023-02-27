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
use crate::common::{self, logs, traits::Convert, Result, ALICE_SS58_ADDRESS};

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

const EXPECTED_MAILBOX: &str = r#"
    destination: "0xd43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d",
    payload: "0x",
    value: 1000000,
    details: None,
    interval: Interval {
        start: 2,
        finish: 31,
    },
}
"#;

#[tokio::test]
async fn test_action_balance_works() -> Result<()> {
    common::login_as_alice().expect("login failed");
    let mut node = common::Node::dev()?;
    node.wait(logs::gear_node::IMPORTING_BLOCKS)?;

    let output = common::gear(&["-e", &node.ws(), "info", "//Alice", "balance"])?;
    assert_eq!(EXPECTED_BALANCE.trim(), output.stdout.convert().trim());
    Ok(())
}

#[tokio::test]
async fn test_action_mailbox_works() -> Result<()> {
    let node = common::create_messager().await?;
    let output = common::gear(&["-e", &node.ws(), "info", ALICE_SS58_ADDRESS, "mailbox"])?;

    assert!(output.stdout.convert().contains(EXPECTED_MAILBOX.trim()));
    Ok(())
}
