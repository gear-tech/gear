// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use super::helpers::{networked_config, start_validators};
use crate::tests::utils::{NodeConfig, TestEnv, init_logger};
use ethexe_common::db::DkgStorageRO;

#[tokio::test(flavor = "multi_thread")]
#[ntest::timeout(60_000)]
async fn dkg_share_is_available_for_validator() {
    init_logger();

    let config = networked_config(3);
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
async fn dkg_persists_across_validator_restart() {
    init_logger();

    let config = networked_config(3);
    let mut env = TestEnv::new(config).await.unwrap();

    let mut validators = start_validators(&mut env).await;

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
