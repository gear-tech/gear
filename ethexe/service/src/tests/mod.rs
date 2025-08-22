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

//! Integration tests.

pub(crate) mod utils;

use crate::{
    Service,
    config::{self, Config},
    tests::utils::{
        EnvNetworkConfig, Node, NodeConfig, TestEnv, TestEnvConfig, ValidatorsConfig, init_logger,
    },
};
use alloy::providers::{Provider as _, ext::AnvilApi};
use ethexe_common::{
    ScheduledTask,
    db::{BlockMetaStorageRead, CodesStorageRead, OnChainStorageRead},
    events::{BlockEvent, MirrorEvent, RouterEvent},
    gear::Origin,
};
use ethexe_db::{Database, verifier::IntegrityVerifier};
use ethexe_observer::EthereumConfig;
use ethexe_prometheus::PrometheusConfig;
use ethexe_rpc::{RpcConfig, test_utils::JsonRpcResponse};
use ethexe_runtime_common::state::{Expiring, MailboxMessage, PayloadLookup, Storage};
use ethexe_tx_pool::{OffchainTransaction, RawOffchainTransaction};
use gear_core::{
    ids::prelude::*,
    message::{ReplyCode, SuccessReplyReason},
};
use gear_core_errors::{ErrorReplyReason, SimpleExecutionError, SimpleUnavailableActorError};
use gprimitives::{ActorId, H160, H256, MessageId};
use parity_scale_codec::Encode;
use std::{
    collections::{BTreeMap, BTreeSet},
    net::{Ipv4Addr, SocketAddr},
    time::Duration,
};
use tempfile::tempdir;

#[ignore = "until rpc fixed"]
#[tokio::test]
async fn basics() {
    init_logger();

    let tmp_dir = tempdir().unwrap();
    let tmp_dir = tmp_dir.path().to_path_buf();

    let node_cfg = config::NodeConfig {
        database_path: tmp_dir.join("db"),
        key_path: tmp_dir.join("key"),
        validator: Default::default(),
        validator_session: Default::default(),
        eth_max_sync_depth: 1_000,
        worker_threads: None,
        blocking_threads: None,
        chunk_processing_threads: 16,
        block_gas_limit: 4_000_000_000_000,
        gas_limit_multiplier: 10,
        dev: true,
        fast_sync: false,
    };

    let eth_cfg = EthereumConfig {
        rpc: "wss://reth-rpc.gear-tech.io/ws".into(),
        beacon_rpc: "https://eth-holesky-beacon.public.blastapi.io".into(),
        router_address: "0x051193e518181887088df3891cA0E5433b094A4a"
            .parse()
            .expect("infallible"),
        block_time: Duration::from_secs(12),
    };

    let mut config = Config {
        node: node_cfg,
        ethereum: eth_cfg,
        network: None,
        rpc: None,
        prometheus: None,
    };

    Service::new(&config).await.unwrap();

    // Enable all optional services
    config.network = Some(ethexe_network::NetworkConfig::new_local(
        tmp_dir.join("net"),
    ));

    config.rpc = Some(RpcConfig {
        listen_addr: SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 9944),
        cors: None,
        dev: true,
    });

    config.prometheus = Some(PrometheusConfig::new(
        "DevNode".into(),
        SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 9635),
    ));

    Service::new(&config).await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
#[ntest::timeout(60_000)]
async fn ping() {
    init_logger();

    let mut env = TestEnv::new(Default::default()).await.unwrap();

    let mut node = env.new_node(NodeConfig::default().validator(env.validators[0]));
    node.start_service().await;

    let res = env
        .upload_code(demo_ping::WASM_BINARY)
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();
    assert!(res.valid);

    let code_id = res.code_id;

    let code = node
        .db
        .original_code(code_id)
        .expect("After approval, the code is guaranteed to be in the database");
    assert_eq!(code, demo_ping::WASM_BINARY);

    let _ = node
        .db
        .instrumented_code(1, code_id)
        .expect("After approval, instrumented code is guaranteed to be in the database");
    let res = env
        .create_program(code_id, 500_000_000_000_000)
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();
    assert_eq!(res.code_id, code_id);

    let res = env
        .send_message(res.program_id, b"PING", 0)
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();

    assert_eq!(res.code, ReplyCode::Success(SuccessReplyReason::Manual));
    assert_eq!(res.payload, b"PONG");
    assert_eq!(res.value, 0);

    let ping_id = res.program_id;

    env.approve_wvara(ping_id).await;

    let res = env
        .send_message(ping_id, b"PING", 0)
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();
    assert_eq!(res.program_id, ping_id);
    assert_eq!(res.code, ReplyCode::Success(SuccessReplyReason::Manual));
    assert_eq!(res.payload, b"PONG");
    assert_eq!(res.value, 0);

    let res = env
        .send_message(ping_id, b"PUNK", 0)
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();
    assert_eq!(res.program_id, ping_id);
    assert_eq!(res.code, ReplyCode::Success(SuccessReplyReason::Auto));
    assert_eq!(res.payload, b"");
    assert_eq!(res.value, 0);
}

#[tokio::test(flavor = "multi_thread")]
#[ntest::timeout(60_000)]
async fn uninitialized_program() {
    init_logger();

    let mut env = TestEnv::new(Default::default()).await.unwrap();

    let mut node = env.new_node(NodeConfig::default().validator(env.validators[0]));
    node.start_service().await;

    let res = env
        .upload_code(demo_async_init::WASM_BINARY)
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();

    assert!(res.valid);

    let code_id = res.code_id;

    // Case #1: Init failed due to panic in init (decoding).
    {
        let res = env
            .create_program(code_id, 500_000_000_000_000)
            .await
            .unwrap()
            .wait_for()
            .await
            .unwrap();

        let reply = env
            .send_message(res.program_id, &[], 0)
            .await
            .unwrap()
            .wait_for()
            .await
            .unwrap();

        let expected_err = ReplyCode::Error(SimpleExecutionError::UserspacePanic.into());
        assert_eq!(reply.code, expected_err);

        let res = env
            .send_message(res.program_id, &[], 0)
            .await
            .unwrap()
            .wait_for()
            .await
            .unwrap();

        let expected_err = ReplyCode::Error(ErrorReplyReason::UnavailableActor(
            SimpleUnavailableActorError::InitializationFailure,
        ));
        assert_eq!(res.code, expected_err);
    }

    // Case #2: async init, replies are acceptable.
    {
        let init_payload = demo_async_init::InputArgs {
            approver_first: env.sender_id,
            approver_second: env.sender_id,
            approver_third: env.sender_id,
        }
        .encode();

        let mut listener = env.observer_events_publisher().subscribe().await;

        let init_res = env
            .create_program(code_id, 500_000_000_000_000)
            .await
            .unwrap()
            .wait_for()
            .await
            .unwrap();
        let init_reply = env
            .send_message(init_res.program_id, &init_payload, 0)
            .await
            .unwrap();
        let mirror = env.ethereum.mirror(init_res.program_id.try_into().unwrap());

        let mut msgs_for_reply = vec![];

        listener
            .apply_until_block_event(|event| match event {
                BlockEvent::Mirror {
                    actor_id,
                    event:
                        MirrorEvent::Message {
                            id, destination, ..
                        },
                } if actor_id == init_res.program_id && destination == env.sender_id => {
                    msgs_for_reply.push(id);

                    if msgs_for_reply.len() == 3 {
                        Ok(Some(()))
                    } else {
                        Ok(None)
                    }
                }
                _ => Ok(None),
            })
            .await
            .unwrap();

        // Handle message to uninitialized program.
        let res = env
            .send_message(init_res.program_id, &[], 0)
            .await
            .unwrap()
            .wait_for()
            .await
            .unwrap();
        let expected_err = ReplyCode::Error(ErrorReplyReason::UnavailableActor(
            SimpleUnavailableActorError::Uninitialized,
        ));
        assert_eq!(res.code, expected_err);
        // Checking further initialization.

        // Required replies.
        for mid in msgs_for_reply {
            mirror.send_reply(mid, [], 0).await.unwrap();
        }

        // Success end of initialization.
        let code = listener
            .apply_until_block_event(|event| match event {
                BlockEvent::Mirror {
                    actor_id,
                    event:
                        MirrorEvent::Reply {
                            reply_code,
                            reply_to,
                            ..
                        },
                } if actor_id == init_res.program_id && reply_to == init_reply.message_id => {
                    Ok(Some(reply_code))
                }
                _ => Ok(None),
            })
            .await
            .unwrap();

        assert!(code.is_success());

        // Handle message handled, but panicked due to incorrect payload as expected.
        let res = env
            .send_message(res.program_id, &[], 0)
            .await
            .unwrap()
            .wait_for()
            .await
            .unwrap();

        let expected_err = ReplyCode::Error(SimpleExecutionError::UserspacePanic.into());
        assert_eq!(res.code, expected_err);
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ntest::timeout(60_000)]
async fn mailbox() {
    init_logger();

    let mut env = TestEnv::new(Default::default()).await.unwrap();

    let mut node = env.new_node(NodeConfig::default().validator(env.validators[0]));
    node.start_service().await;

    let res = env
        .upload_code(demo_async::WASM_BINARY)
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();

    assert!(res.valid);

    let code_id = res.code_id;

    let res = env
        .create_program(code_id, 500_000_000_000_000)
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();

    let init_res = env
        .send_message(res.program_id, &env.sender_id.encode(), 0)
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();
    assert_eq!(init_res.code, ReplyCode::Success(SuccessReplyReason::Auto));

    let pid = res.program_id;

    env.approve_wvara(pid).await;

    let res = env
        .send_message(pid, &demo_async::Command::Mutex.encode(), 0)
        .await
        .unwrap();

    let original_mid = res.message_id;
    let mid_expected_message = MessageId::generate_outgoing(original_mid, 0);
    let ping_expected_message = MessageId::generate_outgoing(original_mid, 1);

    let mut listener = env.observer_events_publisher().subscribe().await;
    let block_data = listener
        .apply_until_block_event_with_header(|event, block_data| match event {
            BlockEvent::Mirror { actor_id, event } if actor_id == pid => {
                if let MirrorEvent::Message {
                    id,
                    destination,
                    payload,
                    ..
                } = event
                {
                    assert_eq!(destination, env.sender_id);

                    if id == mid_expected_message {
                        assert_eq!(payload, res.message_id.encode());
                        Ok(None)
                    } else if id == ping_expected_message {
                        assert_eq!(payload, b"PING");
                        Ok(Some(block_data.clone()))
                    } else {
                        unreachable!()
                    }
                } else {
                    Ok(None)
                }
            }
            _ => Ok(None),
        })
        .await
        .unwrap();

    // -1 bcs execution took place in previous block, not the one that emits events.
    let wake_expiry = block_data.header.height - 1 + 100; // 100 is default wait for.
    let expiry = block_data.header.height - 1 + ethexe_runtime_common::state::MAILBOX_VALIDITY;

    let expected_schedule = BTreeMap::from_iter([
        (
            wake_expiry,
            BTreeSet::from_iter([ScheduledTask::WakeMessage(pid, original_mid)]),
        ),
        (
            expiry,
            BTreeSet::from_iter([
                ScheduledTask::RemoveFromMailbox((pid, env.sender_id), mid_expected_message),
                ScheduledTask::RemoveFromMailbox((pid, env.sender_id), ping_expected_message),
            ]),
        ),
    ]);

    let schedule = node
        .db
        .block_schedule(block_data.header.parent_hash)
        .expect("must exist");

    assert_eq!(schedule, expected_schedule);

    let mid_payload = PayloadLookup::Direct(original_mid.into_bytes().to_vec().try_into().unwrap());
    let ping_payload = PayloadLookup::Direct(b"PING".to_vec().try_into().unwrap());

    let expected_mailbox = BTreeMap::from_iter([(
        env.sender_id,
        BTreeMap::from_iter([
            (
                mid_expected_message,
                Expiring {
                    value: MailboxMessage {
                        payload: mid_payload.clone(),
                        value: 0,
                        origin: Origin::Ethereum,
                    },
                    expiry,
                },
            ),
            (
                ping_expected_message,
                Expiring {
                    value: MailboxMessage {
                        payload: ping_payload,
                        value: 0,
                        origin: Origin::Ethereum,
                    },
                    expiry,
                },
            ),
        ]),
    )]);

    let mirror = env.ethereum.mirror(pid.try_into().unwrap());
    let state_hash = mirror.query().state_hash().await.unwrap();

    let state = node.db.program_state(state_hash).unwrap();
    assert!(!state.mailbox_hash.is_empty());
    let mailbox = state
        .mailbox_hash
        .map_or_default(|hash| node.db.mailbox(hash).unwrap());

    assert_eq!(mailbox.into_values(&node.db), expected_mailbox);

    mirror
        .send_reply(ping_expected_message, "PONG", 0)
        .await
        .unwrap();

    let initial_message = res.message_id;
    let reply_info = res.wait_for().await.unwrap();
    assert_eq!(
        reply_info.code,
        ReplyCode::Success(SuccessReplyReason::Manual)
    );
    assert_eq!(reply_info.payload, initial_message.encode());

    let state_hash = mirror.query().state_hash().await.unwrap();

    let state = node.db.program_state(state_hash).unwrap();
    assert!(!state.mailbox_hash.is_empty());
    let mailbox = state
        .mailbox_hash
        .map_or_default(|hash| node.db.mailbox(hash).unwrap());

    let expected_mailbox = BTreeMap::from_iter([(
        env.sender_id,
        BTreeMap::from_iter([(
            mid_expected_message,
            Expiring {
                value: MailboxMessage {
                    payload: mid_payload,
                    value: 0,
                    origin: Origin::Ethereum,
                },
                expiry,
            },
        )]),
    )]);

    assert_eq!(mailbox.into_values(&node.db), expected_mailbox);

    mirror.claim_value(mid_expected_message).await.unwrap();

    let block_data = listener
        .apply_until_block_event_with_header(|event, block_data| match event {
            BlockEvent::Mirror { actor_id, event } if actor_id == pid => match event {
                MirrorEvent::ValueClaimed { claimed_id, .. }
                    if claimed_id == mid_expected_message =>
                {
                    Ok(Some(block_data.clone()))
                }
                _ => Ok(None),
            },
            _ => Ok(None),
        })
        .await
        .unwrap();

    let state_hash = mirror.query().state_hash().await.unwrap();

    let state = node.db.program_state(state_hash).unwrap();
    assert!(state.mailbox_hash.is_empty());

    let schedule = node
        .db
        .block_schedule(block_data.header.parent_hash)
        .expect("must exist");
    assert!(schedule.is_empty(), "{schedule:?}");
}

#[tokio::test(flavor = "multi_thread")]
#[ntest::timeout(60_000)]
async fn incoming_transfers() {
    init_logger();

    let mut env = TestEnv::new(Default::default()).await.unwrap();

    let mut node = env.new_node(NodeConfig::default().validator(env.validators[0]));
    node.start_service().await;

    let res = env
        .upload_code(demo_ping::WASM_BINARY)
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();

    let code_id = res.code_id;
    let res = env
        .create_program(code_id, 500_000_000_000_000)
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();

    let _ = env
        .send_message(res.program_id, &env.sender_id.encode(), 0)
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();

    let ping_id = res.program_id;

    let wvara = env.ethereum.router().wvara();

    assert_eq!(wvara.query().decimals().await.unwrap(), 12);

    let ping = env.ethereum.mirror(ping_id.to_address_lossy().into());

    let on_eth_balance = wvara
        .query()
        .balance_of(ping.address().0.into())
        .await
        .unwrap();
    assert_eq!(on_eth_balance, 0);

    let state_hash = ping.query().state_hash().await.unwrap();
    let local_balance = node.db.program_state(state_hash).unwrap().balance;
    assert_eq!(local_balance, 0);

    // 1_000 tokens
    const VALUE_SENT: u128 = 1_000_000_000_000_000;

    let mut listener = env.observer_events_publisher().subscribe().await;

    env.transfer_wvara(ping_id, VALUE_SENT).await;

    listener
        .apply_until_block_event(|e| {
            Ok(matches!(e, BlockEvent::Router(RouterEvent::BatchCommitted { .. })).then_some(()))
        })
        .await
        .unwrap();

    let on_eth_balance = wvara
        .query()
        .balance_of(ping.address().0.into())
        .await
        .unwrap();
    assert_eq!(on_eth_balance, VALUE_SENT);

    let state_hash = ping.query().state_hash().await.unwrap();
    let local_balance = node.db.program_state(state_hash).unwrap().balance;
    assert_eq!(local_balance, VALUE_SENT);

    env.approve_wvara(ping_id).await;

    let res = env
        .send_message(ping_id, b"PING", VALUE_SENT)
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();

    assert_eq!(res.code, ReplyCode::Success(SuccessReplyReason::Manual));
    assert_eq!(res.value, 0);

    let on_eth_balance = wvara
        .query()
        .balance_of(ping.address().0.into())
        .await
        .unwrap();
    assert_eq!(on_eth_balance, 2 * VALUE_SENT);

    let state_hash = ping.query().state_hash().await.unwrap();
    let local_balance = node.db.program_state(state_hash).unwrap().balance;
    assert_eq!(local_balance, 2 * VALUE_SENT);
}

#[tokio::test(flavor = "multi_thread")]
#[ntest::timeout(60_000)]
async fn ping_reorg() {
    init_logger();

    let mut env = TestEnv::new(Default::default()).await.unwrap();

    let mut node = env.new_node(NodeConfig::default().validator(env.validators[0]));
    node.start_service().await;

    let res = env
        .upload_code(demo_ping::WASM_BINARY)
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();
    assert!(res.valid);

    let code_id = res.code_id;

    log::info!("📗 Abort service to simulate node blocks skipping");
    node.stop_service().await;

    let create_program = env
        .create_program(code_id, 500_000_000_000_000)
        .await
        .unwrap();
    let init = env
        .send_message(create_program.program_id, b"PING", 0)
        .await
        .unwrap();
    // Mine some blocks to check missed blocks support
    env.skip_blocks(10).await;

    // Start new service
    node.start_service().await;

    // IMPORTANT: Mine one block to sent block event to the new service.
    env.force_new_block().await;

    let res = create_program.wait_for().await.unwrap();
    let init_res = init.wait_for().await.unwrap();
    assert_eq!(res.code_id, code_id);
    assert_eq!(init_res.payload, b"PONG");

    let ping_id = res.program_id;

    env.approve_wvara(ping_id).await;

    log::info!(
        "📗 Create snapshot for block: {}, where ping program is already created",
        env.provider.get_block_number().await.unwrap()
    );
    let program_created_snapshot_id = env.provider.anvil_snapshot().await.unwrap();

    let res = env
        .send_message(ping_id, b"PING", 0)
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();
    assert_eq!(res.program_id, ping_id);
    assert_eq!(res.payload, b"PONG");

    log::info!("📗 Test after reverting to the program creation snapshot");
    env.provider
        .anvil_revert(program_created_snapshot_id)
        .await
        .map(|res| assert!(res))
        .unwrap();

    let res = env
        .send_message(ping_id, b"PING", 0)
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();
    assert_eq!(res.program_id, ping_id);
    assert_eq!(res.payload, b"PONG");

    // The last step is to test correctness after db cleanup
    node.stop_service().await;
    node.db = Database::memory();

    log::info!("📗 Test after db cleanup and service shutting down");
    let send_message = env.send_message(ping_id, b"PING", 0).await.unwrap();

    // Skip some blocks to simulate long time without service
    env.skip_blocks(10).await;

    node.start_service().await;

    // Important: mine one block to sent block event to the new service.
    env.force_new_block().await;

    let res = send_message.wait_for().await.unwrap();
    assert_eq!(res.program_id, ping_id);
    assert_eq!(res.payload, b"PONG");
}

// Stop service - waits 150 blocks - send message - waits 150 blocks - start service.
// Deep sync must load chain in batch.
#[tokio::test(flavor = "multi_thread")]
#[ntest::timeout(60_000)]
async fn ping_deep_sync() {
    init_logger();

    let mut env = TestEnv::new(Default::default()).await.unwrap();

    let mut node = env.new_node(NodeConfig::default().validator(env.validators[0]));
    node.start_service().await;

    let res = env
        .upload_code(demo_ping::WASM_BINARY)
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();
    assert!(res.valid);

    let code_id = res.code_id;

    let res = env
        .create_program(code_id, 500_000_000_000_000)
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();
    let init_res = env
        .send_message(res.program_id, b"PING", 0)
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();
    assert_eq!(res.code_id, code_id);
    assert_eq!(init_res.payload, b"PONG");
    assert_eq!(init_res.value, 0);
    assert_eq!(
        init_res.code,
        ReplyCode::Success(SuccessReplyReason::Manual)
    );

    let ping_id = res.program_id;

    node.stop_service().await;

    env.skip_blocks(150).await;

    env.approve_wvara(ping_id).await;

    let send_message = env.send_message(ping_id, b"PING", 0).await.unwrap();

    env.skip_blocks(150).await;

    node.start_service().await;

    // Important: mine one block to sent block event to the started service.
    env.force_new_block().await;

    let res = send_message.wait_for().await.unwrap();
    assert_eq!(res.program_id, ping_id);
    assert_eq!(res.payload, b"PONG");
    assert_eq!(res.value, 0);
    assert_eq!(res.code, ReplyCode::Success(SuccessReplyReason::Manual));
}

#[tokio::test(flavor = "multi_thread")]
#[ntest::timeout(60_000)]
async fn multiple_validators() {
    init_logger();

    let config = TestEnvConfig {
        validators: ValidatorsConfig::PreDefined(3),
        network: EnvNetworkConfig::Enabled,
        ..Default::default()
    };
    let mut env = TestEnv::new(config).await.unwrap();

    assert_eq!(
        env.validators.len(),
        3,
        "Currently only 3 validators are supported for this test"
    );
    assert!(
        !env.continuous_block_generation,
        "Currently continuous block generation is not supported for this test"
    );

    let mut validators = vec![];
    for (i, v) in env.validators.clone().into_iter().enumerate() {
        log::info!("📗 Starting validator-{i}");
        let mut validator = env.new_node(NodeConfig::named(format!("validator-{i}")).validator(v));
        validator.start_service().await;
        validators.push(validator);
    }

    let res = env
        .upload_code(demo_ping::WASM_BINARY)
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();
    assert!(res.valid);

    let ping_code_id = res.code_id;

    let res = env
        .create_program(ping_code_id, 500_000_000_000_000)
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();
    let init_res = env
        .send_message(res.program_id, b"", 0)
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();
    assert_eq!(res.code_id, ping_code_id);
    assert_eq!(init_res.payload, b"");
    assert_eq!(init_res.value, 0);
    assert_eq!(init_res.code, ReplyCode::Success(SuccessReplyReason::Auto));

    let ping_id = res.program_id;

    let res = env
        .upload_code(demo_async::WASM_BINARY)
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();
    assert!(res.valid);

    let async_code_id = res.code_id;

    let res = env
        .create_program(async_code_id, 500_000_000_000_000)
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();
    let init_res = env
        .send_message(res.program_id, ping_id.encode().as_slice(), 0)
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();
    assert_eq!(res.code_id, async_code_id);
    assert_eq!(init_res.payload, b"");
    assert_eq!(init_res.value, 0);
    assert_eq!(init_res.code, ReplyCode::Success(SuccessReplyReason::Auto));

    let async_id = res.program_id;

    env.approve_wvara(ping_id).await;
    env.approve_wvara(async_id).await;

    let res = env
        .send_message(async_id, demo_async::Command::Common.encode().as_slice(), 0)
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();
    assert_eq!(res.program_id, async_id);
    assert_eq!(res.payload, res.message_id.encode().as_slice());
    assert_eq!(res.value, 0);
    assert_eq!(res.code, ReplyCode::Success(SuccessReplyReason::Manual));

    log::info!("📗 Stop validator 0 and check, that ethexe is still working");
    if env.next_block_producer_index().await == 0 {
        log::info!("📗 Skip one block to be sure validator 0 is not a producer for next block");
        env.force_new_block().await;
    }
    validators[0].stop_service().await;

    let res = env
        .send_message(async_id, demo_async::Command::Common.encode().as_slice(), 0)
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();
    assert_eq!(res.payload, res.message_id.encode().as_slice());

    log::info!("📗 Stop validator 1 and check, that ethexe is not working after");
    if env.next_block_producer_index().await == 1 {
        log::info!("📗 Skip one block to be sure validator 1 is not a producer for next block");
        env.force_new_block().await;
    }
    validators[1].stop_service().await;

    let wait_for_reply_to = env
        .send_message(async_id, demo_async::Command::Common.encode().as_slice(), 0)
        .await
        .unwrap();

    tokio::time::timeout(env.block_time * 5, wait_for_reply_to.clone().wait_for())
        .await
        .expect_err("Timeout expected");

    log::info!(
        "📗 Re-start validator 0 and check, that now ethexe is working, validator 1 is still stopped"
    );
    validators[0].start_service().await;

    if env.next_block_producer_index().await == 1 {
        log::info!("📗 Skip one block to be sure validator 1 is not a producer for next block");
        env.force_new_block().await;
    }

    // IMPORTANT: mine one block to send a new block event.
    env.force_new_block().await;

    let res = wait_for_reply_to.wait_for().await.unwrap();
    assert_eq!(res.payload, res.message_id.encode().as_slice());
}

#[tokio::test(flavor = "multi_thread")]
#[ntest::timeout(60_000)]
async fn tx_pool_gossip() {
    init_logger();

    let test_env_config = TestEnvConfig {
        validators: ValidatorsConfig::PreDefined(2),
        network: EnvNetworkConfig::Enabled,
        ..Default::default()
    };

    // Setup env of 2 nodes, one of them knows about the other one.
    let mut env = TestEnv::new(test_env_config).await.unwrap();

    log::info!("📗 Starting node 0");
    let mut node0 = env.new_node(
        NodeConfig::default()
            .validator(env.validators[0])
            .service_rpc(9505),
    );
    node0.start_service().await;

    log::info!("📗 Starting node 1");
    let mut node1 = env.new_node(NodeConfig::default().validator(env.validators[1]));
    node1.start_service().await;

    log::info!("Populate node-0 and node-1 with 2 valid blocks");

    env.force_new_block().await;
    env.force_new_block().await;

    // Give some time for nodes to process the blocks
    tokio::time::sleep(Duration::from_secs(2)).await;
    let reference_block = node0
        .db
        .latest_computed_block()
        .expect("at least genesis block is latest valid")
        .0;

    // Prepare tx data
    let signed_ethexe_tx = {
        let sender_pub_key = env.signer.generate_key().expect("failed generating key");

        let ethexe_tx = OffchainTransaction {
            raw: RawOffchainTransaction::SendMessage {
                program_id: H160::random(),
                payload: vec![],
            },
            // referring to the latest valid block hash
            reference_block,
        };
        env.signer.signed_data(sender_pub_key, ethexe_tx).unwrap()
    };

    let tx_hash = signed_ethexe_tx.tx_hash();
    let (transaction, signature) = signed_ethexe_tx.clone().into_parts();

    // Send request
    log::info!("Sending tx pool request to node-1");
    let rpc_client = node0.rpc_client().expect("rpc server is set");
    let resp = rpc_client
        .send_message(transaction, signature.encode())
        .await
        .expect("failed sending request");
    assert!(resp.status().is_success());
    let resp_tx_hash = JsonRpcResponse::new(resp)
        .await
        .expect("failed to deserialize json response from rpc")
        .try_extract_res::<H256>()
        .expect("failed to deserialize reply info");
    assert_eq!(resp_tx_hash, tx_hash);

    // Tx executable validation takes time.
    // Sleep for a while so tx is processed by both nodes.
    tokio::time::sleep(Duration::from_secs(12)).await;

    // Check that node-1 received the message
    let node1_db_tx = node1
        .db
        .get_offchain_transaction(tx_hash)
        .expect("tx not found");
    assert_eq!(node1_db_tx, signed_ethexe_tx);
}

#[tokio::test(flavor = "multi_thread")]
#[ntest::timeout(60_000)]
async fn fast_sync() {
    init_logger();

    let assert_chain = |latest_block, fast_synced_block, alice: &Node, bob: &Node| {
        log::info!("Assert chain in range {latest_block}..{fast_synced_block}");

        IntegrityVerifier::new(alice.db.clone())
            .verify_chain(latest_block, fast_synced_block)
            .expect("failed to verify Alice database");

        IntegrityVerifier::new(bob.db.clone())
            .verify_chain(latest_block, fast_synced_block)
            .expect("failed to verify Bob database");

        assert_eq!(
            alice.db.latest_computed_block(),
            bob.db.latest_computed_block()
        );

        let mut block = latest_block;
        loop {
            if fast_synced_block == block {
                break;
            }

            log::trace!("assert block {block}");

            assert_eq!(
                alice.db.block_codes_queue(block),
                bob.db.block_codes_queue(block)
            );

            assert_eq!(
                alice.db.block_meta(block).computed,
                bob.db.block_meta(block).computed
            );
            assert_eq!(
                alice.db.block_program_states(block),
                bob.db.block_program_states(block)
            );
            assert_eq!(alice.db.block_outcome(block), bob.db.block_outcome(block));
            assert_eq!(alice.db.block_schedule(block), bob.db.block_schedule(block));

            assert_eq!(alice.db.block_header(block), bob.db.block_header(block));
            assert_eq!(alice.db.block_events(block), bob.db.block_events(block));
            assert_eq!(
                alice.db.block_meta(block).synced,
                bob.db.block_meta(block).synced,
            );

            let header = alice.db.block_header(block).unwrap();
            block = header.parent_hash;
        }
    };

    let config = TestEnvConfig {
        network: EnvNetworkConfig::Enabled,
        ..Default::default()
    };
    let mut env = TestEnv::new(config).await.unwrap();

    log::info!("Starting Alice");
    let mut alice = env.new_node(NodeConfig::named("Alice").validator(env.validators[0]));
    alice.start_service().await;

    log::info!("Creating `demo-autoreply` programs");

    let code_info = env
        .upload_code(demo_mul_by_const::WASM_BINARY)
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();

    let code_id = code_info.code_id;
    let mut program_ids = [ActorId::zero(); 8];

    for (i, program_id) in program_ids.iter_mut().enumerate() {
        let program_info = env
            .create_program(code_id, 500_000_000_000_000)
            .await
            .unwrap()
            .wait_for()
            .await
            .unwrap();

        *program_id = program_info.program_id;

        let value = i as u64 % 3;
        let _reply_info = env
            .send_message(program_info.program_id, &value.encode(), 0)
            .await
            .unwrap()
            .wait_for()
            .await
            .unwrap();
    }

    let latest_block = env.latest_block().await.hash.0.into();
    alice
        .listener()
        .wait_for_block_processed(latest_block)
        .await;

    log::info!("Starting Bob (fast-sync)");
    let mut bob = env.new_node(NodeConfig::named("Bob").fast_sync());

    bob.start_service().await;

    log::info!("Sending messages to programs");

    for (i, program_id) in program_ids.into_iter().enumerate() {
        let reply_info = env
            .send_message(program_id, &(i as u64).encode(), 0)
            .await
            .unwrap()
            .wait_for()
            .await
            .unwrap();
        assert_eq!(
            reply_info.code,
            ReplyCode::Success(SuccessReplyReason::Manual)
        );
    }

    let latest_block = env.latest_block().await.hash.0.into();
    alice
        .listener()
        .wait_for_block_processed(latest_block)
        .await;
    bob.listener().wait_for_block_processed(latest_block).await;

    log::info!("Stopping Bob");
    bob.stop_service().await;

    assert_chain(
        latest_block,
        bob.latest_fast_synced_block.take().unwrap(),
        &alice,
        &bob,
    );

    for (i, program_id) in program_ids.into_iter().enumerate() {
        let i = (i * 3) as u64;
        let reply_info = env
            .send_message(program_id, &i.encode(), 0)
            .await
            .unwrap()
            .wait_for()
            .await
            .unwrap();
        assert_eq!(
            reply_info.code,
            ReplyCode::Success(SuccessReplyReason::Manual)
        );
    }

    env.skip_blocks(100).await;

    let latest_block = env.latest_block().await.hash.0.into();
    alice
        .listener()
        .wait_for_block_processed(latest_block)
        .await;

    log::info!("Starting Bob again to check how it handles partially empty database");
    bob.start_service().await;

    // mine a block so Bob can produce the event we will wait for
    env.skip_blocks(1).await;

    let latest_block = env.latest_block().await.hash.0.into();
    alice
        .listener()
        .wait_for_block_processed(latest_block)
        .await;
    bob.listener().wait_for_block_processed(latest_block).await;

    assert_chain(
        latest_block,
        bob.latest_fast_synced_block.take().unwrap(),
        &alice,
        &bob,
    );
}
