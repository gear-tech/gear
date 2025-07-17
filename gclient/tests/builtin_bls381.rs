// This file is part of Gear.

// Copyright (C) 2024-2025 Gear Technologies Inc.
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
use ark_std::{UniformRand, ops::Mul};
use demo_bls381::*;
use gclient::{EventListener, EventProcessor, GearApi, Result};
use gstd::prelude::*;

type ArkScale<T> = ark_scale::ArkScale<T, { ark_scale::HOST_CALL }>;
type ScalarField = <G2 as Group>::ScalarField;

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

    assert!(
        listener
            .message_processed(message_id.into())
            .await?
            .succeed()
    );

    Ok(program_id)
}

#[tokio::test]
async fn builtin_bls381() -> Result<()> {
    let client = GearApi::dev_from_path("../target/release/gear").await?;
    let mut listener = client.subscribe().await?;

    let mut rng = ark_std::test_rng();

    let generator: G2 = G2::generator();
    let message: G1Affine = G1::rand(&mut rng).into();
    let mut pub_keys = Vec::new();
    let mut signatures = Vec::new();
    for _ in 0..2 {
        let priv_key: ScalarField = UniformRand::rand(&mut rng);
        let pub_key: G2Affine = generator.mul(priv_key).into();
        let mut pub_key_bytes = Vec::new();
        pub_key.serialize_uncompressed(&mut pub_key_bytes).unwrap();
        pub_keys.push(pub_key_bytes);

        // sign
        let signature: G1Affine = message.mul(priv_key).into();
        let mut sig_bytes = Vec::new();
        signature.serialize_uncompressed(&mut sig_bytes).unwrap();
        signatures.push(sig_bytes);
    }

    let mut gen_bytes = Vec::new();
    generator.serialize_uncompressed(&mut gen_bytes).unwrap();

    let program_id = upload_program(
        &client,
        &mut listener,
        InitMessage {
            g2_gen: gen_bytes,
            pub_keys,
        },
    )
    .await?;

    let message: ArkScale<Vec<G1Affine>> = vec![message].into();
    let message_bytes = message.encode();

    let payload = HandleMessage::MillerLoop {
        message: message_bytes,
        signatures,
    };
    let gas_limit = client
        .calculate_handle_gas(None, program_id.into(), payload.encode(), 0, true)
        .await?
        .min_limit;
    println!("gas_limit {gas_limit:?}");

    let (message_id, _) = client
        .send_message(program_id.into(), payload, gas_limit, 0)
        .await?;

    assert!(listener.message_processed(message_id).await?.succeed());

    let gas_limit = client
        .calculate_handle_gas(
            None,
            program_id.into(),
            HandleMessage::Exp.encode(),
            0,
            true,
        )
        .await?
        .min_limit;
    println!("gas_limit {gas_limit:?}");

    let (message_id, _) = client
        .send_message(program_id.into(), HandleMessage::Exp, gas_limit, 0)
        .await?;

    assert!(listener.message_processed(message_id).await?.succeed());

    Ok(())
}
