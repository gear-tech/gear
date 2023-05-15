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
    Api, Result,
};
use lazy_static::lazy_static;
use parity_scale_codec::Encode;

lazy_static! {
    static ref GEAR_BIN_PATH: String =
        env!("CARGO_MANIFEST_DIR").to_owned() + "/../target/release/gear";
    static ref ALICE_ACCOUNT_ID: AccountId32 =
        AccountId32::from_ss58check("5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY").unwrap();
}

#[tokio::test]
async fn test_calculate_upload_gas() -> Result<()> {
    let args = vec!["--tmp", "--dev"];
    let node = Node::try_from_path(&*GEAR_BIN_PATH, args).unwrap();
    let uri = "ws://".to_string() + &node.address().to_string();
    let api = Api::new(Some(&uri)).await.unwrap();

    let alice: [u8; 32] = *ALICE_ACCOUNT_ID.as_ref();

    api.calculate_upload_gas(
        alice.into(),
        demo_messager::WASM_BINARY.to_vec(),
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
    let args = vec!["--tmp", "--dev"];
    let node = Node::try_from_path(&*GEAR_BIN_PATH, args).unwrap();
    let uri = "ws://".to_string() + &node.address().to_string();

    // 1. upload code.
    let signer = Api::new(Some(&uri))
        .await
        .unwrap()
        .signer("//Alice", None)
        .unwrap();
    signer
        .upload_code(demo_messager::WASM_BINARY.to_vec())
        .await
        .unwrap();

    // 2. calculate create gas and create program.
    let code_id = CodeId::generate(demo_messager::WASM_BINARY);
    let gas_info = signer
        .calculate_create_gas(None, code_id, vec![], 0, true, None)
        .await
        .unwrap();

    signer
        .create_program(code_id, vec![], vec![], gas_info.min_limit, 0)
        .await
        .unwrap();

    Ok(())
}

#[tokio::test]
async fn test_calculate_handle_gas() -> Result<()> {
    let args = vec!["--tmp", "--dev"];
    let node = Node::try_from_path(&*GEAR_BIN_PATH, args).unwrap();
    let uri = "ws://".to_string() + &node.address().to_string();

    let salt = vec![];
    let pid = ProgramId::generate(CodeId::generate(demo_messager::WASM_BINARY), &salt);

    // 1. upload program.
    let signer = Api::new(Some(&uri))
        .await
        .unwrap()
        .signer("//Alice", None)
        .unwrap();

    signer
        .upload_program(
            demo_messager::WASM_BINARY.to_vec(),
            salt,
            vec![],
            100_000_000_000,
            0,
        )
        .await
        .unwrap();

    assert!(signer.api().gprog(pid).await.is_ok());

    // 2. calculate handle gas and send message.
    let gas_info = signer
        .calculate_handle_gas(None, pid, vec![], 0, true, None)
        .await
        .unwrap();

    signer
        .send_message(pid, vec![], gas_info.min_limit, 0)
        .await
        .unwrap();

    Ok(())
}

#[tokio::test]
async fn test_calculate_reply_gas() -> Result<()> {
    let args = vec!["--tmp", "--dev"];
    let node = Node::try_from_path(&*GEAR_BIN_PATH, args).unwrap();
    let uri = "ws://".to_string() + &node.address().to_string();

    let alice: [u8; 32] = *ALICE_ACCOUNT_ID.as_ref();

    let salt = vec![];
    let pid = ProgramId::generate(CodeId::generate(demo_waiter::WASM_BINARY), &salt);
    let payload = demo_waiter::Command::SendUpTo(alice.into(), 10);

    // 1. upload program.
    let signer = Api::new(Some(&uri))
        .await
        .unwrap()
        .signer("//Alice", None)
        .unwrap();
    signer
        .upload_program(
            demo_waiter::WASM_BINARY.to_vec(),
            salt,
            vec![],
            100_000_000_000,
            0,
        )
        .await
        .unwrap();

    assert!(signer.api().gprog(pid).await.is_ok());

    // 2. send wait message.
    signer
        .send_message(pid, payload.encode(), 100_000_000_000, 0)
        .await
        .unwrap();

    let mailbox = signer
        .api()
        .mailbox(Some(ALICE_ACCOUNT_ID.clone()), 10)
        .await
        .unwrap();
    assert_eq!(mailbox.len(), 1);
    let message_id = mailbox[0].0.id.into();

    // 3. calculate reply gas and send reply.
    let gas_info = signer
        .calculate_reply_gas(None, message_id, 1, vec![], 0, true, None)
        .await
        .unwrap();

    signer
        .send_reply(message_id, vec![], gas_info.min_limit, 0)
        .await
        .unwrap();

    Ok(())
}
