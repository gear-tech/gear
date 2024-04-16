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

use ark_bls12_381::{G1Affine, G1Projective as G1, G2Affine, G2Projective as G2};
use ark_ec::Group;
use ark_serialize::CanonicalSerialize;
use ark_std::{ops::Mul, UniformRand};
use demo_ethereum_light_client::{Header, SyncCommittee, Bytes32, Init, WASM_BINARY, primitives::U64, Handle, SignatureBytes};
use gclient::{EventListener, EventProcessor, GearApi, Result};
use gstd::prelude::*;
use serde::{Deserialize, de::DeserializeOwned};
use eyre::Result as EyreResult;
use ssz_rs::Serialize;
use std::cmp;

type ArkScale<T> = ark_scale::ArkScale<T, { ark_scale::HOST_CALL }>;
type ScalarField = <G2 as Group>::ScalarField;

// https://github.com/ethereum/consensus-specs/blob/dev/specs/altair/light-client/p2p-interface.md#configuration
pub const MAX_REQUEST_LIGHT_CLIENT_UPDATES: u8 = 128;
const RPC_URL: &str = "http://unstable.sepolia.beacon-api.nimbus.team";

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

#[derive(Deserialize, Debug, Clone, Default)]
pub struct SyncAggregate {
    pub sync_committee_bits: ssz_rs::Bitvector<512>,
    pub sync_committee_signature: SignatureBytes,
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

async fn get<R: DeserializeOwned>(req: &str) -> EyreResult<R> {
    let bytes = reqwest::get(req).await?.bytes().await?;

    Ok(serde_json::from_slice::<R>(&bytes)?)
}

async fn get_bootstrap(checkpoint: &str) -> EyreResult<Bootstrap> {
    let req = format!(
        "{RPC_URL}/eth/v1/beacon/light_client/bootstrap/{checkpoint}",
    );

    let res: BootstrapResponse = get(&req).await.map_err(|e| {
        log::trace!("get bootstrap: {e:?}");
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
        log::trace!("get updates: {e:?}");

        e
    })?;

    Ok(res.into_iter().map(|d| d.data).collect())
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
    let checkpoint = "0xde41619442beea57eeae7a5c37ed13f1ca4f02611d4ad117d7cfa2e008cac75b";
    let bootstrap = get_bootstrap(checkpoint).await.map_err(|e| anyhow::Error::msg(e.to_string()))?;
    // println!("bootstrap = {bootstrap:?}");

    let mut buffer = Vec::with_capacity(10_000);
    let finalized_header = {
        let len = bootstrap.header.serialize(&mut buffer).unwrap();
        println!("len = {len}");

        buffer.clone()
    };
        println!("encoded = {finalized_header:?}");

    let deser = <Header as ssz_rs::Deserialize>::deserialize(&finalized_header[..]).unwrap();
    assert_eq!(bootstrap.header.slot, deser.slot);
    assert_eq!(bootstrap.header.proposer_index, deser.proposer_index);
    let current_sync_committee = {
        buffer.clear();
        bootstrap.current_sync_committee.serialize(&mut buffer).unwrap();

        buffer.clone()
    };
    let init = Init {
        last_checkpoint: hex::decode(&checkpoint[2..]).unwrap().try_into().unwrap(),
        optimistic_header: finalized_header.clone(),
        finalized_header,
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
        println!("111");
        let signature = <G2 as ark_serialize::CanonicalDeserialize>::deserialize_compressed(update.sync_aggregate.sync_committee_signature.as_ref());
        println!("222");
        // println!("signature = {signature:?}");

        let Ok(signature) = signature else {
            continue;
        };

        let signature_serialized = {
            let mut signature_serialized = Vec::with_capacity(512);
            signature.serialize_uncompressed(&signature_serialized).unwrap();

            signature_serialized 
        };

        println!("update.sync_aggregate.sync_committee_signature.len = {}", update.sync_aggregate.sync_committee_signature.as_ref().len());
        println!("signature_serialized.len = {}", signature_serialized.len());

        // self.verify_update(&update)?;
        // self.apply_update(&update);
    }

    // let message: ArkScale<Vec<G1Affine>> = vec![message].into();
    // let message_bytes = message.encode();

    // let payload = HandleMessage::MillerLoop {
    //     message: message_bytes,
    //     signatures,
    // };
    // let gas_limit = client
    //     .calculate_handle_gas(None, program_id.into(), payload.encode(), 0, true)
    //     .await?
    //     .min_limit;
    // println!("gas_limit {gas_limit:?}");

    // let (message_id, _) = client
    //     .send_message(program_id.into(), payload, gas_limit, 0)
    //     .await?;

    // assert!(listener.message_processed(message_id).await?.succeed());

    // let gas_limit = client
    //     .calculate_handle_gas(
    //         None,
    //         program_id.into(),
    //         HandleMessage::Exp.encode(),
    //         0,
    //         true,
    //     )
    //     .await?
    //     .min_limit;
    // println!("gas_limit {gas_limit:?}");

    // let (message_id, _) = client
    //     .send_message(program_id.into(), HandleMessage::Exp, gas_limit, 0)
    //     .await?;

    // assert!(listener.message_processed(message_id).await?.succeed());

    Ok(())
}
