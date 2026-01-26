// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
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

//! Validator storage and cache integration tests.

use crate::tests::utils::{
    EnvNetworkConfig, InfiniteStreamExt, NodeConfig, TestEnv, TestEnvConfig, TestingEvent,
    ValidatorsConfig, init_logger,
};
use alloy::providers::ext::AnvilApi;
use ethexe_common::{
    crypto::{DkgSessionId, PreNonceCommitment, SignAggregate, SignKind, SignSessionRequest},
    db::{
        DkgSessionState, DkgStorageRO, DkgStorageRW, OnChainStorageRO, SignStorageRO, SignStorageRW,
    },
    network::ValidatorMessage,
};
use ethexe_consensus::ConsensusEvent;
use ethexe_dkg_roast::roast::select_leader;
use gprimitives::{ActorId, H256};

#[tokio::test(flavor = "multi_thread")]
#[ntest::timeout(120_000)]
async fn roast_cache_eviction_still_signs() {
    init_logger();

    let config = TestEnvConfig {
        validators: ValidatorsConfig::PreDefined(1),
        network: EnvNetworkConfig::Enabled,
        ..Default::default()
    };
    let mut env = TestEnv::new(config).await.unwrap();

    let mut validator =
        env.new_node(NodeConfig::named("validator").validator(env.validators[0].clone()));
    validator.start_service().await;

    let timelines = env.db.protocol_timelines().expect("timelines missing");
    let target_era = 5;
    let target_ts = timelines.genesis_ts + timelines.era * target_era;

    let target = ActorId::zero();
    let old_msg_hash0 = H256::random();
    let old_msg_hash1 = H256::random();
    let old_aggregate0 = SignAggregate {
        session: DkgSessionId { era: 0 },
        msg_hash: old_msg_hash0,
        tweaked_pk: [1u8; 33],
        signature96: [2u8; 96],
    };
    let old_aggregate1 = SignAggregate {
        session: DkgSessionId { era: 1 },
        msg_hash: old_msg_hash1,
        tweaked_pk: [3u8; 33],
        signature96: [4u8; 96],
    };
    let old_pre_nonces = vec![PreNonceCommitment {
        commitments: vec![0xAA; 32],
        nonces: vec![0xBB; 64],
    }];

    validator
        .db
        .set_signature_cache(0, target, old_msg_hash0, old_aggregate0);
    validator
        .db
        .set_signature_cache(1, target, old_msg_hash1, old_aggregate1);
    validator
        .db
        .set_pre_nonce_cache(0, target, old_pre_nonces.clone());
    validator
        .db
        .set_pre_nonce_cache(1, target, old_pre_nonces.clone());

    let (next_configs, _commitment) = TestEnv::define_session_keys_for_era(
        &env.signer,
        vec![env.validators[0].public_key],
        target_era,
    );
    let next_public_key_package = next_configs[0].dkg_public_key_package.clone();
    let next_vss_commitment = next_configs[0].dkg_vss_commitment.clone();
    let next_key_package = next_configs[0].dkg_key_package.clone();
    let next_share = next_configs[0].dkg_share.clone();
    let next_session = DkgSessionId { era: target_era };

    validator
        .db
        .set_public_key_package(next_session.era, next_public_key_package);
    validator
        .db
        .set_dkg_vss_commitment(next_session.era, next_vss_commitment);
    validator
        .db
        .set_dkg_key_package(next_session.era, next_key_package);
    validator.db.set_dkg_share(next_share);
    validator.db.set_dkg_session_state(
        next_session,
        DkgSessionState {
            completed: true,
            ..Default::default()
        },
    );

    env.provider
        .anvil_set_next_block_timestamp(target_ts)
        .await
        .unwrap();
    env.force_new_block().await;

    let uploaded_code = env
        .upload_code(demo_ping::WASM_BINARY)
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();
    assert!(uploaded_code.valid);

    let ping_actor = env
        .create_program(uploaded_code.code_id, 500_000_000_000_000)
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();

    let reply = env
        .send_message(ping_actor.program_id, b"PING")
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();
    assert_eq!(reply.payload, b"PONG");

    assert!(
        validator
            .db
            .signature_cache(0, target, old_msg_hash0)
            .is_none()
    );
    assert!(
        validator
            .db
            .signature_cache(1, target, old_msg_hash1)
            .is_none()
    );
    assert!(validator.db.pre_nonce_cache(0, target).is_none());
    assert!(validator.db.pre_nonce_cache(1, target).is_none());
}

#[tokio::test(flavor = "multi_thread")]
#[ntest::timeout(90_000)]
async fn missing_share_triggers_warning_and_recovers() {
    init_logger();

    let config = TestEnvConfig {
        validators: ValidatorsConfig::PreDefined(1),
        network: EnvNetworkConfig::Enabled,
        ..Default::default()
    };
    let mut env = TestEnv::new(config).await.unwrap();

    let mut validator =
        env.new_node(NodeConfig::named("validator").validator(env.validators[0].clone()));
    validator.start_service().await;
    let mut events = validator.new_events();

    let mut share = validator.db.dkg_share(0).expect("missing share");
    let original_share = share.clone();
    share.index = share.index.saturating_add(1);
    validator.db.set_dkg_share(share);

    let participants = vec![env.validators[0].public_key.to_address()];
    let msg_hash = H256::random();
    let request = SignSessionRequest {
        session: DkgSessionId { era: 0 },
        leader: select_leader(&participants, msg_hash, 0, 0),
        attempt: 0,
        msg_hash,
        tweak_target: ActorId::zero(),
        threshold: original_share.threshold,
        participants,
        kind: SignKind::ArbitraryHash,
    };
    let publisher =
        env.new_node(NodeConfig::named("publisher").validator(env.validators[0].clone()));
    publisher
        .publish_validator_message(ValidatorMessage {
            era_index: 0,
            payload: request,
        })
        .await;

    let warning = tokio::time::timeout(env.block_time * 5, async {
        events
            .find_map(|event| match event {
                TestingEvent::Consensus(ConsensusEvent::Warning(msg)) => Some(msg),
                _ => None,
            })
            .await
    })
    .await
    .expect("warning not observed");
    assert!(
        warning.contains("DKG share index mismatch")
            || warning.contains("Missing DKG share details")
            || warning.contains("Key package identifier mismatch")
            || warning.contains("Missing key package"),
        "unexpected warning: {warning}"
    );

    let session = DkgSessionId { era: 0 };
    tokio::time::timeout(env.block_time * 5, async {
        loop {
            let completed = validator
                .db
                .dkg_session_state(session)
                .map(|state| state.completed)
                .unwrap_or(false);
            if !completed {
                break;
            }
            tokio::time::sleep(env.block_time / 2).await;
        }
    })
    .await
    .expect("DKG session was not reset after restart");
}
