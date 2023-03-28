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
#![cfg(not(feature = "vara-testing"))]
use crate::common::{self, Args, Result, ALICE_SS58_ADDRESS as ADDRESS, MESSAGER_SENT_VALUE};
use gsdk::Api;

const REWARD_PER_BLOCK: u128 = 3_000_000; // 3_000 gas * 1_000 value per gas

#[tokio::test]
async fn test_command_claim_works() -> Result<()> {
    let node = common::create_messager().await?;

    // Check the mailbox of the testing account
    let signer = Api::new(Some(&node.ws())).await?.signer("//Alice", None)?;
    let mailbox = signer
        .api()
        .mailbox(Some(common::alice_account_id()), 10)
        .await?;

    assert_eq!(mailbox.len(), 1);
    let id = hex::encode(mailbox[0].0.id.0);

    // Claim value from message id
    let before = signer.api().get_balance(ADDRESS).await?;
    let _ = node.run(Args::new("claim").message_id(id))?;
    let after = signer.api().get_balance(ADDRESS).await?;

    // # TODO
    //
    // not using `//Alice` or estimating the reward
    // before this checking.
    assert_eq!(
        after.saturating_sub(before),
        MESSAGER_SENT_VALUE + REWARD_PER_BLOCK
    );

    Ok(())
}
