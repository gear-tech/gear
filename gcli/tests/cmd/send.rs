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

//! Integration tests for command `send`
use crate::common::{NodeExec, create_messenger};
use anyhow::Result;
use gsdk::Api;
use scale_info::scale::Encode;

#[tokio::test]
async fn test_command_send_works() -> Result<()> {
    let node = create_messenger().await?;

    // Get balance of the testing address
    let api = Api::new(node.ws().as_str()).await?.signed_as_alice();
    let mailbox = api.mailbox_messages(10).await?;
    assert_eq!(mailbox.len(), 1, "Alice should have 1 message in mailbox");

    // Send message to messenger
    let dest = mailbox[0].0.source().to_string();

    node.gcli(["send", &dest, "--gas-limit", "2000000000"])
        .await?;

    let mailbox = api.mailbox_messages(10).await?;
    assert_eq!(
        mailbox.len(),
        2,
        "Alice now should have 2 messages in mailbox"
    );
    assert!(
        mailbox
            .iter()
            .any(|mail| mail.0.payload_bytes() == demo_messenger::SEND_REPLY.encode()),
        "Mailbox should have the send reply message"
    );

    Ok(())
}
