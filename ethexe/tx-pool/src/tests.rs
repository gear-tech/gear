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

use ethexe_db::{Database, MemDb};
use ethexe_signer::{PrivateKey, PublicKey, Signer, ToDigest};
use parity_scale_codec::{Decode, Encode};
use std::str::FromStr;

const PRIVATE_KEY: &str = "4c0883a69102937d6231471b5dbb6204fe51296170827936ea5cce4b76994b0f";

fn prepare_keys() -> (Signer, PublicKey) {
    let signer = Signer::tmp();

    let public_key = signer
        .add_key(PrivateKey::from_str(PRIVATE_KEY).expect("invalid private key"))
        .expect("key addition failed");

    (signer, public_key)
}

// #[test]
// fn test_add_transaction_tx_pool_core() {
//     let (signer, public_key) = prepare_keys();
//     let db = Database::from_one(&MemDb::default(), Default::default());
//     let tx_pool = TxPoolCore::<EthexeTransaction>::new(db.clone());

//     let message = b"hello_world";
//     println!("raw message bytes {message:?}");
//     // sha3 hash of the data
//     let message_digest = message.to_digest();
//     let signature = signer
//         .sign_digest(public_key, message_digest)
//         .expect("signing failed");
//     println!("signature bytes {:?}", signature.encode());

//     let tx = EthexeTransaction::Message {
//         raw_message: message.to_vec(),
//         signature: signature.encode(),
//     };
//     let tx_hash = tx.tx_hash();

//     // Check adding doesn't fail
//     assert!(tx_pool.add_transaction(tx.clone()).is_ok());

//     // Check transaction is in the db
//     let db_data = db.validated_transaction(tx_hash);
//     assert!(db_data.is_some());

//     // Check actual db data
//     let db_tx = EthexeTransaction::decode(&mut db_data.unwrap().as_ref()).expect("decoding failed");
//     assert_eq!(db_tx, tx);
// }
