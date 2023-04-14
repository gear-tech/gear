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

//! Integration tests for command `send`
use crate::common::{self, Args, Result};
use gsdk::Api;
use scale_info::scale::Encode;

#[tokio::test]
async fn test_command_send_works() -> Result<()> {
    let node = common::create_messager().await?;

    // Get balance of the testing address
    let signer = Api::new(Some(&node.ws())).await?.signer("//Alice", None)?;
    let mailbox = signer
        .api()
        .mailbox(Some(common::alice_account_id()), 10)
        .await?;
    assert_eq!(mailbox.len(), 1);

    // Send message to messager
    let dest = hex::encode(mailbox[0].0.source.0);
    let _ = node.run(Args::new("send").destination(dest).gas_limit("2000000000"))?;
    let mailbox = signer
        .api()
        .mailbox(Some(common::alice_account_id()), 10)
        .await?;
    assert_eq!(mailbox.len(), 2);
    assert!(mailbox
        .iter()
        .any(|mail| mail.0.payload.0 == messager::SEND_REPLY.encode()));

    Ok(())
}
