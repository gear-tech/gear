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
use demo_ethereum_light_client::{primitives::U64, ArkScale, Array512, BeaconBlock, BeaconBlockBody, BeaconBlockBodyLight, BeaconBlockHeader, Bytes32, Handle, Hash256, Header, Init, SyncAggregate, SyncCommittee, SyncCommittee2, SLOTS_PER_EPOCH, WASM_BINARY, tree_hash::TreeHash};
use gclient::{EventListener, EventProcessor, GearApi, Result};
use gstd::prelude::*;
use serde::{Deserialize, de::DeserializeOwned};
use eyre::Result as EyreResult;
use ssz_rs::{List, Merkleized, Node, Serialize};
use std::cmp;
use futures::FutureExt;

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
struct BeaconBlockHeaderResponse {
    data: BeaconBlockHeaderData,
}

#[derive(serde::Deserialize, Debug)]
struct BeaconBlockHeaderData {
    header: BeaconBlockHeader2,
}

#[derive(serde::Deserialize, Debug)]
struct BeaconBlockHeader2 {
    message: BeaconBlockHeader,
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

    Ok(serde_json::from_slice::<R>(&bytes)?
        // .map_err(|e| eyre::Report::msg(format!("{}; req = {req}", e.to_string())))?
    )
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

async fn get_block_header(slot: u64) -> EyreResult<BeaconBlockHeader> {
    let req = format!("{RPC_URL}/eth/v1/beacon/headers/{slot}");

    let res: BeaconBlockHeaderResponse = get(&req).await.map_err(|e| {
        println!("get_block_body: {e:?}");

        e
    })?;

    Ok(res.data.header.message)
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

    let finalized_header = BeaconBlockHeader {
        slot: update.finalized_header.slot.into(),
        proposer_index: update.finalized_header.proposer_index.into(),
        parent_root: Hash256::from_slice(update.finalized_header.parent_root.as_ref()),
        state_root: Hash256::from_slice(update.finalized_header.state_root.as_ref()),
        body_root: Hash256::from_slice(update.finalized_header.body_root.as_ref()),
    };
    println!("finalized_header = {finalized_header:?}");

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
        finalized_header,
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
    // 0x865cdce6683d857e1f0458376ffef6790e9dfd6cddbba0d898c1d3b790bdcca6
    // let checkpoint = [134, 92, 220, 230, 104, 61, 133, 126, 31, 4, 88, 55, 111, 254, 246, 121, 14, 157, 253, 108, 221, 187, 160, 216, 152, 193, 211, 183, 144, 189, 204, 166];
    // 0x3bdc4be1121cb51c01bc9e430309ad6df9720336abde88b64840350270c62e9c
    let checkpoint = [59, 220, 75, 225, 18, 28, 181, 28, 1, 188, 158, 67, 3, 9, 173, 109, 249, 114, 3, 54, 171, 222, 136, 182, 72, 64, 53, 2, 112, 198, 46, 156];
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

    println!("program_id = {:?}", hex::encode(&program_id));

    send_blocks(&client, &mut listener, program_id, bootstrap.header.slot.into()).await?;

    return Ok(());

    let current_period = demo_ethereum_light_client::calc_sync_period(bootstrap.header.slot.into());
    let updates = get_updates(current_period, MAX_REQUEST_LIGHT_CLIENT_UPDATES)
        .await
        .map_err(|e| anyhow::Error::msg(e.to_string()))?;

    let slots_update = updates
        .iter()
        .map(|u| u.finalized_header.slot)
        .collect::<Vec<_>>();
    println!("bootstrap slot = {:?}, updates len = {}, slots_update = {slots_update:?}", bootstrap.header.slot, updates.len());

    for update in updates {
        let slot = update.finalized_header.slot;
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

    println!();
    println!("before loop");
    println!();

    for _ in 0..1_000 {
        let update = get_finality_update()
            .await
            .map_err(|e| anyhow::Error::msg(e.to_string()))?;

        let slot: u64 = update.finalized_header.slot.into();
        let current_period = demo_ethereum_light_client::calc_sync_period(slot);
        let mut updates = get_updates(current_period, 1)
            .await
            .map_err(|e| anyhow::Error::msg(e.to_string()))?;
        match updates.pop() {
            Some(update) if updates.is_empty() && update.finalized_header.slot >= slot.into() => {
                println!("update sync committee");
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

            _ => {

        println!("111 slot = {slot:?}, attested slot = {:?}, signature slot = {:?}", update.attested_header.slot, update.signature_slot);
        let signature = <G2 as ark_serialize::CanonicalDeserialize>::deserialize_compressed(update.sync_aggregate.sync_committee_signature.as_ref());
        println!("222");

        let Ok(signature) = signature else {
            println!("failed to deserialize point on G2");
            continue;
        };

        let finalized_header = BeaconBlockHeader {
            slot,
            proposer_index: update.finalized_header.proposer_index.into(),
            parent_root: Hash256::from_slice(update.finalized_header.parent_root.as_ref()),
            state_root: Hash256::from_slice(update.finalized_header.state_root.as_ref()),
            body_root: Hash256::from_slice(update.finalized_header.body_root.as_ref()),
        };
        println!("finalized_header = {finalized_header:?}");
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
            finalized_header,
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

        } // match

        send_blocks(&client, &mut listener, program_id, slot).await?;

        println!();
        println!();
    }

    Ok(())
}

async fn send_blocks(
    client: &GearApi,
    listener: &mut EventListener,
    program_id: [u8; 32],
    slot: u64,
) -> Result<()> {
    // get block for finality update
    let finality_block = get_block_body(slot)
        .await
        .map_err(|e| anyhow::Error::msg(e.to_string()))?;

    // get previous 31 blocks (header and its body)
    let mut requests = Vec::with_capacity(SLOTS_PER_EPOCH as usize);
    let mut requests_headers = Vec::with_capacity(requests.capacity());
    for i in 1 .. SLOTS_PER_EPOCH {
        let slot = slot - i;
        requests.push(get_block_body(slot)
            .map(move |r| (slot, r))
        );
        requests_headers.push(get_block_header(slot));
    }

    let mut buffer = Vec::with_capacity(10_000);

    let responses = futures::future::join_all(requests);
    let responses_headers = futures::future::join_all(requests_headers);
    let (responses, responses_headers) = futures::join!(responses, responses_headers);
    // process responses and construct the array of the related pairs
    let mut blocks = Vec::with_capacity(responses_headers.len());
    for response in responses_headers {
        let block_header = match response {
            Ok(block_header) => block_header, /*BeaconBlockHeader {
                slot: block_header.slot.into(),
                proposer_index: block_header.proposer_index.into(),
                parent_root: Hash256::from_slice(block_header.parent_root.as_ref()),
                state_root: Hash256::from_slice(block_header.state_root.as_ref()),
                body_root: Hash256::from_slice(block_header.body_root.as_ref()),
            },*/

            Err(e) => {
                println!("request failed: {e:?}");
                continue;
            }
        };

        match responses.iter().find_map(|(slot, block_body)| {
            match block_body {
                Ok(block_body) if *slot == block_header.slot =>  {
                    let mut block_body_light: BeaconBlockBodyLight = block_body.to_ref().into();

                    buffer.clear();
                    block_body_light.serialize(&mut buffer).unwrap();

                    Some(buffer.clone())
                }
                _ => None,
            }
        }) {
            Some(block_body) => blocks.push((block_header, block_body)),
            None => println!("unable to find block body for the slot {}", block_header.slot),
        }
    }

    let payload = Handle::BeaconBlockBody {
        finality_block_body: {
            let mut block_body_light: BeaconBlockBodyLight = finality_block.to_ref().into();

            buffer.clear();
            block_body_light.serialize(&mut buffer).unwrap();
            buffer.clone()
        },
        previous_blocks: blocks,
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

    Ok(())
}
