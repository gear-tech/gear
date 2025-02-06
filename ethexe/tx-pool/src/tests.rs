// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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

//! Testing module for the tx pool.
//!
//! Test here mainly focus on:
//! - the overall logic of the tx pool to work as expected
//! - the channels inside the tx pool service to work as expected

use crate::{RawTransacton, SignedTransaction, Transaction, TxPoolEvent, TxPoolService};
use ethexe_db::{BlockHeader, BlockMetaStorage, Database, MemDb};
use ethexe_signer::{PrivateKey, Signer, ToDigest};
use futures::{future::poll_fn, StreamExt};
use gprimitives::{H160, H256};
use parity_scale_codec::Encode;
use std::{str::FromStr, task::Poll};

pub(crate) fn generate_signed_ethexe_tx(reference_block_hash: H256) -> SignedTransaction {
    let signer = Signer::tmp();
    let public_key = signer
        .add_key(
            PrivateKey::from_str(
                "4c0883a69102937d6231471b5dbb6204fe51296170827936ea5cce4b76994b0f",
            )
            .expect("invalid private key"),
        )
        .expect("key addition failed");

    let transaction = Transaction {
        raw: RawTransacton::SendMessage {
            program_id: H160::random(),
            payload: vec![],
            value: 0,
        },
        reference_block: reference_block_hash,
    };
    let signature = signer
        .sign_digest(public_key, transaction.encode().to_digest())
        .expect("signing failed");

    SignedTransaction {
        transaction,
        signature: signature.encode(),
    }
}

pub(crate) fn new_block(parent_hash: Option<H256>) -> (H256, BlockHeader) {
    let block_hash = H256::random();
    let header = BlockHeader {
        height: 0,
        timestamp: 0,
        parent_hash: parent_hash.unwrap_or(H256::random()),
    };

    (block_hash, header)
}

#[tokio::test]
async fn test_pool_stream() {
    let db = Database::from_one(&MemDb::default(), Default::default());
    let mut tx_pool_service = TxPoolService::new(db.clone());

    // Prepare the database by populating it with blocks
    let (tx_reference_block_hash1, block_header1) = new_block(None);
    db.set_block_header(tx_reference_block_hash1, block_header1.clone());
    db.set_latest_valid_block(tx_reference_block_hash1, block_header1);
    let (tx_reference_block_hash2, block_header2) = new_block(Some(tx_reference_block_hash1));
    db.set_block_header(tx_reference_block_hash2, block_header2.clone());
    db.set_latest_valid_block(tx_reference_block_hash2, block_header2);

    // Create some dummy transactions
    let tx1 = generate_signed_ethexe_tx(tx_reference_block_hash1);
    let tx2 = generate_signed_ethexe_tx(tx_reference_block_hash2);

    // Process transactions
    assert!(tx_pool_service.process(tx1.clone()).is_ok());
    assert!(tx_pool_service.process(tx2.clone()).is_ok());

    // Poll next
    let Some(event) = tx_pool_service.next().await else {
        panic!("Expected tx1");
    };
    assert_eq!(event, TxPoolEvent::PropogateTransaction(tx1));

    // Poll next
    let Some(event) = tx_pool_service.next().await else {
        panic!("Expected tx2");
    };
    assert_eq!(event, TxPoolEvent::PropogateTransaction(tx2));

    // Polls when there are no ready transactions
    assert!(tx_pool_service.ready_tx.is_empty());

    poll_fn(|cx| {
        // Never returns `None`.
        let poll = tx_pool_service.poll_next_unpin(cx);
        assert_eq!(poll, Poll::Pending);

        Poll::Ready(())
    })
    .await;

    // Process another transaction
    let tx3 = generate_signed_ethexe_tx(tx_reference_block_hash1);
    assert!(tx_pool_service.process(tx3.clone()).is_ok());

    // Poll next
    let Some(event) = tx_pool_service.next().await else {
        panic!("Expected tx3");
    };
    assert_eq!(event, TxPoolEvent::PropogateTransaction(tx3));
}

#[tokio::test]
async fn test_add_transaction() {
    gear_utils::init_default_logger();

    let db = Database::from_one(&MemDb::default(), Default::default());

    let mut tx_pool = TxPoolService::new(db.clone());

    // -------------- Test adding a valid transaction --------------

    // Prepare the database by populating it with blocks
    let block_data = new_block(None);
    db.set_block_header(block_data.0, block_data.1.clone());
    db.set_latest_valid_block(block_data.0, block_data.1);
    let (tx_reference_block_hash, block_header) = new_block(Some(block_data.0));
    db.set_block_header(tx_reference_block_hash, block_header.clone());
    db.set_latest_valid_block(tx_reference_block_hash, block_header);

    // Add the transaction to the service
    let signed_ethexe_tx = generate_signed_ethexe_tx(tx_reference_block_hash);
    assert!(tx_pool.process(signed_ethexe_tx.clone()).is_ok());

    // Check propogation event
    let event = tx_pool.select_next_some().await;
    assert_eq!(
        event,
        TxPoolEvent::PropogateTransaction(signed_ethexe_tx.clone())
    );

    // -------------- Test adding invalid transaction --------------

    // Populate more blocks in db
    let mut block_hash = tx_reference_block_hash;
    for _ in 0..30 {
        let block_data = new_block(Some(block_hash));
        db.set_block_header(block_data.0, block_data.1.clone());
        db.set_latest_valid_block(block_data.0, block_data.1);
        block_hash = block_data.0;
    }

    // Rotten block hash
    let invalid_tx = generate_signed_ethexe_tx(tx_reference_block_hash);
    let res = tx_pool.process(invalid_tx.clone());
    assert!(res.is_err());
    let err_string = format!("{:?}", res.expect_err("checked"));
    assert!(err_string.contains("Transaction out of recent blocks window"));
}
