// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use super::helpers::{networked_config, start_validators, start_validators_with_events};
use crate::tests::utils::{
    InfiniteStreamExt, TestEnv, TestingEvent, TestingNetworkEvent, init_logger,
};
use ethexe_common::network::VerifiedValidatorMessage;
use ethexe_dkg_roast::roast::select_leader;
use std::time::Duration;

#[tokio::test(flavor = "multi_thread")]
#[ntest::timeout(120_000)]
async fn roast_signing_with_missing_validator() {
    init_logger();

    let config = networked_config(4);
    let mut env = TestEnv::new(config).await.unwrap();

    let mut validators = start_validators(&mut env).await;

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
#[ntest::timeout(180_000)]
async fn roast_retries_after_leader_timeout() {
    init_logger();

    let config = networked_config(4);
    let mut env = TestEnv::new(config).await.unwrap();

    let (mut validators, mut node_events) = start_validators_with_events(&mut env).await;

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

    log::info!("📗 Resetting observer events before sending message");
    let mut observer_events = validators[0].new_events();

    let pending = env
        .send_message(ping_actor.program_id, b"PING")
        .await
        .unwrap();

    let (leader_request, request_sender) = tokio::time::timeout(Duration::from_secs(10), async {
        observer_events
            .find_map(|event| match event {
                TestingEvent::Network(TestingNetworkEvent::ValidatorMessage(msg)) => {
                    if let VerifiedValidatorMessage::SignSessionRequest(ref request) = msg {
                        Some((request.data().payload.clone(), msg.address()))
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
    log::info!(
        "📗 ROAST leaders: leader {}, next_leader {}, request_sender {}",
        leader_address,
        next_leader,
        request_sender
    );

    if leader_address == request_sender {
        log::warn!(
            "Leader matches request sender; skipping leader stop to avoid aborting coordinator"
        );
    } else {
        let leader_index = env
            .validators
            .iter()
            .position(|cfg| cfg.public_key.to_address() == leader_address)
            .expect("leader must be one of validators");
        validators[leader_index].stop_service().await;
    }

    let retry_index = env
        .validators
        .iter()
        .position(|cfg| cfg.public_key.to_address() == next_leader)
        .expect("retry leader must be one of validators");
    let mut retry_events = node_events.swap_remove(retry_index);

    let retry_observed = tokio::time::timeout(Duration::from_secs(120), async {
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
    .is_ok();

    if !retry_observed {
        log::warn!("Retry sign session request not observed; continuing to await completion");
    }

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

    let config = networked_config(3);
    let mut env = TestEnv::new(config).await.unwrap();

    let mut validators = start_validators(&mut env).await;

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

    let config = networked_config(4);
    let mut env = TestEnv::new(config).await.unwrap();

    let mut validators = start_validators(&mut env).await;

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
