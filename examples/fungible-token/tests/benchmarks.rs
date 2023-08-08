// This file is part of Gear.
//
// Copyright (C) 2021-2023 Gear Technologies Inc.
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

#![cfg(test)]

use demo_fungible_token::WASM_BINARY;
use ft_io::*;
use gclient::{EventProcessor, GearApi, Result};
use gear_core::ids::{MessageId, ProgramId};
use gstd::{vec, ActorId, Encode, Vec};
use rand::{rngs::StdRng, Rng, SeedableRng};
use statrs::statistics::Statistics;

/// Path to the gear node binary.
const GEAR_PATH: &str = "../../target/release/gear";

/// This constant defines the number of messages in the batch.
/// It is calculated empirically, and 25 is considered the optimal value for
/// messages in this test. If the value were larger, transactions would
/// exhaust the block limits.
const BATCH_CHUNK_SIZE: usize = 25;
const MAX_GAS_LIMIT: u64 = 250_000_000_000;

async fn send_messages_in_parallel(
    api: &GearApi,
    batch_size: usize,
    treads_number: usize,
    messages: &[(ProgramId, Vec<u8>, u64, u128)],
) -> Result<Vec<MessageId>> {
    // TODO: currently have problem with transaction priorities from one user.
    // Fix this after loader become a lib #2781
    assert_eq!(treads_number, 1);

    let step_size = treads_number * batch_size;
    let mut message_ids = vec![];
    for step in messages.chunks(step_size) {
        let tasks: Vec<_> = step
            .chunks(batch_size)
            .map(|batch| api.send_message_bytes_batch(batch.to_vec()))
            .collect();
        for res in futures::future::join_all(tasks).await {
            let (results, _) = res?;
            for res in results {
                let (msg_id, _) = res?;
                message_ids.push(msg_id);
            }
        }
    }

    Ok(message_ids)
}

/// This test runs stress-loading for the fungible token contract.
/// Its primary purpose is to benchmark memory allocator gas consumption.
/// It does not verify whether the contract or the runtime works correctly.
///
/// See [galloc optimization doc](../../../galloc/docs/optimization.md) for
/// reference.
#[ignore]
#[tokio::test]
async fn stress_test() -> Result<()> {
    let api = GearApi::dev_from_path(GEAR_PATH).await?;

    // Subscribing for events.
    let mut listener = api.subscribe().await?;

    // Checking that blocks still running.
    assert!(listener.blocks_running().await?);

    // Uploading program.
    let init_msg = InitConfig {
        name: "MyToken".to_string(),
        symbol: "MTK".to_string(),
        decimals: 18,
        initial_capacity: None,
    }
    .encode();

    let (message_id, program_id, _hash) = api
        .upload_program_bytes(WASM_BINARY.to_vec(), [137u8], init_msg, MAX_GAS_LIMIT, 0)
        .await?;

    assert!(listener.message_processed(message_id).await?.succeed());

    // Getting basic users and their actor ids.
    let users = vec!["//Alice", "//Bob"];

    // Creating batch of transactions for each user.
    let mut batch: Vec<FTAction> = vec![];

    for user in users {
        let api = api.clone().with(user)?;
        let actor_id =
            ActorId::from_slice(&api.account_id().encode()).expect("failed to create actor id");

        // Mint 1_000_000 tokens to main user
        let mint_payload = FTAction::Mint(1_000_000);
        batch.push(mint_payload);

        // Transfer 6_000 tokens to users 1-20
        for i in 1..=20u64 {
            let transfer_payload = FTAction::Transfer {
                from: actor_id,
                to: i.into(),
                amount: 6_000,
            };
            batch.push(transfer_payload);
        }

        // Check the balance of `user` and users 1-20
        let balance_payload = FTAction::BalanceOf(actor_id);
        batch.push(balance_payload);

        for i in 1..=20u64 {
            let balance_payload = FTAction::BalanceOf(i.into());
            batch.push(balance_payload);
        }

        // Users 1-20 send 1_000 tokens to users 21-40
        for i in 1..=20u64 {
            let transfer_payload = FTAction::Transfer {
                from: i.into(),
                to: (i + 20).into(),
                amount: 1_000,
            };
            batch.push(transfer_payload);
        }

        // Check the balance of users 1..20 after transfers
        for i in 1..=20u64 {
            let balance_payload = FTAction::BalanceOf(i.into());
            batch.push(balance_payload);
        }

        // Mint 10_000_000 tokens to main user
        let mint_payload = FTAction::Mint(10_000_000);
        batch.push(mint_payload);

        // Mint 5_000 tokens, transfer them to users 87-120 and check their balance
        for i in 87..=120u64 {
            let mint_payload = FTAction::Mint(5_000);
            batch.push(mint_payload);

            let transfer_payload = FTAction::Transfer {
                from: actor_id,
                to: i.into(),
                amount: 5_000,
            };
            batch.push(transfer_payload);

            let balance_payload = FTAction::BalanceOf(i.into());
            batch.push(balance_payload);
        }

        // Same as above, but for users 918-1339 and then these users send 1_000 tokens
        // to user i*2
        for i in 918..=1339u64 {
            let mint_payload = FTAction::Mint(5_000);
            batch.push(mint_payload);

            let transfer_payload = FTAction::Transfer {
                from: actor_id,
                to: i.into(),
                amount: 5_000,
            };
            batch.push(transfer_payload);

            let transfer_payload = FTAction::Transfer {
                from: i.into(),
                to: (i * 2).into(),
                amount: i as u128 / 4u128,
            };
            batch.push(transfer_payload);
        }
    }

    // Converting batch
    let batch: Vec<(_, Vec<u8>, u64, _)> = batch
        .iter()
        .map(|x| (program_id, x.encode(), MAX_GAS_LIMIT, 0))
        .collect();

    // Sending batch
    for chunk in batch.chunks_exact(BATCH_CHUNK_SIZE) {
        api.send_message_bytes_batch(chunk.to_vec()).await?;
    }

    Ok(())
}

#[ignore]
#[tokio::test]
async fn stress_transfer() -> Result<()> {
    let mut rng = StdRng::seed_from_u64(42);

    let api = GearApi::dev_from_path(GEAR_PATH).await?;
    // Use this code in comment for custom node run:
    // let api = GearApi::dev().await?.with("//Alice")?;

    // Subscribing for events.
    let mut listener = api.subscribe().await?;

    // Checking that blocks still running.
    assert!(listener.blocks_running().await?);

    // Uploading program.
    let init_msg = InitConfig {
        name: "MyToken".to_string(),
        symbol: "MTK".to_string(),
        decimals: 18,
        initial_capacity: Some(300_000),
    }
    .encode();

    let salt: u8 = rng.gen();
    let (message_id, program_id, _hash) = api
        .upload_program_bytes(WASM_BINARY.to_vec(), [salt], init_msg, MAX_GAS_LIMIT, 0)
        .await?;

    assert!(listener.message_processed(message_id).await?.succeed());

    // Fill program with test users balances
    let mut actions: Vec<FTAction> = vec![];

    actions.push(FTAction::Mint(u64::MAX as u128));

    // Add this amount of user balances in one message
    let step_size = 5_000;
    // Amount of added users in balances
    let users_amount = 250_000;

    for user_id in (1..=users_amount).step_by(step_size as usize) {
        actions.push(FTAction::TestSet(
            user_id..user_id + step_size,
            u64::MAX as u128,
        ));
    }

    let messages: Vec<(_, Vec<u8>, u64, _)> = actions
        .into_iter()
        .map(|action| (program_id, action.encode(), MAX_GAS_LIMIT, 0))
        .collect();

    let message_ids = send_messages_in_parallel(&api, BATCH_CHUNK_SIZE, 1, &messages).await?;

    // Wait until messages are not processed
    if let Some((msg_id, status)) = listener
        .message_processed_batch(message_ids)
        .await?
        .into_iter()
        .find(|(_, status)| !status.succeed())
    {
        panic!(
            "{msg_id:?} ended with error status: {status:?}, may be need to decrease `step_size`"
        );
    };

    // Estimate gas for one transfer action
    let mut gas_burned = Vec::new();
    for _ in 0..100 {
        let from: u64 = rng.gen_range(1..=users_amount);
        let to: u64 = rng.gen_range(1..=users_amount);
        let amount: u128 = rng.gen_range(1..=100);
        let action = FTAction::Transfer {
            from: from.into(),
            to: to.into(),
            amount,
        };
        let burned = api
            .calculate_handle_gas(None, program_id, action.encode(), 0, false)
            .await
            .unwrap()
            .burned;
        gas_burned.push(burned as f64);
    }

    println!(
        "\n===================\n
        Gas burned for one transfer operation = {} * 10^9. \
        Calculated as geometric mean from 100 transfer operations.\n",
        gas_burned.geometric_mean() / 1_000_000_000f64
    );

    Ok(())
}
