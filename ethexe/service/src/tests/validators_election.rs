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

//! Validator election integration tests.

use crate::tests::utils::{
    EnvNetworkConfig, InfiniteStreamExt, Node, NodeConfig, TestEnv, TestEnvConfig, ValidatorConfig,
    ValidatorsConfig, Wallets, init_logger,
};
use alloy::providers::{Provider as _, ext::AnvilApi};
use anyhow::Result;
use ethexe_common::{
    Address, ValidatorsVec,
    crypto::{DkgPublicKeyPackage, DkgSessionId, DkgVssCommitment},
    db::{DkgSessionState, DkgStorageRW},
    events::{BlockEvent, RouterEvent},
};
use ethexe_ethereum::deploy::ContractsDeploymentParams;
use gsigner::secp256k1::Signer;
use std::{collections::BTreeSet, time::Duration};

struct ElectionEnv {
    env: TestEnv,
    validators: Vec<Node>,
    next_validators_configs: Vec<ValidatorConfig>,
    next_public_key_package: DkgPublicKeyPackage,
    next_vss_commitment: DkgVssCommitment,
    next_dkg_session: DkgSessionId,
    election_ts: u64,
    era_duration: u64,
    genesis_ts: u64,
}

async fn setup_election_env() -> ElectionEnv {
    let election_ts = 20 * 60 * 60;
    let era_duration = 24 * 60 * 60;
    let deploy_params = ContractsDeploymentParams {
        with_middleware: true,
        era_duration,
        election_duration: era_duration - election_ts,
    };

    let signer = Signer::memory();
    let mut wallets = Wallets::anvil(&signer);

    let current_validators: Vec<_> = (0..5).map(|_| wallets.next()).collect();
    let next_validators: Vec<_> = (0..5).map(|_| wallets.next()).collect();

    let env_config = TestEnvConfig {
        validators: ValidatorsConfig::ProvidedValidators(current_validators),
        deploy_params,
        network: EnvNetworkConfig::Enabled,
        signer: signer.clone(),
        ..Default::default()
    };
    let mut env = TestEnv::new(env_config).await.unwrap();

    let genesis_block_hash = env
        .ethereum
        .router()
        .query()
        .genesis_block_hash()
        .await
        .unwrap();
    let genesis_ts = env
        .provider
        .get_block_by_hash(genesis_block_hash.0.into())
        .await
        .unwrap()
        .unwrap()
        .header
        .timestamp;

    let mut validators = vec![];
    for (i, v) in env.validators.clone().into_iter().enumerate() {
        log::info!("📗 Starting validator-{i}");
        let mut validator = env.new_node(NodeConfig::named(format!("validator-{i}")).validator(v));
        validator.start_service().await;
        validators.push(validator);
    }

    let (next_validators_configs, _commitment) =
        TestEnv::define_session_keys_for_era(&signer, next_validators, 1);
    let next_public_key_package = next_validators_configs[0].dkg_public_key_package.clone();
    let next_vss_commitment = next_validators_configs[0].dkg_vss_commitment.clone();
    let next_dkg_session = DkgSessionId { era: 1 };

    ElectionEnv {
        env,
        validators,
        next_validators_configs,
        next_public_key_package,
        next_vss_commitment,
        next_dkg_session,
        election_ts,
        era_duration,
        genesis_ts,
    }
}

fn validator_addresses(configs: &[ValidatorConfig]) -> Vec<Address> {
    configs
        .iter()
        .map(|cfg| cfg.public_key.to_address())
        .collect()
}

async fn seed_next_era_dkg(env: &mut ElectionEnv) {
    for validator in env.validators.iter_mut() {
        validator.db.set_public_key_package(
            env.next_dkg_session.era,
            env.next_public_key_package.clone(),
        );
        validator
            .db
            .set_dkg_vss_commitment(env.next_dkg_session.era, env.next_vss_commitment.clone());
        validator.db.set_dkg_session_state(
            env.next_dkg_session,
            DkgSessionState {
                completed: true,
                ..Default::default()
            },
        );
    }
}

async fn apply_next_era_state(env: &mut ElectionEnv, next_validators: ValidatorsVec) {
    env.env
        .election_provider
        .set_predefined_election_at(env.election_ts + env.genesis_ts, next_validators)
        .await;

    env.env
        .provider
        .anvil_set_next_block_timestamp(env.election_ts + env.genesis_ts)
        .await
        .unwrap();

    seed_next_era_dkg(env).await;

    env.env.force_new_block().await;
}

async fn wait_for_validators_commit(env: &TestEnv) -> Result<(), tokio::time::error::Elapsed> {
    tokio::time::timeout(
        Duration::from_secs(20),
        env.new_observer_events()
            .filter_map_block_synced()
            .find(|event| {
                matches!(
                    event,
                    BlockEvent::Router(RouterEvent::ValidatorsCommittedForEra(_))
                )
            }),
    )
    .await
    .map(|_| ())
}

#[tokio::test(flavor = "multi_thread")]
#[ntest::timeout(180_000)]
async fn validators_election_quorum_with_offline_next_validator() {
    init_logger();

    let mut env = setup_election_env().await;

    let next_validators = validator_addresses(&env.next_validators_configs);

    apply_next_era_state(&mut env, next_validators.try_into().unwrap()).await;

    wait_for_validators_commit(&env.env)
        .await
        .expect("validators commitment should succeed");

    let uploaded_code = env
        .env
        .upload_code(demo_ping::WASM_BINARY)
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();
    assert!(uploaded_code.valid);

    let ping_actor = env
        .env
        .create_program(uploaded_code.code_id, 500_000_000_000_000)
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();
    assert_eq!(ping_actor.code_id, uploaded_code.code_id);

    for mut node in env.validators.into_iter() {
        node.stop_service().await;
    }

    env.env.validators = env.next_validators_configs;
    let mut new_validators = vec![];
    for (i, v) in env.env.validators.clone().into_iter().enumerate() {
        if i == 4 {
            continue;
        }
        log::info!("📗 Starting validator-{i}");
        let mut validator = env
            .env
            .new_node(NodeConfig::named(format!("validator-{i}")).validator(v));
        validator.start_service().await;
        new_validators.push(validator);
    }

    env.env
        .provider
        .anvil_set_next_block_timestamp(env.era_duration + env.genesis_ts)
        .await
        .unwrap();
    env.env.force_new_block().await;

    let reply = env
        .env
        .send_message(ping_actor.program_id, b"PING")
        .await
        .expect("pong reply")
        .wait_for()
        .await
        .expect("reply info");

    assert_eq!(reply.payload, b"PONG");
    assert_eq!(reply.program_id, ping_actor.program_id);
}

#[tokio::test(flavor = "multi_thread")]
#[ntest::timeout(120_000)]
async fn validators_election_overwrites_invalid_update() {
    init_logger();

    let mut env = setup_election_env().await;

    let mut wrong_validators: Vec<_> = env
        .next_validators_configs
        .iter()
        .map(|cfg| cfg.public_key.to_address())
        .collect();
    wrong_validators.pop();

    let next_validators: Vec<_> = env
        .next_validators_configs
        .iter()
        .map(|cfg| cfg.public_key.to_address())
        .collect();

    env.env
        .election_provider
        .set_predefined_election_at(
            env.election_ts + env.genesis_ts,
            wrong_validators.try_into().unwrap(),
        )
        .await;
    env.env
        .election_provider
        .set_predefined_election_at(
            env.election_ts + env.genesis_ts,
            next_validators.clone().try_into().unwrap(),
        )
        .await;

    env.env
        .provider
        .anvil_set_next_block_timestamp(env.election_ts + env.genesis_ts)
        .await
        .unwrap();

    for validator in env.validators.iter_mut() {
        validator.db.set_public_key_package(
            env.next_dkg_session.era,
            env.next_public_key_package.clone(),
        );
        validator
            .db
            .set_dkg_vss_commitment(env.next_dkg_session.era, env.next_vss_commitment.clone());
        validator.db.set_dkg_session_state(
            env.next_dkg_session,
            DkgSessionState {
                completed: true,
                ..Default::default()
            },
        );
    }
    env.env.force_new_block().await;

    wait_for_validators_commit(&env.env)
        .await
        .expect("validators commitment should succeed with updated election");

    env.env
        .provider
        .anvil_set_next_block_timestamp(env.era_duration + env.genesis_ts)
        .await
        .unwrap();
    env.env.force_new_block().await;

    let latest_block = env.env.latest_block().await;
    let committed_validators = env
        .env
        .ethereum
        .router()
        .query()
        .validators_at(latest_block.hash)
        .await
        .unwrap();
    let committed_set: BTreeSet<_> = committed_validators.iter().copied().collect();
    let expected_set: BTreeSet<_> = next_validators.iter().copied().collect();
    assert_eq!(committed_set, expected_set);
}

#[tokio::test(flavor = "multi_thread")]
#[ntest::timeout(120_000)]
async fn validators_election_rejects_wrong_validator_count() {
    init_logger();

    let mut env = setup_election_env().await;

    let mut wrong_validators = validator_addresses(&env.next_validators_configs);
    wrong_validators.pop();

    apply_next_era_state(&mut env, wrong_validators.try_into().unwrap()).await;

    assert!(
        wait_for_validators_commit(&env.env).await.is_err(),
        "validators commitment should be rejected with wrong validator count"
    );

    env.env
        .provider
        .anvil_set_next_block_timestamp(env.era_duration + env.genesis_ts)
        .await
        .unwrap();
    env.env.force_new_block().await;

    let latest_block = env.env.latest_block().await;
    let committed_validators = env
        .env
        .ethereum
        .router()
        .query()
        .validators_at(latest_block.hash)
        .await
        .unwrap();
    let committed_set: BTreeSet<_> = committed_validators.iter().copied().collect();
    let expected_set: BTreeSet<_> = validator_addresses(&env.env.validators)
        .iter()
        .copied()
        .collect();
    assert_eq!(committed_set, expected_set);
}

#[tokio::test(flavor = "multi_thread")]
#[ntest::timeout(120_000)]
async fn validators_election_last_update_wins() {
    init_logger();

    let mut env = setup_election_env().await;

    let first_validators = validator_addresses(&env.env.validators);
    let next_validators = validator_addresses(&env.next_validators_configs);

    env.env
        .election_provider
        .set_predefined_election_at(
            env.election_ts + env.genesis_ts,
            first_validators.try_into().unwrap(),
        )
        .await;
    env.env
        .election_provider
        .set_predefined_election_at(
            env.election_ts + env.genesis_ts,
            next_validators.clone().try_into().unwrap(),
        )
        .await;

    env.env
        .provider
        .anvil_set_next_block_timestamp(env.election_ts + env.genesis_ts)
        .await
        .unwrap();

    seed_next_era_dkg(&mut env).await;
    env.env.force_new_block().await;

    wait_for_validators_commit(&env.env)
        .await
        .expect("validators commitment should succeed with latest election");

    env.env
        .provider
        .anvil_set_next_block_timestamp(env.era_duration + env.genesis_ts)
        .await
        .unwrap();
    env.env.force_new_block().await;

    let latest_block = env.env.latest_block().await;
    let committed_validators = env
        .env
        .ethereum
        .router()
        .query()
        .validators_at(latest_block.hash)
        .await
        .unwrap();
    let committed_set: BTreeSet<_> = committed_validators.iter().copied().collect();
    let expected_set: BTreeSet<_> = next_validators.iter().copied().collect();
    assert_eq!(committed_set, expected_set);
}

#[tokio::test(flavor = "multi_thread")]
#[ntest::timeout(120_000)]
async fn validators_election_succeeds_with_offline_current_validator() {
    init_logger();

    let mut env = setup_election_env().await;

    let next_validators: Vec<_> = env
        .next_validators_configs
        .iter()
        .map(|cfg| cfg.public_key.to_address())
        .collect();

    env.validators[0].stop_service().await;
    apply_next_era_state(&mut env, next_validators.try_into().unwrap()).await;

    wait_for_validators_commit(&env.env)
        .await
        .expect("validators commitment should succeed with quorum");
}

#[tokio::test(flavor = "multi_thread")]
#[ntest::timeout(60_000)]
async fn validators_election() {
    init_logger();

    let election_ts = 20 * 60 * 60;
    let era_duration = 24 * 60 * 60;
    let deploy_params = ContractsDeploymentParams {
        with_middleware: true,
        era_duration,
        election_duration: era_duration - election_ts,
    };

    let signer = Signer::memory();
    let mut wallets = Wallets::anvil(&signer);

    let current_validators: Vec<_> = (0..5).map(|_| wallets.next()).collect();
    let next_validators: Vec<_> = (0..5).map(|_| wallets.next()).collect();

    let env_config = TestEnvConfig {
        validators: ValidatorsConfig::ProvidedValidators(current_validators),
        deploy_params,
        network: EnvNetworkConfig::Enabled,
        signer: signer.clone(),
        ..Default::default()
    };
    let mut env = TestEnv::new(env_config).await.unwrap();

    let genesis_block_hash = env
        .ethereum
        .router()
        .query()
        .genesis_block_hash()
        .await
        .unwrap();
    let genesis_ts = env
        .provider
        .get_block_by_hash(genesis_block_hash.0.into())
        .await
        .unwrap()
        .unwrap()
        .header
        .timestamp;

    let mut validators = vec![];
    for (i, v) in env.validators.clone().into_iter().enumerate() {
        let mut validator = env.new_node(NodeConfig::named(format!("validator-{i}")).validator(v));
        validator.start_service().await;
        validators.push(validator);
    }

    let (next_validators_configs, _commitment) =
        TestEnv::define_session_keys_for_era(&signer, next_validators, 1);
    let next_public_key_package = next_validators_configs[0].dkg_public_key_package.clone();
    let next_vss_commitment = next_validators_configs[0].dkg_vss_commitment.clone();
    let next_dkg_session = DkgSessionId { era: 1 };

    let next_validators: Vec<_> = next_validators_configs
        .iter()
        .map(|cfg| cfg.public_key.to_address())
        .collect();

    env.election_provider
        .set_predefined_election_at(
            election_ts + genesis_ts,
            next_validators.try_into().unwrap(),
        )
        .await;

    env.provider
        .anvil_set_next_block_timestamp(election_ts + genesis_ts)
        .await
        .unwrap();

    for validator in validators.iter_mut() {
        validator
            .db
            .set_public_key_package(next_dkg_session.era, next_public_key_package.clone());
        validator
            .db
            .set_dkg_vss_commitment(next_dkg_session.era, next_vss_commitment.clone());
        validator.db.set_dkg_session_state(
            next_dkg_session,
            DkgSessionState {
                completed: true,
                ..Default::default()
            },
        );
    }
    env.force_new_block().await;

    env.new_observer_events()
        .filter_map_block_synced()
        .find(|event| {
            matches!(
                event,
                BlockEvent::Router(RouterEvent::ValidatorsCommittedForEra(_))
            )
        })
        .await;

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
    assert_eq!(ping_actor.code_id, uploaded_code.code_id);

    for mut node in validators.into_iter() {
        node.stop_service().await;
    }

    env.validators = next_validators_configs;
    let mut new_validators = vec![];
    for (i, v) in env.validators.clone().into_iter().enumerate() {
        let mut validator = env.new_node(NodeConfig::named(format!("validator-{i}")).validator(v));
        validator.start_service().await;
        new_validators.push(validator);
    }

    env.provider
        .anvil_set_next_block_timestamp(era_duration + genesis_ts)
        .await
        .unwrap();
    env.force_new_block().await;

    let reply = env
        .send_message(ping_actor.program_id, b"PING")
        .await
        .expect("pong reply")
        .wait_for()
        .await
        .expect("reply info");

    assert_eq!(reply.payload, b"PONG");
    assert_eq!(reply.program_id, ping_actor.program_id);
}
