// This file is part of Gear.
//
// Copyright (C) 2023-2024 Gear Technologies Inc.
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

//! Requires node to be built in release mode

use gear_core::{
    ids::{prelude::*, CodeId, ProgramId},
    message::ReplyInfo,
};
use gear_core_errors::{ReplyCode, SuccessReplyReason};
use gsdk::{Api, Error, Result};
use jsonrpsee::types::error::{CallError, ErrorObject};
use parity_scale_codec::Encode;
use std::{borrow::Cow, process::Command, str::FromStr, time::Instant};
use subxt::{error::RpcError, utils::H256, Error as SubxtError};
use utils::{alice_account_id, dev_node};

mod utils;

#[tokio::test]
async fn pallet_errors_formatting() -> Result<()> {
    let node = dev_node();
    let api = Api::new(Some(&node.ws())).await?;

    let err = api
        .calculate_upload_gas(
            [0u8; 32].into(),
            /* invalid code */ vec![],
            vec![],
            0,
            true,
            None,
        )
        .await
        .expect_err("Must return error");

    let expected_err = Error::Subxt(SubxtError::Rpc(RpcError::ClientError(Box::new(
        CallError::Custom(ErrorObject::owned(
            8000,
            "Runtime error",
            Some("Extrinsic `gear.upload_program` failed: 'ProgramConstructionFailed'"),
        )),
    ))));

    assert_eq!(format!("{err}"), format!("{expected_err}"));

    Ok(())
}

#[tokio::test]
async fn test_calculate_upload_gas() -> Result<()> {
    let node = dev_node();
    let api = Api::new(Some(&node.ws())).await?;

    let alice: [u8; 32] = *alice_account_id().as_ref();

    api.calculate_upload_gas(
        alice.into(),
        demo_messenger::WASM_BINARY.to_vec(),
        vec![],
        0,
        true,
        None,
    )
    .await?;

    Ok(())
}

#[tokio::test]
async fn test_calculate_create_gas() -> Result<()> {
    let node = dev_node();

    // 1. upload code.
    let signer = Api::new(Some(&node.ws())).await?.signer("//Alice", None)?;
    signer
        .calls
        .upload_code(demo_messenger::WASM_BINARY.to_vec())
        .await?;

    // 2. calculate create gas and create program.
    let code_id = CodeId::generate(demo_messenger::WASM_BINARY);
    let gas_info = signer
        .rpc
        .calculate_create_gas(None, code_id, vec![], 0, true, None)
        .await?;

    signer
        .calls
        .create_program(code_id, vec![], vec![], gas_info.min_limit, 0)
        .await?;

    Ok(())
}

#[tokio::test]
async fn test_calculate_handle_gas() -> Result<()> {
    let node = dev_node();

    let salt = vec![];
    let pid = ProgramId::generate_from_user(CodeId::generate(demo_messenger::WASM_BINARY), &salt);

    // 1. upload program.
    let signer = Api::new(Some(&node.ws())).await?.signer("//Alice", None)?;

    signer
        .calls
        .upload_program(
            demo_messenger::WASM_BINARY.to_vec(),
            salt,
            vec![],
            100_000_000_000,
            0,
        )
        .await?;

    assert!(
        signer.api().gprog(pid).await.is_ok(),
        "Program not exists on chain."
    );

    // 2. calculate handle gas and send message.
    let gas_info = signer
        .rpc
        .calculate_handle_gas(None, pid, vec![], 0, true, None)
        .await?;

    signer
        .calls
        .send_message(pid, vec![], gas_info.min_limit, 0)
        .await?;

    Ok(())
}

#[tokio::test]
async fn test_calculate_reply_gas() -> Result<()> {
    let node = dev_node();

    let alice: [u8; 32] = *alice_account_id().as_ref();

    let salt = vec![];

    let pid = ProgramId::generate_from_user(CodeId::generate(demo_waiter::WASM_BINARY), &salt);
    let payload = demo_waiter::Command::SendUpTo(alice, 10);

    // 1. upload program.
    let signer = Api::new(Some(&node.ws())).await?.signer("//Alice", None)?;
    signer
        .calls
        .upload_program(
            demo_waiter::WASM_BINARY.to_vec(),
            salt,
            vec![],
            100_000_000_000,
            0,
        )
        .await?;

    assert!(
        signer.api().gprog(pid).await.is_ok(),
        "Program not exists on chain"
    );

    // 2. send wait message.
    signer
        .calls
        .send_message(pid, payload.encode(), 100_000_000_000, 0)
        .await?;

    let mailbox = signer
        .api()
        .mailbox(Some(alice_account_id().clone()), 10)
        .await?;
    assert_eq!(mailbox.len(), 1);
    let message_id = mailbox[0].0.id.into();

    // 3. calculate reply gas and send reply.
    let gas_info = signer
        .rpc
        .calculate_reply_gas(None, message_id, vec![], 0, true, None)
        .await?;

    signer
        .calls
        .send_reply(message_id, vec![], gas_info.min_limit, 0)
        .await?;

    Ok(())
}

#[tokio::test]
async fn test_runtime_wasm_blob_version() -> Result<()> {
    let git_commit_hash = || -> Cow<str> {
        // This code is taken from
        // https://github.com/paritytech/substrate/blob/ae1a608c91a5da441a0ee7c26a4d5d410713580d/utils/build-script-utils/src/version.rs#L21
        let commit = if let Ok(hash) = std::env::var("SUBSTRATE_CLI_GIT_COMMIT_HASH") {
            Cow::from(hash.trim().to_owned())
        } else {
            // We deliberately set the length here to `11` to ensure that
            // the emitted hash is always of the same length; otherwise
            // it can (and will!) vary between different build environments.
            match Command::new("git")
                .args(["rev-parse", "--short=11", "HEAD"])
                .output()
            {
                Ok(o) if o.status.success() => {
                    let sha = String::from_utf8_lossy(&o.stdout).trim().to_owned();
                    Cow::from(sha)
                }
                Ok(o) => {
                    println!("cargo:warning=Git command failed with status: {}", o.status);
                    Cow::from("unknown")
                }
                Err(err) => {
                    println!("cargo:warning=Failed to execute git command: {}", err);
                    Cow::from("unknown")
                }
            }
        };
        commit
    };

    // This test relies on the fact the node has been built from the same commit hash
    // as the test has been.
    let git_commit_hash = git_commit_hash();
    assert_ne!(git_commit_hash, "unknown");

    let node = dev_node();
    let api = Api::new(Some(&node.ws())).await?;
    let mut finalized_blocks = api.subscribe_finalized_blocks().await?;

    let wasm_blob_version_1 = api.runtime_wasm_blob_version(None).await?;
    assert!(
        wasm_blob_version_1.ends_with(git_commit_hash.as_ref()),
        "The WASM blob version {} does not end with the git commit hash {}",
        wasm_blob_version_1,
        git_commit_hash
    );

    let block_hash_1 = finalized_blocks.next_events().await.unwrap()?.block_hash();
    let wasm_blob_version_2 = api.runtime_wasm_blob_version(Some(block_hash_1)).await?;
    assert_eq!(wasm_blob_version_1, wasm_blob_version_2);

    let block_hash_2 = finalized_blocks.next_events().await.unwrap()?.block_hash();
    let wasm_blob_version_3 = api.runtime_wasm_blob_version(Some(block_hash_2)).await?;
    assert_ne!(block_hash_1, block_hash_2);
    assert_eq!(wasm_blob_version_2, wasm_blob_version_3);

    Ok(())
}

#[tokio::test]
async fn test_runtime_wasm_blob_version_history() -> Result<()> {
    let api = Api::new(Some("wss://archive-rpc.vara.network:443")).await?;

    {
        let no_method_block_hash = sp_core::H256::from_str(
            "0xa84349fc30b8f2d02cc31d49fe8d4a45b6de5a3ac1f1ad975b8920b0628dd6b9",
        )
        .unwrap();

        let wasm_blob_version_result = api
            .runtime_wasm_blob_version(Some(no_method_block_hash))
            .await;

        let err = CallError::Custom(ErrorObject::owned(
            9000,
            "Unable to find WASM blob version in WASM blob",
            None::<String>,
        ));
        assert!(
            matches!(
                &wasm_blob_version_result,
                Err(Error::Subxt(SubxtError::Rpc(RpcError::ClientError(e)))) if e.to_string() == err.to_string()
            ),
            "Error does not match: {wasm_blob_version_result:?}"
        );
    }

    Ok(())
}

#[tokio::test]
async fn test_original_code_storage() -> Result<()> {
    let node = dev_node();

    let salt = vec![];
    let pid = ProgramId::generate_from_user(CodeId::generate(demo_messenger::WASM_BINARY), &salt);

    let signer = Api::new(Some(&node.ws())).await?.signer("//Alice", None)?;

    signer
        .calls
        .upload_program(
            demo_messenger::WASM_BINARY.to_vec(),
            salt,
            vec![],
            100_000_000_000,
            0,
        )
        .await?;

    let program = signer.api().gprog(pid).await?;
    let rpc = signer.api().backend();
    let block_hash = rpc.latest_finalized_block_ref().await?.hash();
    let code = signer
        .api()
        .original_code_storage_at(program.code_hash.0.into(), Some(block_hash))
        .await?;

    assert_eq!(
        code,
        demo_messenger::WASM_BINARY.to_vec(),
        "Program code mismatched"
    );

    Ok(())
}

// The test demonstrates how to query some storage at a lower level.
#[ignore]
#[tokio::test]
async fn test_program_counters() -> Result<()> {
    // let uri = String::from("wss://rpc.vara.network:443");
    // let uri = String::from("wss://archive-rpc.vara.network:443");
    let uri = String::from("wss://testnet.vara.network:443");
    // https://polkadot.js.org/apps/?rpc=wss://archive-rpc.vara.network#/explorer/query/9642000
    // let block_hash = H256::from_slice(&hex::decode("533ab8551fc1ecc812cfa4fa91d8667bfb3bdbcf64eacc5fccdbbf9b20e539a3")?);
    let instant = Instant::now();
    let (block_hash, block_number, count_program, count_active_program, count_memory_page) =
        query_program_counters(&uri, None).await?;
    println!("elapsed = {:?}", instant.elapsed());
    println!("testnet block_hash = {block_hash}, block_number = {block_number}, count_program = {count_program}, count_active_program = {count_active_program}, count_memory_page = {count_memory_page}");

    Ok(())
}

#[tokio::test]
async fn test_calculate_reply_for_handle() -> Result<()> {
    let node = dev_node();

    let salt = vec![];
    let pid = ProgramId::generate_from_user(CodeId::generate(demo_new_meta::WASM_BINARY), &salt);

    // 1. upload program.
    let signer = Api::new(Some(&node.ws())).await?.signer("//Alice", None)?;

    signer
        .calls
        .upload_program(
            demo_new_meta::WASM_BINARY.to_vec(),
            salt,
            vec![],
            100_000_000_000,
            0,
        )
        .await?;

    assert!(
        signer.api().gprog(pid).await.is_ok(),
        "Program not exists on chain."
    );

    let message_in = demo_new_meta::MessageIn {
        id: demo_new_meta::Id {
            decimal: 1,
            hex: [1].to_vec(),
        },
    };

    let message_out = demo_new_meta::MessageOut {
        res: demo_new_meta::Wallet::test_sequence()
            .iter()
            .find(|w| w.id.decimal == message_in.id.decimal)
            .cloned(),
    };

    // 2. calculate reply for handle
    let reply_info = signer
        .rpc
        .calculate_reply_for_handle(None, pid, message_in.encode(), 100_000_000_000, 0, None)
        .await?;

    // 3. assert
    assert_eq!(
        reply_info,
        ReplyInfo {
            payload: message_out.encode(),
            value: 0,
            code: ReplyCode::Success(SuccessReplyReason::Manual)
        }
    );

    Ok(())
}

#[tokio::test]
async fn test_calculate_reply_for_handle_does_not_change_state() -> Result<()> {
    let node = dev_node();

    let salt = vec![];
    let pid = ProgramId::generate_from_user(CodeId::generate(demo_vec::WASM_BINARY), &salt);

    // 1. upload program.
    let signer = Api::new(Some(&node.ws())).await?.signer("//Alice", None)?;

    signer
        .calls
        .upload_program(
            demo_vec::WASM_BINARY.to_vec(),
            salt,
            vec![],
            100_000_000_000,
            0,
        )
        .await?;

    assert!(
        signer.api().gprog(pid).await.is_ok(),
        "Program not exists on chain."
    );

    // 2. read initial state
    let pid_h256 = H256::from_slice(pid.as_ref());
    let initial_state = signer.api().read_state(pid_h256, vec![], None).await?;

    // 3. calculate reply for handle
    let reply_info = signer
        .rpc
        .calculate_reply_for_handle(None, pid, 42i32.encode(), 100_000_000_000, 0, None)
        .await?;

    // 4. assert that calculated result correct
    assert_eq!(
        reply_info,
        ReplyInfo {
            payload: 42i32.encode(),
            value: 0,
            code: ReplyCode::Success(SuccessReplyReason::Manual)
        }
    );

    // 5. read state after calculate
    let calcualted_state = signer.api().read_state(pid_h256, vec![], None).await?;

    // 6. assert that state hasn't changed
    assert_eq!(initial_state, calcualted_state);

    // 7. make call
    signer
        .calls
        .send_message(pid, 42i32.encode(), 100_000_000_000, 0)
        .await?;

    // 8. read state after call
    let updated_state = signer.api().read_state(pid_h256, vec![], None).await?;

    // 9. assert that state has changed
    assert_ne!(initial_state, updated_state);

    Ok(())
}

async fn query_program_counters(
    uri: &str,
    block_hash: Option<H256>,
) -> Result<(H256, u32, u64, u64, u64)> {
    use gsdk::{
        metadata::{runtime_types::gear_core::program::Program, storage::GearProgramStorage},
        BlockNumber,
    };
    use parity_scale_codec::Decode;
    use subxt::dynamic::Value;

    let signer = Api::new(Some(uri)).await?.signer("//Alice", None)?;

    let client_block = signer.api().blocks();
    let (block_hash, block_number) = match block_hash {
        Some(hash) => {
            let block = client_block.at(hash).await?;
            assert_eq!(hash, block.hash(), "block hash mismatched");

            (hash, block.number())
        }

        None => {
            let latest_block = client_block.at_latest().await?;

            (latest_block.hash(), latest_block.number())
        }
    };

    let storage = signer.api().get_storage(Some(block_hash)).await?;
    let addr = Api::storage(GearProgramStorage::ProgramStorage, Vec::<Value>::new());

    let mut iter = storage.iter(addr).await?;
    let mut count_memory_page = 0u64;
    let mut count_program = 0u64;
    let mut count_active_program = 0u64;
    while let Some(Ok((_key, value))) = iter.next().await {
        let program = Program::<BlockNumber>::decode(&mut value.encoded())?;
        count_program += 1;

        if let Program::Active(p) = program {
            count_active_program += 1;
            count_memory_page += p
                .pages_with_data
                .inner
                .iter()
                .flat_map(|(start, end)| start.0..=end.0)
                .count() as u64;
        }
    }

    Ok((
        block_hash,
        block_number,
        count_program,
        count_active_program,
        count_memory_page,
    ))
}
