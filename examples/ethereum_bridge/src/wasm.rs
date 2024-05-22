// This file is part of Gear.

// Copyright (C) 2024 Gear Technologies Inc.
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

use super::*;
use gstd::{ActorId, debug, Vec, msg};
use demo_ethereum_bridge_common::ETH_BRIDGE::EthToVaraTransferEvent;
use demo_ethereum_common::{
    hash_db,
    trie_db::{HashDB, Trie},
    rlp_node_codec::RlpNodeCodec,
    types,
    rlp::RlpStream,
    ethereum_types::{H256, U256},
    patricia_trie::TrieDB,
};
use alloy_sol_types::SolEvent;

struct State {
    light_client: ActorId,
    fungible_token: ActorId,
    nonce: U256,
}

static mut STATE: Option<State> = None;

#[no_mangle]
extern "C" fn init() {
    let init: Init = msg::load().expect("Unable to decode `Init` message");

    unsafe {
        STATE = Some(State {
            light_client: ActorId::from(init.light_client),
            fungible_token: ActorId::from(init.fungible_token),
            nonce: 0.into(),
        })
    }
}

#[gstd::async_main]
async fn main() {
    let message: EthToVaraEvent = msg::load().expect("Unable to decode `EthToVaraEvent`");
    let state = unsafe { STATE.as_mut() }.expect("Program is not initialized");

    let receipt = message.receipt;
    for log in &receipt.logs {
        let Ok(event) = EthToVaraTransferEvent::decode_raw_log(log.topics.iter().map(|hash| hash.0), &log.data, true) else {
            continue;
        };

        let nonce = state.nonce + U256::one();
        let nonce = ruint::aliases::U256::from_limbs_slice(nonce.as_ref());
        if event.nonceId != nonce {
            continue;
        }

        let request = demo_ethereum_light_client::Handle::GetReceiptsRoot(message.block_number).encode();
        let reply = msg::send_bytes_for_reply(state.light_client, &request, 0, 0)
            .expect("Failed to send message")
            .await
            .expect("Received error reply");
        let response = Option::<[u8; 32]>::decode(&mut reply.as_slice()).expect("Unable to decode response from eth light client");
        let Some(receipts_root) = response else {
            panic!("Receipts root for the specified block number is not found");
        };
        let receipts_root = H256(receipts_root);

        // verify Merkle-PATRICIA proof
        let mut memory_db = demo_ethereum_common::new_memory_db();
        for proof_node in message.proof {
            memory_db.insert(hash_db::EMPTY_PREFIX, &proof_node);
        }

        let trie = match TrieDB::new(&memory_db, &receipts_root) {
            Ok(trie) => trie,
            Err(e) => panic!("Unable to construct Trie: {e:?}"),
        };

        let key_db = rlp_encode_transaction_index(message.transaction_index as usize);
        let value_db = types::rlp_encode_receipt(&receipt);
        match trie.get(&key_db) {
            Ok(Some(found_value)) if found_value == value_db => {
                debug!("proof verified. Mint wrapped ETH");

                todo!()
            }

            result => panic!("proof invalid: {result:?}"),
        }
    }
}

pub fn rlp_encode_transaction_index(transaction_index: usize) -> Bytes {
    let mut rlp_stream = RlpStream::new();
    rlp_stream.append(&transaction_index);
    rlp_stream.out().to_vec()
}
