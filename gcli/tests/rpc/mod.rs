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

use crate::common::{self, logs, Result};
use gear_core::ids::CodeId;
use gsdk::Api;
use scale_info::scale::Encode;

#[tokio::test]
async fn test_calculate_upload_gas() -> Result<()> {
    let mut node = common::Node::dev()?;
    node.wait(logs::gear_node::IMPORTING_BLOCKS)?;

    let api = Api::new(Some(&node.ws())).await?;
    let alice_account_id = common::alice_account_id();
    let alice: [u8; 32] = *alice_account_id.as_ref();

    api.calculate_upload_gas(
        alice.into(),
        messager::WASM_BINARY.to_vec(),
        vec![],
        0,
        true,
        None,
    )
    .await
    .unwrap();

    Ok(())
}

#[tokio::test]
async fn test_calculate_create_gas() -> Result<()> {
    let mut node = common::Node::dev()?;
    node.wait(logs::gear_node::IMPORTING_BLOCKS)?;

    // 1. upload code.
    let signer = Api::new(Some(&node.ws())).await?.signer("//Alice", None)?;
    signer.upload_code(messager::WASM_BINARY.to_vec()).await?;

    // 2. calculate create gas and create program.
    let code_id = CodeId::generate(messager::WASM_BINARY);
    let gas_info = signer
        .calculate_create_gas(None, code_id, vec![], 0, true, None)
        .await?;

    signer
        .create_program(code_id, vec![], vec![], gas_info.min_limit, 0)
        .await
        .unwrap();

    Ok(())
}

#[tokio::test]
async fn test_calculate_handle_gas() -> Result<()> {
    let mut node = common::Node::dev()?;
    node.wait(logs::gear_node::IMPORTING_BLOCKS)?;

    let salt = vec![];
    let pid = common::program_id(messager::WASM_BINARY, &salt);

    // 1. upload program.
    let signer = Api::new(Some(&node.ws())).await?.signer("//Alice", None)?;

    signer
        .upload_program(
            messager::WASM_BINARY.to_vec(),
            salt,
            vec![],
            100_000_000_000,
            0,
        )
        .await?;

    assert!(signer.api().gprog(pid).await.is_ok());

    // 2. calculate handle gas and send message.
    let gas_info = signer
        .calculate_handle_gas(None, pid, vec![], 0, true, None)
        .await?;

    signer
        .send_message(pid, vec![], gas_info.min_limit, 0)
        .await?;

    Ok(())
}

#[tokio::test]
async fn test_calculate_reply_gas() -> Result<()> {
    let mut node = common::Node::dev()?;
    node.wait(logs::gear_node::IMPORTING_BLOCKS)?;

    let alice_account_id = common::alice_account_id();
    let alice: [u8; 32] = *alice_account_id.as_ref();
    let salt = vec![];
    let pid = common::program_id(demo_waiter::WASM_BINARY, &salt);
    let payload = demo_waiter::Command::SendUpTo(alice.into(), 10);

    // 1. upload program.
    let signer = Api::new(Some(&node.ws())).await?.signer("//Alice", None)?;
    signer
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
        .send_message(pid, payload.encode(), 100_000_000_000, 0)
        .await?;

    let mailbox = signer.api().mailbox(Some(alice_account_id), 10).await?;
    assert_eq!(mailbox.len(), 1);
    let message_id = mailbox[0].0.id.into();

    // 3. calculate reply gas and send reply.
    let gas_info = signer
        .calculate_reply_gas(None, message_id, 1, vec![], 0, true, None)
        .await?;

    signer
        .send_reply(message_id, vec![], gas_info.min_limit, 0)
        .await
        .unwrap();

    Ok(())
}
