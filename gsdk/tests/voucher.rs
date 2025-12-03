// This file is part of Gear.

// Copyright (C) 2022-2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use gear_core::ids::{CodeId, prelude::CodeIdExt};
use gsdk::{Api, Result};
use sp_core::crypto::Ss58Codec;
use sp_runtime::AccountId32;
use utils::{alice_account_id, dev_node};

mod utils;

#[tokio::test]
async fn test_issue_voucher() -> Result<()> {
    // arrange
    let node = dev_node();

    let api = Api::new(node.ws().as_str())
        .await?
        .signed("//Alice", None)?;
    let account_id = alice_account_id();
    let voucher_initial_balance = 100_000_000_000_000;

    // act
    let voucher_id = api
        .issue_voucher(&account_id, voucher_initial_balance, None, false, 100)
        .await?
        .0;

    let voucher_address = AccountId32::new(voucher_id.0);
    let voucher_balance = api.unsigned().free_balance(&voucher_address).await?;

    // assert
    assert_eq!(voucher_initial_balance, voucher_balance);

    Ok(())
}

#[tokio::test]
async fn test_decline_revoke_voucher() -> Result<()> {
    // arrange
    let node = dev_node();

    let api = Api::new(node.ws().as_str())
        .await?
        .signed("//Alice", None)?;
    let account_id = api.account_id();
    let voucher_initial_balance = 100_000_000_000_000;

    // issue voucher
    let voucher_id = api
        .issue_voucher(account_id.clone(), voucher_initial_balance, None, true, 100)
        .await?
        .0;
    let _voucher_address = AccountId32::new(voucher_id.0).to_ss58check();

    // act
    let declined_id = api.decline_voucher(voucher_id.clone()).await?.0;

    let revoked_id = api
        .revoke_voucher(account_id.clone(), voucher_id.clone())
        .await?
        .0;

    // assert
    assert_eq!(voucher_id, declined_id);
    assert_eq!(voucher_id, revoked_id);

    Ok(())
}

#[tokio::test]
async fn test_upload_code_with_voucher() -> Result<()> {
    // arrange
    let node = dev_node();

    let api = Api::new(node.ws().as_str())
        .await?
        .signed("//Alice", None)?;
    let account_id = api.account_id();
    let voucher_initial_balance = 100_000_000_000_000;
    let expected_code_id = CodeId::generate(demo_messenger::WASM_BINARY);

    // issue voucher
    let voucher_id = api
        .issue_voucher(account_id.clone(), voucher_initial_balance, None, true, 100)
        .await?
        .0;
    let voucher_address = AccountId32::new(voucher_id.0);

    // account balance before upload code
    let account_initial_balance = api.free_balance().await?;

    // act
    let code_id = api
        .upload_code_with_voucher(voucher_id, demo_messenger::WASM_BINARY.to_vec())
        .await?
        .0;

    let account_balance = api.free_balance().await?;
    let voucher_balance = api.unsigned().free_balance(&voucher_address).await?;

    // assert
    assert_eq!(expected_code_id, code_id);
    // account balance not changed
    assert_eq!(account_initial_balance, account_balance);
    // voucher balance less then initial
    assert!(voucher_balance < voucher_initial_balance);

    Ok(())
}

#[tokio::test]
async fn test_send_message_with_voucher() -> Result<()> {
    // arrange
    let node = dev_node();

    let api = Api::new(node.ws().as_str())
        .await?
        .signed("//Alice", None)?;
    let account_id = api.account_id();
    let voucher_initial_balance = 100_000_000_000_000;

    // 1. issue voucher
    let voucher_id = api
        .issue_voucher(account_id.clone(), voucher_initial_balance, None, true, 100)
        .await?
        .0;
    let voucher_address = AccountId32::new(voucher_id.0);

    // 2. upload code with voucher
    let code_id = api
        .upload_code_with_voucher(voucher_id.clone(), demo_messenger::WASM_BINARY.to_vec())
        .await?
        .0;

    // 3. calculate create gas and create program
    let gas_info = api
        .calculate_create_gas(code_id, vec![], 0, true, None)
        .await?;
    let program_id = api
        .create_program_bytes(code_id, vec![], vec![], gas_info.min_limit, 0)
        .await?
        .1;

    // 4. calculate handle gas and send message with voucher
    let account_before_balance = api.free_balance().await?;
    let voucher_before_balance = api.unsigned().free_balance(&voucher_address).await?;

    let gas_info = api
        .calculate_handle_gas(program_id, vec![], 0, true, None)
        .await?;
    api.send_message_with_voucher(
        voucher_id.clone(),
        program_id,
        vec![],
        gas_info.min_limit,
        0,
        false,
    )
    .await?;

    let account_after_balance = api.free_balance().await?;
    let voucher_after_balance = api.unsigned().free_balance(&voucher_address).await?;

    // assert
    // account balance remain unchanged
    assert_eq!(account_before_balance, account_after_balance);
    // voucher balance changed
    assert!(voucher_before_balance > voucher_after_balance);

    Ok(())
}
