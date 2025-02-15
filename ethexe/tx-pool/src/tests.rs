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
use ethexe_db::{BlockHeader, BlockMetaStorage, Database, MemDb};
use ethexe_signer::{PrivateKey, Signer, ToDigest};
use gprimitives::{H160, H256};
use parity_scale_codec::Encode;
use std::str::FromStr;

pub(crate) fn generate_signed_ethexe_tx(reference_block_hash: H256) -> SignedOffchainTransaction {
    let signer = Signer::tmp();
    let public_key = signer
        .add_key(
            PrivateKey::from_str(
                "4c0883a69102937d6231471b5dbb6204fe51296170827936ea5cce4b76994b0f",
            )
            .expect("invalid private key"),
        )
        .expect("key addition failed");

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
async fn test_add_transaction() {
    gear_utils::init_default_logger();

    let db = Database::from_one(&MemDb::default(), Default::default());

    let tx_pool = TxPoolService::new(db.clone());

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
    assert!(tx_pool.validate(signed_ethexe_tx.clone()).is_ok());

    // -------------- Test adding invalid transaction --------------

    // Populate more blocks in db
    let mut block_hash = tx_reference_block_hash;
    for _ in 0..32 {
        let block_data = new_block(Some(block_hash));
        db.set_block_header(block_data.0, block_data.1.clone());
        db.set_latest_valid_block(block_data.0, block_data.1);
        block_hash = block_data.0;
    }

    // Rotten block hash
    let invalid_tx = generate_signed_ethexe_tx(tx_reference_block_hash);
    let res = tx_pool.validate(invalid_tx.clone());
    assert!(res.is_err());
    let err_string = format!("{:?}", res.expect_err("checked"));
    println!("{}", err_string);
    assert!(err_string.contains("Reference block isn't within recent blocks window"));
}
