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
use gear_core::ids::{ActorId, MessageId};
use gsdk::{Result, SignedApi, events};
use parity_scale_codec::Encode;
use std::pin::pin;
use utils::dev_node;

mod utils;

type ArkScale<T> = ark_scale::ArkScale<T, { ark_scale::HOST_CALL }>;
type ScalarField = <G2 as Group>::ScalarField;

async fn common_upload_program(
    api: &SignedApi,
    code: Vec<u8>,
    payload: impl Encode,
) -> Result<(MessageId, ActorId)> {
    let encoded_payload = payload.encode();
    let gas_limit = api
        .calculate_upload_gas(code.clone(), encoded_payload, 0, true)
        .await?
        .min_limit;

    println!("init gas {gas_limit:?}");
    let (message_id, program_id) = api
        .upload_program(
            code,
            gear_utils::now_micros().to_le_bytes(),
            payload,
            gas_limit,
            0,
        )
        .await?
        .value;

    Ok((message_id, program_id))
}

async fn upload_program(api: &SignedApi, payload: impl Encode) -> Result<ActorId> {
    let events = api.subscribe_all_events().await?;

    let (message_id, program_id) =
        common_upload_program(api, WASM_BINARY.to_vec(), payload).await?;

    assert!(
        events::message_dispatch_status(message_id, events)
            .await?
            .is_success()
    );

    Ok(program_id)
}

#[tokio::test]
async fn builtin_bls381() -> Result<()> {
    let (_node, api) = dev_node().await;

    let mut events = pin!(api.subscribe_all_events().await?);

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
        &api,
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
    let gas_limit = api
        .calculate_handle_gas(program_id, payload.encode(), 0, true)
        .await?
        .min_limit;
    println!("gas_limit {gas_limit:?}");

    let message_id = api
        .send_message(program_id, payload, gas_limit, 0)
        .await?
        .value;

    assert!(
        events::message_dispatch_status(message_id, &mut events)
            .await?
            .is_success()
    );

    let gas_limit = api
        .calculate_handle_gas(program_id, HandleMessage::Exp.encode(), 0, true)
        .await?
        .min_limit;
    println!("gas_limit {gas_limit:?}");

    let message_id = api
        .send_message(program_id, HandleMessage::Exp, gas_limit, 0)
        .await?
        .value;

    assert!(
        events::message_dispatch_status(message_id, &mut events)
            .await?
            .is_success()
    );

    Ok(())
}
