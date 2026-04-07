// This file is part of Gear.
//
// Copyright (C) 2026 Gear Technologies Inc.
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

use ethexe_common::{
    Address,
    injected::{AddressedInjectedTransaction, InjectedTransaction},
};
use ethexe_rpc::{BlockClient as _, InjectedClient as _};
use gprimitives::H256;
use gsigner::secp256k1::{Secp256k1SignerExt as _, Signer};
use jsonrpsee::ws_client::WsClientBuilder;
use std::{str::FromStr as _, time::Instant};

#[tokio::test]
#[ignore = "requires connection to vara.network validator"]
async fn send_injected_tx_join_us() {
    const VALIDATOR_RPC_URL: &str = "wss://validator-1-eth.vara.network";
    const DESTINATION: &str = "0x6286a1f8ebbd8b7d2ab75321f3f00b507d5ecc01";
    // SCALE-encoded payload: OneOfUs::JoinUs
    const PAYLOAD: &[u8] = &[
        0x1c, 0x4f, 0x6e, 0x65, 0x4f, 0x66, 0x55, 0x73, 0x18, 0x4a, 0x6f, 0x69, 0x6e, 0x55, 0x73,
    ];

    let client = WsClientBuilder::new()
        .build(VALIDATOR_RPC_URL)
        .await
        .unwrap();

    // Get latest block hash as reference_block from ethexe RPC.
    let (reference_block, _header) = client.block_header(None).await.unwrap();

    let signer = Signer::memory();
    let key = signer.generate().unwrap();

    let tx = InjectedTransaction {
        destination: Address::from_str(DESTINATION).unwrap().into(),
        payload: PAYLOAD.to_vec().try_into().unwrap(),
        value: 0,
        reference_block,
        salt: H256::random().0.to_vec().try_into().unwrap(),
    };

    let message_id = tx.to_message_id();
    let tx_hash = tx.to_hash();
    println!("Message ID: {message_id:?}");
    println!("Tx hash: {tx_hash:?}");

    let transaction = AddressedInjectedTransaction {
        recipient: Address::default(),
        tx: signer.signed_message(key, tx, None).unwrap(),
    };

    println!("Sending transaction ...");
    let start = Instant::now();

    let mut subscription = client
        .send_transaction_and_watch(transaction)
        .await
        .unwrap();

    println!("Waiting for promise to ...");

    let promise = subscription
        .next()
        .await
        .expect("promise from subscription")
        .expect("transaction promise")
        .into_data();

    let elapsed = start.elapsed();
    println!(
        "Promise received in {:.2}s: {promise:?}",
        elapsed.as_secs_f64()
    );
}
