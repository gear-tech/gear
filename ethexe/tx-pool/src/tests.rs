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

use crate::{
    service, Transaction, InputTask, OutputTask, RawTransacton,
    SignedTransaction, TxHashBlake2b256, TxPoolKit,
};
use ethexe_db::{BlockHeader, BlockMetaStorage, Database, MemDb};
use ethexe_signer::{PrivateKey, Signer, ToDigest};
use gprimitives::{H160, H256};
use parity_scale_codec::Encode;
use std::str::FromStr;
use tokio::sync::oneshot;

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

pub(crate) fn random_block() -> (H256, BlockHeader) {
    let block_hash = H256::random();
    let header = BlockHeader {
        height: 0,
        timestamp: 0,
        parent_hash: H256::random(),
    };

    (block_hash, header)
}

#[tokio::test]
async fn test_add_transaction() {
    gear_utils::init_default_logger();

    let db = Database::from_one(&MemDb::default(), Default::default());

    let TxPoolKit {
        service,
        tx_pool_sender: input_sender,
        tx_pool_receiver: mut output_receiver,
    } = service::new(db.clone());

    // Spawn the service in a separate thread
    tokio::spawn(service.run());

    // -------------- Test adding valid transaction --------------

    // Prepare the database by populating it with blocks
    let block_data = random_block();
    db.set_latest_valid_block(block_data.0, block_data.1);
    let (block_hash, block_header) = random_block();
    db.set_latest_valid_block(block_hash, block_header);

    // Send the transaction to the service
    let signed_ethexe_tx = generate_signed_ethexe_tx(block_hash);
    let (response_sender, response_receiver) = oneshot::channel();
    input_sender
        .send(InputTask::AddTransaction {
            transaction: signed_ethexe_tx.clone(),
            response_sender: Some(response_sender),
        })
        .expect("failed to send input task");

    // Check received output tasks
    let validation_task = output_receiver
        .recv()
        .await
        .expect("failed to receive output task");
    let OutputTask::CheckIsExecutableTransaction {
        transaction,
        response_sender,
    } = validation_task
    else {
        // Expected validation task from TxValidator
        panic!("invalid task received - {validation_task:#?}");
    };
    assert_eq!(transaction, signed_ethexe_tx);
    response_sender.send(true).expect("failed to send response");

    // Propogation task
    let task = output_receiver
        .recv()
        .await
        .expect("failed to receive output task");
    assert!(
        matches!(task, OutputTask::PropogateTransaction { transaction } if transaction == signed_ethexe_tx)
    );
    // Execution task
    let task = output_receiver
        .recv()
        .await
        .expect("failed to receive output task");
    assert!(
        matches!(task, OutputTask::ExecuteTransaction { transaction } if transaction == signed_ethexe_tx)
    );

    // Assert response is ok
    let response = response_receiver.await.expect("failed to receive response");
    assert!(response.is_ok());

    // Check tx is in the db
    assert!(db
        .validated_transaction(signed_ethexe_tx.tx_hash())
        .is_some());

    // -------------- Test adding invalid transaction --------------

    // Populate more blocks in db
    for _ in 0..30 {
        let block_data = random_block();
        db.set_latest_valid_block(block_data.0, block_data.1);
    }

    // Rotten block hash
    let invalid_tx = generate_signed_ethexe_tx(block_hash);
    let tx_hash = invalid_tx.tx_hash();
    let (response_sender, response_receiver) = oneshot::channel();
    input_sender
        .send(InputTask::AddTransaction {
            transaction: invalid_tx,
            response_sender: Some(response_sender),
        })
        .expect("failed to send input task");

    // No need here to execute `CheckIsExecutableTransaction` output task, as validation won't reach it.

    // Check response
    let response = response_receiver.await.expect("failed to receive response");
    assert!(response.is_err());

    // Check tx isn't in the db
    assert!(db.validated_transaction(tx_hash).is_none());
}

#[tokio::test]
async fn test_pre_execution_validity() {
    gear_utils::init_default_logger();

    let db = Database::from_one(&MemDb::default(), Default::default());

    let TxPoolKit {
        service,
        tx_pool_sender: input_sender,
        tx_pool_receiver: mut output_receiver,
    } = service::new(db.clone());

    // Spawn the service in a separate thread
    tokio::spawn(service.run());

    // Prepare the database by populating it with blocks
    let block_data = random_block();
    db.set_latest_valid_block(block_data.0, block_data.1);
    let (block_hash, block_header) = random_block();
    db.set_latest_valid_block(block_hash, block_header);

    // Send add transaction task, so transaction is validated and added
    let signed_ethexe_tx = generate_signed_ethexe_tx(block_hash);
    input_sender
        .send(InputTask::AddTransaction {
            transaction: signed_ethexe_tx.clone(),
            response_sender: None,
        })
        .expect("failed to send input task");

    // In order for the tx to be added to the pool th validation task must be finished
    let validation_task = output_receiver
        .recv()
        .await
        .expect("failed to receive output task");
    let OutputTask::CheckIsExecutableTransaction {
        transaction,
        response_sender,
    } = validation_task
    else {
        // Expected validation task from TxValidator
        panic!("invalid task received - {validation_task:#?}");
    };
    assert_eq!(transaction, signed_ethexe_tx);
    response_sender.send(true).expect("failed to send response");

    // Now check existent validated transaction pre-execution validity
    let (response_sender, response_receiver) = oneshot::channel();
    input_sender
        .send(InputTask::ValidateTransaction {
            transaction: signed_ethexe_tx.clone(),
            response_sender,
        })
        .expect("failed to send input task");

    // Check response
    let response = response_receiver.await.expect("failed to receive response");
    assert!(response.is_ok());

    // Check tx isn't in the db
    assert!(db
        .validated_transaction(signed_ethexe_tx.tx_hash())
        .is_some());

    // Now make the validated transaction rotten
    for _ in 0..30 {
        let block_data = random_block();
        db.set_latest_valid_block(block_data.0, block_data.1);
    }

    // Check for the pre-execution validity of the same transaction
    let (response_sender, response_receiver) = oneshot::channel();
    input_sender
        .send(InputTask::ValidateTransaction {
            transaction: signed_ethexe_tx,
            response_sender,
        })
        .expect("failed to send input task");

    // Check response
    let response = response_receiver.await.expect("failed to receive response");
    assert!(response.is_err());
}
