// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::tests::utils::{
    EnvNetworkConfig, InfiniteStreamExt, NodeConfig, TestEnv, TestEnvConfig, TestingEvent,
    TestingNetworkEvent, ValidatorsConfig, init_logger,
};
use ethexe_common::{db::DkgStorageRO, network::VerifiedValidatorMessage};
use ethexe_consensus::roast::select_leader;
use std::time::Duration;

#[tokio::test(flavor = "multi_thread")]
#[ntest::timeout(60_000)]
async fn dkg_share_is_available_for_validator() {
    init_logger();

    let config = TestEnvConfig {
        validators: ValidatorsConfig::PreDefined(3),
        network: EnvNetworkConfig::Enabled,
        ..Default::default()
    };
    let mut env = TestEnv::new(config).await.unwrap();

    let mut validator =
        env.new_node(NodeConfig::named("validator").validator(env.validators[0].clone()));
    validator.start_service().await;

    assert!(validator.db.dkg_share(0).is_some());
    assert!(validator.db.public_key_package(0).is_some());
    assert!(validator.db.dkg_vss_commitment(0).is_some());
}

#[tokio::test(flavor = "multi_thread")]
#[ntest::timeout(60_000)]
async fn roast_signing_with_missing_validator() {
    init_logger();

    let config = TestEnvConfig {
        validators: ValidatorsConfig::PreDefined(4),
        network: EnvNetworkConfig::Enabled,
        ..Default::default()
    };
    let mut env = TestEnv::new(config).await.unwrap();

    let mut validators = vec![];
    for (i, v) in env.validators.clone().into_iter().enumerate() {
        log::info!("📗 Starting validator-{i}");
        let mut validator = env.new_node(NodeConfig::named(format!("validator-{i}")).validator(v));
        validator.start_service().await;
        validators.push(validator);
    }

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

    validators[3].stop_service().await;

    while env.next_block_producer_index().await == 3 {
        env.force_new_block().await;
    }

    let reply = env
        .send_message(ping_actor.program_id, b"PING")
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();

    assert_eq!(reply.payload, b"PONG");
}

#[tokio::test(flavor = "multi_thread")]
#[ntest::timeout(60_000)]
async fn dkg_persists_across_validator_restart() {
    init_logger();

    let config = TestEnvConfig {
        validators: ValidatorsConfig::PreDefined(3),
        network: EnvNetworkConfig::Enabled,
        ..Default::default()
    };
    let mut env = TestEnv::new(config).await.unwrap();

    let mut validators = vec![];
    for (i, v) in env.validators.clone().into_iter().enumerate() {
        log::info!("📗 Starting validator-{i}");
        let mut validator = env.new_node(NodeConfig::named(format!("validator-{i}")).validator(v));
        validator.start_service().await;
        validators.push(validator);
    }

    validators[0].stop_service().await;
    validators[0].start_service().await;

    assert!(validators[0].db.dkg_share(0).is_some());
    assert!(validators[0].db.public_key_package(0).is_some());
    assert!(validators[0].db.dkg_vss_commitment(0).is_some());

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
}

#[tokio::test(flavor = "multi_thread")]
#[ntest::timeout(120_000)]
async fn roast_retries_after_leader_timeout() {
    init_logger();

    let config = TestEnvConfig {
        validators: ValidatorsConfig::PreDefined(4),
        network: EnvNetworkConfig::Enabled,
        ..Default::default()
    };
    let mut env = TestEnv::new(config).await.unwrap();

    let mut validators = vec![];
    let mut observer_events = None;
    let mut node_events = Vec::new();
    for (i, v) in env.validators.clone().into_iter().enumerate() {
        log::info!("📗 Starting validator-{i}");
        let mut validator = env.new_node(NodeConfig::named(format!("validator-{i}")).validator(v));
        validator.start_service().await;
        if i == 0 {
            observer_events = Some(validator.new_events());
        }
        node_events.push(validator.new_events());
        validators.push(validator);
    }
    let mut observer_events = observer_events.expect("observer events missing");

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

    let pending = env
        .send_message(ping_actor.program_id, b"PING")
        .await
        .unwrap();

    let leader_request = tokio::time::timeout(Duration::from_secs(10), async {
        observer_events
            .find_map(|event| match event {
                TestingEvent::Network(TestingNetworkEvent::ValidatorMessage(msg)) => {
                    if let VerifiedValidatorMessage::SignSessionRequest(request) = msg {
                        Some(request.data().payload.clone())
                    } else {
                        None
                    }
                }
                _ => None,
            })
            .await
    })
    .await
    .expect("sign session request not observed");
    let leader_address = leader_request.leader;
    let next_leader = select_leader(
        &leader_request.participants,
        leader_request.msg_hash,
        leader_request.session.era,
        leader_request.attempt.saturating_add(1),
    );

    let leader_index = env
        .validators
        .iter()
        .position(|cfg| cfg.public_key.to_address() == leader_address)
        .expect("leader must be one of validators");
    validators[leader_index].stop_service().await;

    let retry_index = env
        .validators
        .iter()
        .position(|cfg| cfg.public_key.to_address() == next_leader)
        .expect("retry leader must be one of validators");
    let mut retry_events = node_events.swap_remove(retry_index);

    tokio::time::timeout(Duration::from_secs(80), async {
        retry_events
            .find_map(|event| match event {
                TestingEvent::Network(TestingNetworkEvent::ValidatorMessage(msg)) => {
                    if let VerifiedValidatorMessage::SignSessionRequest(request) = msg {
                        let payload = request.data().payload.clone();
                        if payload.attempt > 0 || payload.leader == next_leader {
                            return Some(payload.attempt);
                        }
                    }
                    None
                }
                _ => None,
            })
            .await
    })
    .await
    .expect("retry sign session request not observed");

    let reply = tokio::time::timeout(Duration::from_secs(60), pending.wait_for())
        .await
        .expect("signing did not complete after retry")
        .unwrap();
    assert_eq!(reply.payload, b"PONG");
}

#[tokio::test(flavor = "multi_thread")]
#[ntest::timeout(120_000)]
async fn roast_completes_after_lagging_validator_catchup() {
    init_logger();

    let config = TestEnvConfig {
        validators: ValidatorsConfig::PreDefined(3),
        network: EnvNetworkConfig::Enabled,
        ..Default::default()
    };
    let mut env = TestEnv::new(config).await.unwrap();

    let mut validators = vec![];
    for (i, v) in env.validators.clone().into_iter().enumerate() {
        log::info!("📗 Starting validator-{i}");
        let mut validator = env.new_node(NodeConfig::named(format!("validator-{i}")).validator(v));
        validator.start_service().await;
        validators.push(validator);
    }

    let lagging_index = 2;
    validators[lagging_index].stop_service().await;

    validators[lagging_index].start_service().await;
    let mut lagging_events = validators[lagging_index].new_events();
    env.force_new_block().await;
    let latest_block = env.latest_block().await;
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let synced = lagging_events.find_block_synced().await;
            if synced == latest_block.hash {
                break;
            }
        }
    })
    .await
    .expect("lagging validator did not catch up to latest block");

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
}

#[tokio::test(flavor = "multi_thread")]
#[ntest::timeout(120_000)]
async fn roast_recovers_from_mid_session_crash() {
    init_logger();

    let config = TestEnvConfig {
        validators: ValidatorsConfig::PreDefined(4),
        network: EnvNetworkConfig::Enabled,
        ..Default::default()
    };
    let mut env = TestEnv::new(config).await.unwrap();

    let mut validators = vec![];
    for (i, v) in env.validators.clone().into_iter().enumerate() {
        log::info!("📗 Starting validator-{i}");
        let mut validator = env.new_node(NodeConfig::named(format!("validator-{i}")).validator(v));
        validator.start_service().await;
        validators.push(validator);
    }

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

    let mut observer_events = validators[0].new_events();
    let observer_address = env.validators[0].public_key.to_address();

    let pending = env
        .send_message(ping_actor.program_id, b"PING")
        .await
        .unwrap();

    let leader_address = tokio::time::timeout(Duration::from_secs(10), async {
        observer_events
            .find_map(|event| match event {
                TestingEvent::Network(TestingNetworkEvent::ValidatorMessage(msg)) => {
                    if let VerifiedValidatorMessage::SignSessionRequest(request) = msg {
                        return Some(request.data().payload.leader);
                    }
                    None
                }
                _ => None,
            })
            .await
    })
    .await
    .expect("sign session request not observed");

    let crash_address = env
        .validators
        .iter()
        .map(|cfg| cfg.public_key.to_address())
        .find(|addr| *addr != leader_address && *addr != observer_address)
        .expect("no crash candidate");

    let crash_index = env
        .validators
        .iter()
        .position(|cfg| cfg.public_key.to_address() == crash_address)
        .expect("crash validator must be one of validators");
    validators[crash_index].stop_service().await;
    tokio::time::sleep(env.block_time * 2).await;
    validators[crash_index].start_service().await;

    drop(pending);

    let reply = env
        .send_message(ping_actor.program_id, b"PING")
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();
    assert_eq!(reply.payload, b"PONG");
}
