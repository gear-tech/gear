// This file is part of Gear.
//
// Copyright (C) 2021-2024 Gear Technologies Inc.
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

use crate::common::{
    self, node::NodeExec, Args, Result, ALICE_SS58_ADDRESS as ADDRESS, RENT_POOL_SS58_ADDRESS,
};
use gsdk::Api;

const REWARD_PER_BLOCK: u128 = 18_000; // 3_000 gas * 6 value per gas

#[tokio::test]
async fn test_command_claim_works() -> Result<()> {
    // hack to check initial alice balance
    let (initial_balance, initial_stash, rent_pool_initial) = {
        let node = common::dev()?;

        // Get balance of the testing address
        let signer = Api::new(&node.ws()).await?.signer("//Alice//stash", None)?;
        (
            signer.api().get_balance(ADDRESS).await.unwrap_or(0),
            signer
                .api()
                .get_balance(&signer.address())
                .await
                .unwrap_or(0),
            signer
                .api()
                .get_balance(RENT_POOL_SS58_ADDRESS)
                .await
                .unwrap_or(0),
        )
    };

    let node = common::create_messager().await?;

    // Check the mailbox of the testing account
    let signer = Api::new(&node.ws()).await?.signer("//Alice//stash", None)?;
    let mailbox = signer
        .api()
        .mailbox(Some(common::alice_account_id()), 10)
        .await?;

    assert_eq!(mailbox.len(), 1, "Mailbox should have 1 message");
    let id = hex::encode(mailbox[0].0.id.0);

    let burned_before = signer.api().get_balance(&signer.address()).await? - initial_stash;
    let before = signer.api().get_balance(ADDRESS).await?;

    // Claim value from message id
    let _ = node.run(Args::new("claim").message_id(id))?;

    let burned_after = signer.api().get_balance(&signer.address()).await? - initial_stash;
    let after = signer.api().get_balance(ADDRESS).await?;
    let rent_pool = signer.api().get_balance(RENT_POOL_SS58_ADDRESS).await?;

    assert_eq!(
        initial_balance - before - burned_before,
        REWARD_PER_BLOCK,
        "Reward per block mismatched "
    );
    assert_eq!(
        initial_balance - burned_after - (rent_pool - rent_pool_initial),
        after,
        "Transaction spent mismatched"
    );

    Ok(())
}
