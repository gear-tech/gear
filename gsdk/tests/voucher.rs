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

use gear_core::ids::{ActorId, CodeId, MessageId, prelude::CodeIdExt};
use gsdk::{
    Api, Event, Result, TxInBlock,
    metadata::runtime_types::pallet_gear_voucher::internal::VoucherId,
};
use sp_core::crypto::Ss58Codec;
use sp_runtime::AccountId32;
use utils::{alice_account_id, dev_node};

mod utils;

#[tokio::test]
async fn test_issue_voucher() -> Result<()> {
    // arrange
    let node = dev_node();

    let signer = Api::new(node.ws().as_str())
        .await?
        .signer("//Alice", None)?;
    let account_id = alice_account_id();
    let voucher_initial_balance = 100_000_000_000_000;

    // act
    let tx = signer
        .calls
        .issue_voucher(
            account_id.clone(),
            voucher_initial_balance,
            None,
            false,
            100,
        )
        .await?;

    let voucher_id = get_issued_voucher_id(tx).await?;
    let voucher_address = AccountId32::new(voucher_id.0).to_ss58check();
    let voucher_balance = signer.api().get_balance(&voucher_address).await?;

    // assert
    assert_eq!(voucher_initial_balance, voucher_balance);

    Ok(())
}

#[tokio::test]
async fn test_decline_revoke_voucher() -> Result<()> {
    // arrange
    let node = dev_node();

    let signer = Api::new(node.ws().as_str())
        .await?
        .signer("//Alice", None)?;
    let account_id = signer.account_id();
    let voucher_initial_balance = 100_000_000_000_000;

    // issue voucher
    let tx = signer
        .calls
        .issue_voucher(account_id.clone(), voucher_initial_balance, None, true, 100)
        .await?;
    let voucher_id = get_issued_voucher_id(tx).await?;
    let _voucher_address = AccountId32::new(voucher_id.0).to_ss58check();

    // act
    let tx = signer.calls.decline_voucher(voucher_id.clone()).await?;
    let declined_id = get_declined_voucher_id(tx).await?;

    let tx = signer
        .calls
        .revoke_voucher(account_id.clone(), voucher_id.clone())
        .await?;
    let revoked_id = get_revoked_voucher_id(tx).await?;

    // assert
    assert_eq!(voucher_id, declined_id);
    assert_eq!(voucher_id, revoked_id);

    Ok(())
}

#[tokio::test]
async fn test_upload_code_with_voucher() -> Result<()> {
    // arrange
    let node = dev_node();

    let signer = Api::new(node.ws().as_str())
        .await?
        .signer("//Alice", None)?;
    let account_id = signer.account_id();
    let voucher_initial_balance = 100_000_000_000_000;
    let expected_code_id = CodeId::generate(demo_messenger::WASM_BINARY);

    // issue voucher
    let tx = signer
        .calls
        .issue_voucher(account_id.clone(), voucher_initial_balance, None, true, 100)
        .await?;
    let voucher_id = get_issued_voucher_id(tx).await?;
    let voucher_address = AccountId32::new(voucher_id.0).to_ss58check();

    // account balance before upload code
    let account_initial_balance = signer.rpc.get_balance().await?;

    // act
    let tx = signer
        .calls
        .upload_code_with_voucher(voucher_id, demo_messenger::WASM_BINARY.to_vec())
        .await?;

    let code_id = get_last_code_id(tx).await?;

    let account_balance = signer.rpc.get_balance().await?;
    let voucher_balance = signer.api().get_balance(&voucher_address).await?;

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

    let signer = Api::new(node.ws().as_str())
        .await?
        .signer("//Alice", None)?;
    let account_id = signer.account_id();
    let voucher_initial_balance = 100_000_000_000_000;

    // 1. issue voucher
    let tx = signer
        .calls
        .issue_voucher(account_id.clone(), voucher_initial_balance, None, true, 100)
        .await?;
    let voucher_id = get_issued_voucher_id(tx).await?;
    let voucher_address = AccountId32::new(voucher_id.0).to_ss58check();

    // 2. upload code with voucher
    let tx = signer
        .calls
        .upload_code_with_voucher(voucher_id.clone(), demo_messenger::WASM_BINARY.to_vec())
        .await?;
    let code_id = get_last_code_id(tx).await?;

    // 3. calculate create gas and create program
    let gas_info = signer
        .rpc
        .calculate_create_gas(None, code_id, vec![], 0, true, None)
        .await?;
    let tx = signer
        .calls
        .create_program(code_id, vec![], vec![], gas_info.min_limit, 0)
        .await?;
    let program_id = get_last_program_id(tx).await?;

    // 4. calculate handle gas and send message with voucher
    let account_before_balance = signer.rpc.get_balance().await?;
    let voucher_before_balance = signer.api().get_balance(&voucher_address).await?;

    let gas_info = signer
        .rpc
        .calculate_handle_gas(None, program_id, vec![], 0, true, None)
        .await?;
    let tx = signer
        .calls
        .send_message_with_voucher(
            voucher_id.clone(),
            program_id,
            vec![],
            gas_info.min_limit,
            0,
            false,
        )
        .await?;
    let _message_id = get_last_message_id(tx).await?;

    let account_after_balance = signer.rpc.get_balance().await?;
    let voucher_after_balance = signer.api().get_balance(&voucher_address).await?;

    // assert
    // account balance remain unchanged
    assert_eq!(account_before_balance, account_after_balance);
    // voucher balance changed
    assert!(voucher_before_balance > voucher_after_balance);

    Ok(())
}

async fn get_issued_voucher_id(tx: TxInBlock) -> Result<VoucherId> {
    for event in tx.wait_for_success().await?.iter() {
        if let Event::GearVoucher(
            gsdk::metadata::runtime_types::pallet_gear_voucher::pallet::Event::VoucherIssued {
                voucher_id,
                ..
            },
        ) = event?.as_root_event::<Event>()?
        {
            dbg!(&voucher_id);
            return Ok(voucher_id);
        }
    }
    panic!("voucher not issued");
}

async fn get_last_code_id(tx: TxInBlock) -> Result<CodeId> {
    for event in tx.wait_for_success().await?.iter() {
        if let Event::Gear(
            gsdk::metadata::runtime_types::pallet_gear::pallet::Event::CodeChanged {
                id,
                change:
                    gsdk::metadata::runtime_types::gear_common::event::CodeChangeKind::Active { .. },
            },
        ) = event?.as_root_event::<Event>()?
        {
            return Ok(id.into());
        }
    }
    panic!("code not uploaded");
}

async fn get_last_program_id(tx: TxInBlock) -> Result<ActorId> {
    for event in tx.wait_for_success().await?.iter() {
        if let Event::Gear(
            gsdk::metadata::runtime_types::pallet_gear::pallet::Event::ProgramChanged {
                id,
                change:
                    gsdk::metadata::runtime_types::gear_common::event::ProgramChangeKind::ProgramSet {
                        ..
                    },
            },
        ) = event?.as_root_event::<Event>()?
        {
            return Ok(id.into());
        }
    }
    panic!("program not created");
}

async fn get_last_message_id(tx: TxInBlock) -> Result<MessageId> {
    for event in tx.wait_for_success().await?.iter() {
        if let Event::Gear(
            gsdk::metadata::runtime_types::pallet_gear::pallet::Event::MessageQueued { id, .. },
        ) = event?.as_root_event::<Event>()?
        {
            return Ok(id.into());
        }
    }
    panic!("message not sent");
}

async fn get_declined_voucher_id(tx: TxInBlock) -> Result<VoucherId> {
    for event in tx.wait_for_success().await?.iter() {
        if let Event::GearVoucher(
            gsdk::metadata::runtime_types::pallet_gear_voucher::pallet::Event::VoucherDeclined {
                voucher_id,
                ..
            },
        ) = event?.as_root_event::<Event>()?
        {
            return Ok(voucher_id);
        }
    }
    panic!("voucher not declined");
}

async fn get_revoked_voucher_id(tx: TxInBlock) -> Result<VoucherId> {
    for event in tx.wait_for_success().await?.iter() {
        if let Event::GearVoucher(
            gsdk::metadata::runtime_types::pallet_gear_voucher::pallet::Event::VoucherRevoked {
                voucher_id,
                ..
            },
        ) = event?.as_root_event::<Event>()?
        {
            return Ok(voucher_id);
        }
    }
    panic!("voucher not revoked");
}
