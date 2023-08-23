// This file is part of Gear.
//
// Copyright (C) 2023 Gear Technologies Inc.
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

use gear_core::ids::{CodeId, ProgramId};
use gsdk::{
    ext::{sp_core::crypto::Ss58Codec, sp_runtime::AccountId32},
    testing::Node,
    Api, Error, Result,
};
use jsonrpsee::types::error::{CallError, ErrorObject};
use parity_scale_codec::Encode;
use std::{assert_matches::assert_matches, borrow::Cow, process::Command, str::FromStr};
use subxt::{config::Header, error::RpcError, Error as SubxtError};

fn dev_node() -> Node {
    // Use release build because of performance reasons.
    let bin_path = env!("CARGO_MANIFEST_DIR").to_owned() + "/../target/release/gear";

    #[cfg(not(feature = "vara-testing"))]
    let args = vec!["--tmp", "--dev"];
    #[cfg(feature = "vara-testing")]
    let args = vec![
        "--tmp",
        "--chain=vara-dev",
        "--alice",
        "--validator",
        "--reserved-only",
    ];

    Node::try_from_path(bin_path, args)
        .expect("Failed to start node: Maybe it isn't built with --release flag?")
}

fn node_uri(node: &Node) -> String {
    format!("ws://{}", &node.address())
}

fn alice_account_id() -> AccountId32 {
    AccountId32::from_ss58check("5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY").unwrap()
}

#[tokio::test]
async fn test_calculate_upload_gas() -> Result<()> {
    let node = dev_node();
    let api = Api::new(Some(&node_uri(&node))).await?;

    let alice: [u8; 32] = *alice_account_id().as_ref();

    api.calculate_upload_gas(
        alice.into(),
        demo_messager::WASM_BINARY.to_vec(),
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
    let signer = Api::new(Some(&node_uri(&node)))
        .await?
        .signer("//Alice", None)?;
    signer
        .calls
        .upload_code(demo_messager::WASM_BINARY.to_vec())
        .await?;

    // 2. calculate create gas and create program.
    let code_id = CodeId::generate(demo_messager::WASM_BINARY);
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
    let pid = ProgramId::generate(CodeId::generate(demo_messager::WASM_BINARY), &salt);

    // 1. upload program.
    let signer = Api::new(Some(&node_uri(&node)))
        .await?
        .signer("//Alice", None)?;

    signer
        .calls
        .upload_program(
            demo_messager::WASM_BINARY.to_vec(),
            salt,
            vec![],
            100_000_000_000,
            0,
        )
        .await?;

    assert!(signer.api().gprog(pid).await.is_ok());

    // 2. calculate handle gas and send message.
    let gas_info = signer
        .rpc
        .calculate_handle_gas(None, pid, vec![], 0, true, None)
        .await?;

    signer
        .calls
        .send_message(pid, vec![], gas_info.min_limit, 0, false)
        .await?;

    Ok(())
}

#[tokio::test]
async fn test_calculate_reply_gas() -> Result<()> {
    let node = dev_node();

    let alice: [u8; 32] = *alice_account_id().as_ref();

    let salt = vec![];
    let pid = ProgramId::generate(CodeId::generate(demo_waiter::WASM_BINARY), &salt);
    let payload = demo_waiter::Command::SendUpTo(alice.into(), 10);

    // 1. upload program.
    let signer = Api::new(Some(&node_uri(&node)))
        .await?
        .signer("//Alice", None)?;
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

    assert!(signer.api().gprog(pid).await.is_ok());

    // 2. send wait message.
    signer
        .calls
        .send_message(pid, payload.encode(), 100_000_000_000, 0, false)
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
        .send_reply(message_id, vec![], gas_info.min_limit, 0, false)
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
    let api = Api::new(Some(&node_uri(&node))).await?;
    let mut finalized_blocks = api.finalized_blocks().await?;

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
    let api = Api::new(Some("wss://archive-rpc.vara-network.io:443")).await?;

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
                wasm_blob_version_result,
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
    let pid = ProgramId::generate(CodeId::generate(demo_messager::WASM_BINARY), &salt);

    let signer = Api::new(Some(&node_uri(&node)))
        .await?
        .signer("//Alice", None)?;

    signer
        .calls
        .upload_program(
            demo_messager::WASM_BINARY.to_vec(),
            salt,
            vec![],
            100_000_000_000,
            0,
        )
        .await?;

    let program = signer.api().gprog(pid).await?;
    let rpc = signer.api().rpc();
    let last_block = rpc.block(None).await?.unwrap().block.header.number();
    let block_hash = rpc.block_hash(Some(last_block.into())).await?;
    let code = signer
        .api()
        .original_code_storage_at(program.code_hash.0.into(), block_hash)
        .await?;

    assert_eq!(code, demo_messager::WASM_BINARY.to_vec());

    Ok(())
}
