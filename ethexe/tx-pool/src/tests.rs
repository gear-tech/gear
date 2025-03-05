// This file is part of Gear.
//
// Copyright(C) 2025 Gear Technologies Inc.
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

use crate::{
    OffchainTransaction, RawOffchainTransaction, SignedOffchainTransaction, TxPoolService,
};
use ethexe_db::{BlockHeader, BlockMetaStorage, Database, MemDb, OnChainStorage};
use ethexe_signer::{Signer, ToDigest};
use gprimitives::{H160, H256};
use parity_scale_codec::Encode;

pub(crate) fn generate_signed_ethexe_tx(reference_block_hash: H256) -> SignedOffchainTransaction {
    let signer = Signer::tmp();
    let public_key = signer.generate_key().expect("failed to generate key");

    let transaction = OffchainTransaction {
        raw: RawOffchainTransaction::SendMessage {
            program_id: H160::random(),
            payload: vec![],
        },
        reference_block: reference_block_hash,
    };
    let signature = signer
        .sign_digest(public_key, transaction.encode().to_digest())
        .expect("signing failed");

    SignedOffchainTransaction {
        transaction,
        signature: signature.encode(),
    }
}

pub(crate) struct BlocksManager {
    db: Database,
}

impl BlocksManager {
    pub(crate) fn new(db: Database) -> Self {
        Self { db }
    }

    pub(crate) fn add_block(&self) -> (H256, BlockHeader) {
        let block_hash = H256::random();

        match self.db.latest_computed_block() {
            Some((parent_hash, parent_header)) => {
                let header = BlockHeader {
                    height: parent_header.height + 1,
                    timestamp: now(),
                    parent_hash,
                };

                self.db.set_block_header(block_hash, header.clone());
                self.db
                    .set_latest_computed_block(block_hash, header.clone());

                (block_hash, header)
            }
            None => {
                let header = BlockHeader {
                    height: 0,
                    timestamp: now(),
                    parent_hash: H256::zero(),
                };

                self.db.set_block_header(block_hash, header.clone());
                self.db
                    .set_latest_computed_block(block_hash, header.clone());

                (block_hash, header)
            }
        }
    }
}

fn now() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time went backwards")
        .as_millis() as u64
}

#[tokio::test]
async fn test_add_transaction() {
    gear_utils::init_default_logger();

    let db = Database::from_one(&MemDb::default(), Default::default());
    let bm = BlocksManager::new(db.clone());

    let tx_pool = TxPoolService::new(db);

    // -------------- Test adding a valid transaction --------------

    // Prepare the database by populating it with blocks
    bm.add_block();
    let (tx_reference_block_hash, _) = bm.add_block();

    // Add the transaction to the service
    let signed_ethexe_tx = generate_signed_ethexe_tx(tx_reference_block_hash);
    assert!(tx_pool.validate(signed_ethexe_tx.clone()).is_ok());

    // -------------- Test adding invalid transaction --------------

    // Populate more blocks in db
    (0..32).for_each(|_| {
        bm.add_block();
    });

    // Rotten block hash
    let invalid_tx = generate_signed_ethexe_tx(tx_reference_block_hash);
    let res = tx_pool.validate(invalid_tx.clone());
    assert!(res.is_err());
    let err_string = format!("{:?}", res.expect_err("checked"));
    println!("{}", err_string);
    assert!(err_string.contains("Transaction reference block hash is out of recent blocks window"));
}
