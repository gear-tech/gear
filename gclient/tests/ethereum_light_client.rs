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

use ark_bls12_381::{G1Projective as G1, G2Projective as G2};
use ark_serialize::CanonicalDeserialize;
use demo_ethereum_light_client::{primitives::U64, ArkScale, BeaconBlock, BeaconBlockBody, BeaconBlockBodyLight, Bytes32, Handle, Header, Init, SyncAggregate, SyncCommittee, WASM_BINARY, SyncCommittee2, Array512, BeaconBlockHeader, Hash256};
use gclient::{EventListener, EventProcessor, GearApi, Result};
use gstd::prelude::*;
use serde::{Deserialize, de::DeserializeOwned};
use eyre::Result as EyreResult;
use ssz_rs::{List, Merkleized, Node, Serialize};
use std::cmp;

// https://github.com/ethereum/consensus-specs/blob/dev/specs/altair/light-client/p2p-interface.md#configuration
pub const MAX_REQUEST_LIGHT_CLIENT_UPDATES: u8 = 128;
//const RPC_URL: &str = "http://unstable.sepolia.beacon-api.nimbus.team";
const RPC_URL: &str = "http://127.0.0.1:5052";

#[derive(Deserialize)]
#[serde(untagged)]
enum LightClientHeader {
    Unwrapped(Header),
    Wrapped(Beacon),
}

#[derive(Deserialize)]
struct Beacon {
    beacon: Header,
}

pub fn header_deserialize<'de, D>(deserializer: D) -> Result<Header, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let header: LightClientHeader = Deserialize::deserialize(deserializer)?;

    Ok(match header {
        LightClientHeader::Unwrapped(header) => header,
        LightClientHeader::Wrapped(header) => header.beacon,
    })
}

#[derive(Deserialize, Debug)]
pub struct Bootstrap {
    #[serde(deserialize_with = "header_deserialize")]
    pub header: Header,
    pub current_sync_committee: SyncCommittee,
    pub current_sync_committee_branch: Vec<Bytes32>,
}

#[derive(Deserialize, Debug)]
struct BootstrapResponse {
    data: Bootstrap,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Update {
    #[serde(deserialize_with = "header_deserialize")]
    pub attested_header: Header,
    pub next_sync_committee: SyncCommittee,
    pub next_sync_committee_branch: Vec<Bytes32>,
    #[serde(deserialize_with = "header_deserialize")]
    pub finalized_header: Header,
    pub finality_branch: Vec<Bytes32>,
    pub sync_aggregate: SyncAggregate,
    pub signature_slot: U64,
}

type UpdateResponse = Vec<UpdateData>;

#[derive(Deserialize, Debug)]
struct UpdateData {
    data: Update,
}

#[derive(serde::Deserialize, Debug)]
struct BeaconBlockResponse {
    data: BeaconBlockData,
}

#[derive(serde::Deserialize, Debug)]
struct BeaconBlockData {
    message: BeaconBlock,
}

#[derive(serde::Deserialize, Debug)]
struct FinalityUpdateResponse {
    data: FinalityUpdate,
}

#[derive(serde::Deserialize, Debug)]
pub struct FinalityUpdate {
    #[serde(deserialize_with = "header_deserialize")]
    pub attested_header: Header,
    #[serde(deserialize_with = "header_deserialize")]
    pub finalized_header: Header,
    pub finality_branch: Vec<Bytes32>,
    pub sync_aggregate: SyncAggregate,
    pub signature_slot: U64,
}

async fn get<R: DeserializeOwned>(req: &str) -> EyreResult<R> {
    let bytes = reqwest::get(req).await?.bytes().await?;

    Ok(serde_json::from_slice::<R>(&bytes)?)
}

async fn get_bootstrap(checkpoint: &str) -> EyreResult<Bootstrap> {
    let checkpoint_no_prefix = match checkpoint.starts_with("0x") {
        true => &checkpoint[2..],
        false => checkpoint,
    };

    let req = format!(
        "{RPC_URL}/eth/v1/beacon/light_client/bootstrap/0x{checkpoint_no_prefix}",
    );

    let res: BootstrapResponse = get(&req).await.map_err(|e| {
        println!("get bootstrap: {e:?}");
        e
    })?;

    Ok(res.data)
}

async fn get_updates(period: u64, count: u8) -> EyreResult<Vec<Update>> {
    let count = cmp::min(count, MAX_REQUEST_LIGHT_CLIENT_UPDATES);
    let req = format!(
        "{RPC_URL}/eth/v1/beacon/light_client/updates?start_period={}&count={}",
        period, count
    );

    let res: UpdateResponse = get(&req).await.map_err(|e| {
        println!("get updates: {e:?}");

        e
    })?;

    Ok(res.into_iter().map(|d| d.data).collect())
}

async fn get_block_body(slot: u64) -> EyreResult<BeaconBlockBody> {
    let req = format!("{RPC_URL}/eth/v2/beacon/blocks/{slot}");

    let res: BeaconBlockResponse = get(&req).await.map_err(|e| {
        println!("get_block_body: {e:?}");

        e
    })?;

    Ok(res.data.message.body)
}

async fn get_finality_update() -> EyreResult<FinalityUpdate> {
    let req = format!("{RPC_URL}/eth/v1/beacon/light_client/finality_update");
    let res: FinalityUpdateResponse = get(&req).await.map_err(|e| {
        println!("get_finality_update: {e:?}");

        e
    })?;

    Ok(res.data)
}

fn create_payload(update: Update) -> Handle {
    // println!("111");
    let signature = <G2 as ark_serialize::CanonicalDeserialize>::deserialize_compressed(update.sync_aggregate.sync_committee_signature.as_ref()).unwrap();
    // println!("222");
    // println!("signature = {signature:?}");

    // let Ok(signature) = signature else {
    //     println!("failed to deserialize point on G2");
    //     continue;
    // };

    let next_sync_committee_keys = Some({
        let pub_keys = update
            .next_sync_committee
            .pubkeys
            .as_ref()
            .iter()
            .map(|pub_key_compressed| {
                <G1 as CanonicalDeserialize>::deserialize_compressed_unchecked(&pub_key_compressed[..]).unwrap()
            })
            .collect::<Vec<_>>();

        let ark_scale: ArkScale<Vec<G1>> = pub_keys.into();

        ark_scale
    });
    let next_sync_committee = SyncCommittee2 {
        pubkeys: Array512(update
            .next_sync_committee
            .pubkeys
            .as_ref()
            .iter()
            .map(|pub_key_compressed| {
                <[u8; 48]>::try_from(pub_key_compressed.as_ref()).unwrap()
            })
            .collect::<Vec<[u8; 48]>>()
            .try_into()
            .unwrap()),
        aggregate_pubkey: <[u8; 48]>::try_from(update.next_sync_committee.aggregate_pubkey.as_ref()).unwrap(),
    };

    Handle::Update {
        update: {
            let update = demo_ethereum_light_client::Update {
                attested_header: update.attested_header,
                sync_aggregate: update.sync_aggregate,
            };

            let mut buffer = Vec::with_capacity(10_000);
            update.serialize(&mut buffer).unwrap();

            buffer
        },
        signature_slot: update.signature_slot.into(),
        next_sync_committee: Some(next_sync_committee),
        finalized_header: BeaconBlockHeader {
            slot: update.finalized_header.slot.into(),
            proposer_index: update.finalized_header.proposer_index.into(),
            parent_root: Hash256::from_slice(update.finalized_header.parent_root.as_ref()),
            state_root: Hash256::from_slice(update.finalized_header.state_root.as_ref()),
            body_root: Hash256::from_slice(update.finalized_header.body_root.as_ref()),
        },
        sync_committee_signature: signature.into(),
        next_sync_committee_keys,
        next_sync_committee_branch: Some(update
            .next_sync_committee_branch
            .iter()
            .map(|branch| <[u8; 32]>::try_from(branch.as_slice()).unwrap())
            .collect::<_>()),
        finality_branch: update
            .finality_branch
            .iter()
            .map(|branch| <[u8; 32]>::try_from(branch.as_slice()).unwrap())
            .collect::<_>(),
    }
}

async fn common_upload_program(
    client: &GearApi,
    code: Vec<u8>,
    payload: impl Encode,
) -> Result<([u8; 32], [u8; 32])> {
    let encoded_payload = payload.encode();
    let gas_limit = client
        .calculate_upload_gas(None, code.clone(), encoded_payload, 0, true)
        .await?
        .min_limit;
    println!("init gas {gas_limit:?}");
    let (message_id, program_id, _) = client
        .upload_program(
            code,
            gclient::now_micros().to_le_bytes(),
            payload,
            gas_limit,
            0,
        )
        .await?;

    Ok((message_id.into(), program_id.into()))
}

async fn upload_program(
    client: &GearApi,
    listener: &mut EventListener,
    payload: impl Encode,
) -> Result<[u8; 32]> {
    let (message_id, program_id) =
        common_upload_program(client, WASM_BINARY.to_vec(), payload).await?;

    assert!(listener
        .message_processed(message_id.into())
        .await?
        .succeed());

    Ok(program_id)
}

#[tokio::test]
async fn ethereum_light_client() -> Result<()> {
    // 0xe8897518856db6c1585a4fb26c4f2192d4fa60a48ca68e2d971b5800932428b0
    let checkpoint = [232, 137, 117, 24, 133, 109, 182, 193, 88, 90, 79, 178, 108, 79, 33, 146, 212, 250, 96, 164, 140, 166, 142, 45, 151, 27, 88, 0, 147, 36, 40, 176];
    let checkpoint_hex = hex::encode(checkpoint);
    let bootstrap = get_bootstrap(&checkpoint_hex).await.map_err(|e| anyhow::Error::msg(e.to_string()))?;
    // println!("bootstrap = {bootstrap:?}");

    let mut buffer = Vec::with_capacity(10_000);
    // let finalized_header = {
    //     let len = bootstrap.header.serialize(&mut buffer).unwrap();
    //     println!("len = {len}");

    //     buffer.clone()
    // };
    // println!("encoded = {finalized_header:?}");

//    let deser = <Header as ssz_rs::Deserialize>::deserialize(&finalized_header[..]).unwrap();
//    assert_eq!(bootstrap.header.slot, deser.slot);
//    assert_eq!(bootstrap.header.proposer_index, deser.proposer_index);
    // let current_sync_committee = {
    //     buffer.clear();
    //     bootstrap.current_sync_committee.serialize(&mut buffer).unwrap();

    //     buffer.clone()
    // };
    let current_sync_committee = SyncCommittee2 {
        pubkeys: Array512(bootstrap
            .current_sync_committee
            .pubkeys
            .as_ref()
            .iter()
            .map(|pub_key_compressed| {
                <[u8; 48]>::try_from(pub_key_compressed.as_ref()).unwrap()
            })
            .collect::<Vec<[u8; 48]>>()
            .try_into()
            .unwrap()),
        aggregate_pubkey: <[u8; 48]>::try_from(bootstrap.current_sync_committee.aggregate_pubkey.as_ref()).unwrap(),
     };
    let pub_keys = bootstrap
        .current_sync_committee
        .pubkeys
        .as_ref()
        .iter()
        .map(|pub_key_compressed| {
            <G1 as CanonicalDeserialize>::deserialize_compressed_unchecked(&pub_key_compressed[..]).unwrap()
        })
        .collect::<Vec<_>>();
    let init = Init {
        last_checkpoint: checkpoint,
        pub_keys: pub_keys.into(),
        finalized_header: BeaconBlockHeader {
            slot: bootstrap.header.slot.into(),
            proposer_index: bootstrap.header.proposer_index.into(),
            parent_root: Hash256::from_slice(bootstrap.header.parent_root.as_ref()),
            state_root: Hash256::from_slice(bootstrap.header.state_root.as_ref()),
            body_root: Hash256::from_slice(bootstrap.header.body_root.as_ref()),
        },
        current_sync_committee,
        current_sync_committee_branch: bootstrap
            .current_sync_committee_branch
            .iter()
            .map(|branch| <[u8; 32]>::try_from(branch.as_slice()).unwrap())
            .collect::<_>(),
    };

    // let client = GearApi::dev_from_path("../target/release/gear").await?;
    let client = GearApi::dev().await?;
    let mut listener = client.subscribe().await?;

    let program_id = upload_program(
        &client,
        &mut listener,
        init,
    )
    .await?;

    let current_period = demo_ethereum_light_client::calc_sync_period(bootstrap.header.slot.into());
    let updates = get_updates(current_period, MAX_REQUEST_LIGHT_CLIENT_UPDATES)
        .await
        .map_err(|e| anyhow::Error::msg(e.to_string()))?;

    for update in updates {
        let payload = create_payload(update);

        let gas_limit = client
            .calculate_handle_gas(None, program_id.into(), payload.encode(), 0, true)
            .await?
            .min_limit;
        println!("gas_limit {gas_limit:?}");

        let (message_id, _) = client
            .send_message(program_id.into(), payload, gas_limit, 0)
            .await?;

        assert!(listener.message_processed(message_id).await?.succeed());
    }

    println!();
    println!("before loop");
    println!();

    let mut last_period = current_period;
    for _ in 0..1_000 {
        let update = get_finality_update()
            .await
            .map_err(|e| anyhow::Error::msg(e.to_string()))?;

        let slot: u64 = update.finalized_header.slot.into();
        let current_period = demo_ethereum_light_client::calc_sync_period(slot);
        if current_period == last_period + 1 {
            println!("checking for sync committee update");
            let mut updates = get_updates(current_period, 1)
                .await
                .map_err(|e| anyhow::Error::msg(e.to_string()))?;
            match updates.pop() {
                Some(update) if updates.is_empty() => {
                    let payload = create_payload(update);
                    let gas_limit = client
                        .calculate_handle_gas(None, program_id.into(), payload.encode(), 0, true)
                        .await?
                        .min_limit;
                    println!("update gas_limit {gas_limit:?}");
    
                    let (message_id, _) = client
                        .send_message(program_id.into(), payload, gas_limit, 0)
                        .await?;
    
                    assert!(listener.message_processed(message_id).await?.succeed());
                }
    
                _ => ()
            }
        } else {

        println!("111 slot = {slot:?}, attested slot = {:?}, signature slot = {:?}", update.attested_header.slot, update.signature_slot);
        let signature = <G2 as ark_serialize::CanonicalDeserialize>::deserialize_compressed(update.sync_aggregate.sync_committee_signature.as_ref());
        println!("222");

        let Ok(signature) = signature else {
            println!("failed to deserialize point on G2");
            continue;
        };

        let payload = Handle::Update {
            update: {
                let update = demo_ethereum_light_client::Update {
                    attested_header: update.attested_header,
                    sync_aggregate: update.sync_aggregate,
                };

                buffer.clear();
                update.serialize(&mut buffer).unwrap();

                buffer.clone()
            },
            signature_slot: update.signature_slot.into(),
            next_sync_committee: None,
            finalized_header: BeaconBlockHeader {
                slot,
                proposer_index: update.finalized_header.proposer_index.into(),
                parent_root: Hash256::from_slice(update.finalized_header.parent_root.as_ref()),
                state_root: Hash256::from_slice(update.finalized_header.state_root.as_ref()),
                body_root: Hash256::from_slice(update.finalized_header.body_root.as_ref()),
            },
            sync_committee_signature: signature.into(),
            next_sync_committee_keys: None,
            next_sync_committee_branch: None,
            finality_branch: update
                .finality_branch
                .iter()
                .map(|branch| <[u8; 32]>::try_from(branch.as_slice()).unwrap())
                .collect::<_>(),
        };

        let gas_limit = client
            .calculate_handle_gas(None, program_id.into(), payload.encode(), 0, true)
            .await?
            .min_limit;
        println!("finality_update gas_limit {gas_limit:?}");

        let (message_id, _) = client
            .send_message(program_id.into(), payload, gas_limit, 0)
            .await?;

        assert!(listener.message_processed(message_id).await?.succeed());

        }

        // send block
        let block_body = get_block_body(slot)
            .await
            .map_err(|e| anyhow::Error::msg(e.to_string()))?;
        let block_body_light: BeaconBlockBodyLight = block_body.to_ref().into();

        let exec_payload = block_body.execution_payload().clone();

        let transactions = exec_payload.transactions();
        println!("transaction count = {}", transactions.len());
        let mut transaction_hashes: List<Node, 1_048_576> = Default::default();
        for transaction in transactions.iter() {
            transaction_hashes.push(transaction.clone().hash_tree_root().unwrap());
        }

        let payload = Handle::BeaconBlockBody {
            beacon_block_body_light: {
                buffer.clear();
                block_body_light.serialize(&mut buffer).unwrap();

                // let deserialized = BeaconBlockBodyLight::deserialize(&buffer[..]).unwrap();
                // println!("deserialized: {:?}", deserialized);
                // println!("light origin: {:?}", block_body_light);

                buffer.clone()
            },
            transaction_hashes: {
                buffer.clear();
                transaction_hashes.serialize(&mut buffer).unwrap();

                buffer.clone()
            }
        };

        let gas_limit = client
            .calculate_handle_gas(None, program_id.into(), payload.encode(), 0, true)
            .await?
            .min_limit;
        println!("send_block gas_limit {gas_limit:?}");

        let (message_id, _) = client
            .send_message(program_id.into(), payload, gas_limit, 0)
            .await?;

        assert!(listener.message_processed(message_id).await?.succeed());

        println!();
    }

    Ok(())
}
