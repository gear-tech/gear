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

use crate::common::{self, Args, Result, TREASURY_SS58_ADDRESS, node::NodeExec};
use gsdk::{Api, gear};

const REWARD_PER_BLOCK: u128 = 300_000; // 3_000 gas * 100 value per gas

#[tokio::test]
async fn test_command_claim_works() -> Result<()> {
    let node = common::create_messenger().await?;

    // Check the mailbox of the testing account
    let signer = Api::new(node.ws().as_str())
        .await?
        .signer("//Alice//stash", None)?;

    let mailbox = signer
        .api()
        .mailbox_messages(Some(common::alice_account_id()), 10)
        .await?;

    assert_eq!(mailbox.len(), 1, "Mailbox should have 1 message");
    let id = hex::encode(mailbox[0].0.id());

    let treasury_before = signer
        .api()
        .get_balance(TREASURY_SS58_ADDRESS)
        .await
        .unwrap_or(0);

    // Claim value from message id
    let _ = node.run(Args::new("claim").message_id(id))?;

    let mailbox = signer
        .api()
        .mailbox_messages(Some(common::alice_account_id()), 10)
        .await?;

    assert!(mailbox.is_empty(), "Mailbox should be empty");

    let treasury_after = signer
        .api()
        .get_balance(TREASURY_SS58_ADDRESS)
        .await
        .unwrap_or(0);

    let treasury_gas_fee_share = signer
        .api()
        .constants()
        .at(&gear::constants().gear_bank().treasury_gas_fee_share())
        .map_err(gsdk::Error::from)?
        .0;
    let treasury_tx_fee_share = signer
        .api()
        .constants()
        .at(&gear::constants().gear_bank().treasury_tx_fee_share())
        .map_err(gsdk::Error::from)?
        .0;

    // Current settings. Check for ease of testing, otherwise
    // we need to know exact value of tx and gas payouts.
    assert_eq!(treasury_gas_fee_share, treasury_tx_fee_share);
    let treasury_fee_share = treasury_gas_fee_share;

    assert!(
        treasury_after >= treasury_before + treasury_fee_share * REWARD_PER_BLOCK,
        "Treasury income mismatched"
    );

    Ok(())
}
