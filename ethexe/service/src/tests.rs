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

use crate::{
    config::{self, Config},
    tests::utils::Node,
    Service,
};
use alloy::{
    node_bindings::{Anvil, AnvilInstance},
    providers::{ext::AnvilApi, Provider as _, RootProvider},
    rpc::types::{anvil::MineOptions, Header as RpcHeader},
};
use anyhow::Result;
use ethexe_common::{
    db::{BlockMetaStorage, CodesStorage, OnChainStorage},
    events::{BlockEvent, MirrorEvent, RouterEvent},
    gear::Origin,
    ScheduledTask,
};
use ethexe_compute::{BlockProcessed, ComputeEvent};
use ethexe_db::Database;
use ethexe_ethereum::Ethereum;
use ethexe_observer::{BlobReader, EthereumConfig, MockBlobReader};
use ethexe_processor::Processor;
use ethexe_prometheus::PrometheusConfig;
use ethexe_rpc::{test_utils::RpcClient, RpcConfig};
use ethexe_runtime_common::state::{Expiring, MailboxMessage, PayloadLookup, Storage};
use ethexe_signer::Signer;
use ethexe_tx_pool::{OffchainTransaction, RawOffchainTransaction};
use gear_core::{
    ids::prelude::*,
    message::{ReplyCode, SuccessReplyReason},
};
use gear_core_errors::{ErrorReplyReason, SimpleExecutionError, SimpleUnavailableActorError};
use gprimitives::{ActorId, CodeId, MessageId, H160, H256};
use parity_scale_codec::Encode;
use std::{
    collections::{BTreeMap, BTreeSet},
    net::{Ipv4Addr, SocketAddr},
    time::Duration,
};
use tempfile::tempdir;
use tokio::task::{self, JoinHandle};
use utils::{EnvNetworkConfig, NodeConfig, TestEnv, TestEnvConfig, ValidatorsConfig};

#[ignore = "until rpc fixed"]
#[tokio::test]
async fn basics() {
    utils::init_logger();

    let tmp_dir = tempdir().unwrap();
    let tmp_dir = tmp_dir.path().to_path_buf();

    let node_cfg = config::NodeConfig {
        database_path: tmp_dir.join("db"),
        key_path: tmp_dir.join("key"),
        validator: Default::default(),
        validator_session: Default::default(),
        eth_max_sync_depth: 1_000,
        worker_threads_override: None,
        virtual_threads: 16,
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
    utils::init_logger();

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
    assert_eq!(res.code, demo_ping::WASM_BINARY);
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
    utils::init_logger();

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
    utils::init_logger();

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

    let state = node.db.read_state(state_hash).unwrap();
    assert!(!state.mailbox_hash.is_empty());
    let mailbox = state
        .mailbox_hash
        .map_or_default(|hash| node.db.read_mailbox(hash).unwrap());

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

    let state = node.db.read_state(state_hash).unwrap();
    assert!(!state.mailbox_hash.is_empty());
    let mailbox = state
        .mailbox_hash
        .map_or_default(|hash| node.db.read_mailbox(hash).unwrap());

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

    let state = node.db.read_state(state_hash).unwrap();
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
    utils::init_logger();

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
    let local_balance = node.db.read_state(state_hash).unwrap().balance;
    assert_eq!(local_balance, 0);

    // 1_000 tokens
    const VALUE_SENT: u128 = 1_000_000_000_000_000;

    let mut listener = env.observer_events_publisher().subscribe().await;

    env.transfer_wvara(ping_id, VALUE_SENT).await;

    listener
        .apply_until_block_event(|e| {
            Ok(matches!(e, BlockEvent::Router(RouterEvent::BlockCommitted { .. })).then_some(()))
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
    let local_balance = node.db.read_state(state_hash).unwrap().balance;
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
    let local_balance = node.db.read_state(state_hash).unwrap().balance;
    assert_eq!(local_balance, 2 * VALUE_SENT);
}

#[tokio::test(flavor = "multi_thread")]
#[ntest::timeout(60_000)]
async fn ping_reorg() {
    utils::init_logger();

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
    utils::init_logger();

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
    assert_eq!(res.code.as_slice(), demo_ping::WASM_BINARY);
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
    utils::init_logger();

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
    assert_eq!(res.code, demo_ping::WASM_BINARY);
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
    assert_eq!(res.code, demo_async::WASM_BINARY);
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
    utils::init_logger();

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

    let (transaction, signature) = signed_ethexe_tx.clone().into_parts();

    // Send request
    log::info!("Sending tx pool request to node-1");
    let rpc_client = node0.rpc_client().expect("rpc server is set");
    let resp = rpc_client
        .send_message(transaction, signature.encode())
        .await
        .expect("failed sending request");
    assert!(resp.status().is_success());

    // This way the response from RPC server is checked to be `Ok`.
    // In case of error RPC returns the `Ok` response with error message.
    let resp = resp
        .json::<serde_json::Value>()
        .await
        .expect("failed to deserialize json response from rpc");
    assert!(resp.get("result").is_some());

    // Tx executable validation takes time.
    // Sleep for a while so tx is processed by both nodes.
    tokio::time::sleep(Duration::from_secs(12)).await;

    // Check that node-1 received the message
    let tx_hash = signed_ethexe_tx.tx_hash();
    let node1_db_tx = node1
        .db
        .get_offchain_transaction(tx_hash)
        .expect("tx not found");
    assert_eq!(node1_db_tx, signed_ethexe_tx);
}

#[tokio::test(flavor = "multi_thread")]
#[ntest::timeout(60_000)]
async fn fast_sync() {
    utils::init_logger();

    let assert_chain = |latest_block, fast_synced_block, alice: &Node, bob: &Node| {
        log::info!("Assert chain in range {latest_block}..{fast_synced_block}");

        assert_eq!(alice.db.program_ids(), bob.db.program_ids());
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
                alice.db.block_commitment_queue(block),
                bob.db.block_commitment_queue(block)
            );
            assert_eq!(
                alice.db.block_codes_queue(block),
                bob.db.block_codes_queue(block)
            );

            assert_eq!(alice.db.block_computed(block), bob.db.block_computed(block));
            assert_eq!(
                alice.db.previous_not_empty_block(block),
                bob.db.previous_not_empty_block(block)
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
                alice.db.block_is_synced(block),
                bob.db.block_is_synced(block)
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

mod utils {
    use super::*;
    use crate::Event;
    use alloy::eips::BlockId;
    use ethexe_common::{
        db::OnChainStorage,
        ecdsa::{PrivateKey, PublicKey},
        Address, SimpleBlockData,
    };
    use ethexe_consensus::{ConsensusService, SimpleConnectService, ValidatorService};
    use ethexe_network::{export::Multiaddr, NetworkConfig, NetworkEvent, NetworkService};
    use ethexe_observer::{ObserverEvent, ObserverService};
    use ethexe_rpc::RpcService;
    use ethexe_tx_pool::TxPoolService;
    use futures::{executor::block_on, StreamExt};
    use gear_core::message::ReplyCode;
    use rand::{rngs::StdRng, SeedableRng};
    use roast_secp256k1_evm::frost::{
        keys::{self, IdentifierList, PublicKeyPackage, VerifiableSecretSharingCommitment},
        Identifier, SigningKey,
    };
    use std::{
        pin::Pin,
        sync::atomic::{AtomicUsize, Ordering},
    };
    use tokio::sync::broadcast::{self, Receiver, Sender};
    use tracing::Instrument;
    use tracing_subscriber::EnvFilter;

    /// Max network services which can be created by one test environment.
    const MAX_NETWORK_SERVICES_PER_TEST: usize = 1000;

    pub fn init_logger() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::from_default_env())
            .without_time()
            .try_init();
    }

    pub struct TestEnv {
        pub eth_cfg: EthereumConfig,
        #[allow(unused)]
        pub wallets: Wallets,
        pub blob_reader: MockBlobReader,
        pub provider: RootProvider,
        pub ethereum: Ethereum,
        pub signer: Signer,
        pub validators: Vec<ValidatorConfig>,
        pub sender_id: ActorId,
        pub threshold: u64,
        pub block_time: Duration,
        pub continuous_block_generation: bool,

        /// In order to reduce amount of observers, we create only one observer and broadcast events to all subscribers.
        broadcaster: Sender<ObserverEvent>,
        db: Database,
        /// If network is enabled by test, then we store here:
        /// network service polling thread, bootstrap address and nonce for new node address generation.
        bootstrap_network: Option<(JoinHandle<()>, String, usize)>,
        _anvil: Option<AnvilInstance>,
        _events_stream: JoinHandle<()>,
    }

    impl TestEnv {
        pub async fn new(config: TestEnvConfig) -> Result<Self> {
            let TestEnvConfig {
                validators,
                block_time,
                rpc,
                wallets,
                router_address,
                continuous_block_generation,
                network,
            } = config;

            log::info!(
                "📗 Starting new test environment. Continuous block generation: {}",
                continuous_block_generation
            );

            let (rpc_url, anvil) = match rpc {
                EnvRpcConfig::ProvidedURL(rpc_url) => {
                    log::info!("📍 Using provided RPC URL: {}", rpc_url);
                    (rpc_url, None)
                }
                EnvRpcConfig::CustomAnvil {
                    slots_in_epoch,
                    genesis_timestamp,
                } => {
                    let mut anvil = Anvil::new();

                    if continuous_block_generation {
                        anvil = anvil.block_time(block_time.as_secs())
                    }
                    if let Some(slots_in_epoch) = slots_in_epoch {
                        anvil = anvil.arg(format!("--slots-in-an-epoch={slots_in_epoch}"));
                    }
                    if let Some(genesis_timestamp) = genesis_timestamp {
                        anvil = anvil.arg(format!("--timestamp={genesis_timestamp}"));
                    }

                    let anvil = anvil.spawn();

                    log::info!("📍 Anvil started at {}", anvil.ws_endpoint());
                    (anvil.ws_endpoint(), Some(anvil))
                }
            };

            let signer = Signer::memory();

            let mut wallets = if let Some(wallets) = wallets {
                Wallets::custom(&signer, wallets)
            } else {
                Wallets::anvil(&signer)
            };

            let validators: Vec<_> = match validators {
                ValidatorsConfig::PreDefined(amount) => {
                    (0..amount).map(|_| wallets.next()).collect()
                }
                ValidatorsConfig::Custom(keys) => keys
                    .iter()
                    .map(|k| {
                        let private_key = k.parse().unwrap();
                        signer.storage_mut().add_key(private_key).unwrap()
                    })
                    .collect(),
            };

            let (validators, verifiable_secret_sharing_commitment) =
                Self::define_session_keys(&signer, validators);

            let sender_address = wallets.next().to_address();

            let ethereum = if let Some(router_address) = router_address {
                log::info!("📗 Connecting to existing router at {}", router_address);
                Ethereum::new(
                    &rpc_url,
                    router_address.parse().unwrap(),
                    signer.clone(),
                    sender_address,
                )
                .await?
            } else {
                log::info!("📗 Deploying new router");
                Ethereum::deploy(
                    &rpc_url,
                    validators
                        .iter()
                        .map(|k| k.public_key.to_address())
                        .collect(),
                    signer.clone(),
                    sender_address,
                    verifiable_secret_sharing_commitment,
                )
                .await?
            };

            let router = ethereum.router();
            let router_query = router.query();
            let router_address = router.address();

            let blob_reader = MockBlobReader::new();

            let db = Database::memory();

            let eth_cfg = EthereumConfig {
                rpc: rpc_url.clone(),
                beacon_rpc: Default::default(),
                router_address,
                block_time: config.block_time,
            };
            let mut observer =
                ObserverService::new(&eth_cfg, u32::MAX, db.clone(), blob_reader.clone_boxed())
                    .await
                    .unwrap();

            let provider = observer.provider().clone();

            let (broadcaster, _events_stream) = {
                let (sender, mut receiver) = broadcast::channel(2048);
                let cloned_sender = sender.clone();

                let (send_subscription_created, receive_subscription_created) =
                    tokio::sync::oneshot::channel::<()>();
                let handle = task::spawn(
                    async move {
                        send_subscription_created.send(()).unwrap();

                        while let Ok(event) = observer.select_next_some().await {
                            log::trace!(target: "test-event", "📗 Event: {:?}", event);

                            cloned_sender
                                .send(event)
                                .inspect_err(|err| log::error!("Failed to broadcast event: {err}"))
                                .unwrap();

                            // At least one receiver is presented always, in order to avoid the channel dropping.
                            receiver
                                .recv()
                                .await
                                .inspect_err(|err| log::error!("Failed to receive event: {err}"))
                                .unwrap();
                        }

                        panic!("📗 Observer stream ended");
                    }
                    .instrument(tracing::trace_span!("observer-stream")),
                );
                receive_subscription_created.await.unwrap();

                (sender, handle)
            };

            let threshold = router_query.threshold().await?;

            let network_address = match network {
                EnvNetworkConfig::Disabled => None,
                EnvNetworkConfig::Enabled => Some(None),
                EnvNetworkConfig::EnabledWithCustomAddress(address) => Some(Some(address)),
            };

            let bootstrap_network = network_address.map(|maybe_address| {
                static NONCE: AtomicUsize = AtomicUsize::new(1);

                // mul MAX_NETWORK_SERVICES_PER_TEST to avoid address collision between different test-threads
                let nonce = NONCE.fetch_add(1, Ordering::SeqCst) * MAX_NETWORK_SERVICES_PER_TEST;
                let address = maybe_address.unwrap_or_else(|| format!("/memory/{nonce}"));

                let config_path = tempfile::tempdir().unwrap().into_path();
                let multiaddr: Multiaddr = address.parse().unwrap();

                let mut config = NetworkConfig::new_test(config_path);
                config.listen_addresses = [multiaddr.clone()].into();
                config.external_addresses = [multiaddr.clone()].into();
                let mut service = NetworkService::new(config, &signer, db.clone()).unwrap();

                let local_peer_id = service.local_peer_id();

                let handle = task::spawn(
                    async move {
                        loop {
                            let _event = service.select_next_some().await;
                        }
                    }
                    .instrument(tracing::trace_span!("network-stream")),
                );

                let bootstrap_address = format!("{address}/p2p/{local_peer_id}");

                (handle, bootstrap_address, nonce)
            });

            // By default, anvil set system time as block time. For testing purposes we need to have constant increment.
            if anvil.is_some() && !continuous_block_generation {
                provider
                    .anvil_set_block_timestamp_interval(block_time.as_secs())
                    .await
                    .unwrap();
            }

            Ok(TestEnv {
                eth_cfg,
                wallets,
                blob_reader,
                provider,
                ethereum,
                signer,
                validators,
                sender_id: ActorId::from(H160::from(sender_address.0)),
                threshold,
                block_time,
                continuous_block_generation,
                broadcaster,
                db,
                bootstrap_network,
                _anvil: anvil,
                _events_stream,
            })
        }

        pub fn new_node(&mut self, config: NodeConfig) -> Node {
            let NodeConfig {
                name,
                db,
                validator_config,
                rpc: service_rpc_config,
                fast_sync,
            } = config;

            let db = db.unwrap_or_else(Database::memory);

            let (network_address, network_bootstrap_address) = self
                .bootstrap_network
                .as_mut()
                .map(|(_, bootstrap_address, nonce)| {
                    *nonce += 1;

                    if *nonce % MAX_NETWORK_SERVICES_PER_TEST == 0 {
                        panic!("Too many network services created by one test env: max is {MAX_NETWORK_SERVICES_PER_TEST}");
                    }

                    (format!("/memory/{nonce}"), bootstrap_address.clone())
                })
                .unzip();

            Node {
                name,
                db,
                multiaddr: None,
                latest_fast_synced_block: None,
                eth_cfg: self.eth_cfg.clone(),
                receiver: None,
                blob_reader: self.blob_reader.clone(),
                signer: self.signer.clone(),
                threshold: self.threshold,
                block_time: self.block_time,
                running_service_handle: None,
                validator_config,
                network_address,
                network_bootstrap_address,
                service_rpc_config,
                fast_sync,
            }
        }

        pub async fn upload_code(&self, code: &[u8]) -> Result<WaitForUploadCode> {
            log::info!("📗 Upload code, len {}", code.len());

            let listener = self.observer_events_publisher().subscribe().await;

            // Lock the blob reader to lock any other threads that may use it
            let mut guard = self.blob_reader.storage_mut();

            let pending_builder = block_on(
                self.ethereum
                    .router()
                    .request_code_validation_with_sidecar(code),
            )?;

            let code_id = pending_builder.code_id();
            let tx_hash = pending_builder.tx_hash();

            guard.insert(tx_hash, code.to_vec());

            Ok(WaitForUploadCode { listener, code_id })
        }

        pub async fn create_program(
            &self,
            code_id: CodeId,
            initial_executable_balance: u128,
        ) -> Result<WaitForProgramCreation> {
            log::info!("📗 Create program, code_id {code_id}");

            let listener = self.observer_events_publisher().subscribe().await;

            let router = self.ethereum.router();

            let (_, program_id) = router.create_program(code_id, H256::random()).await?;

            if initial_executable_balance != 0 {
                let program_address = program_id.to_address_lossy().0.into();
                router
                    .wvara()
                    .approve(program_address, initial_executable_balance)
                    .await?;

                let mirror = self.ethereum.mirror(program_address.into_array().into());

                mirror
                    .executable_balance_top_up(initial_executable_balance)
                    .await?;
            }

            Ok(WaitForProgramCreation {
                listener,
                program_id,
            })
        }

        pub async fn send_message(
            &self,
            target: ActorId,
            payload: &[u8],
            value: u128,
        ) -> Result<WaitForReplyTo> {
            log::info!("📗 Send message to {target}, payload len {}", payload.len());

            let listener = self.observer_events_publisher().subscribe().await;

            let program_address = Address::try_from(target)?;
            let program = self.ethereum.mirror(program_address);

            let (_, message_id) = program.send_message(payload, value).await?;

            Ok(WaitForReplyTo {
                listener,
                message_id,
            })
        }

        pub async fn approve_wvara(&self, program_id: ActorId) {
            log::info!("📗 Approving WVara for {program_id}");

            let program_address = Address::try_from(program_id).unwrap();
            let wvara = self.ethereum.router().wvara();
            wvara.approve_all(program_address.0.into()).await.unwrap();
        }

        pub async fn transfer_wvara(&self, program_id: ActorId, value: u128) {
            log::info!("📗 Transferring {value} WVara to {program_id}");

            let program_address = Address::try_from(program_id).unwrap();
            let wvara = self.ethereum.router().wvara();
            wvara
                .transfer(program_address.0.into(), value)
                .await
                .unwrap();
        }

        pub fn observer_events_publisher(&self) -> ObserverEventsPublisher {
            ObserverEventsPublisher {
                broadcaster: self.broadcaster.clone(),
                db: self.db.clone(),
            }
        }

        /// Force new block generation on rpc node.
        /// The difference between this method and `skip_blocks` is that
        /// `skip_blocks` will wait for the block event to be generated,
        /// while this method does not guarantee that.
        pub async fn force_new_block(&self) {
            if self.continuous_block_generation {
                // nothing to do: new block will be generated automatically
            } else {
                self.provider.evm_mine(None).await.unwrap();
            }
        }

        /// Force new `blocks_amount` blocks generation on rpc node,
        /// and wait for the block event to be generated.
        pub async fn skip_blocks(&self, blocks_amount: u32) {
            if self.continuous_block_generation {
                let mut blocks_count = 0;
                self.observer_events_publisher()
                    .subscribe()
                    .await
                    .apply_until_block_event(|_| {
                        blocks_count += 1;
                        Ok((blocks_count >= blocks_amount).then_some(()))
                    })
                    .await
                    .unwrap();
            } else {
                self.provider
                    .evm_mine(Some(MineOptions::Options {
                        timestamp: None,
                        blocks: Some(blocks_amount.into()),
                    }))
                    .await
                    .unwrap();
            }
        }

        /// Returns the index in validators list of the next block producer.
        ///
        /// ## Note
        /// This function is not completely thread-safe.
        /// If you have some other threads or processes,
        /// that can produce blocks for the same rpc node,
        /// then the return may be outdated.
        pub async fn next_block_producer_index(&self) -> usize {
            let timestamp = self.latest_block().await.timestamp;
            ethexe_consensus::block_producer_index(
                self.validators.len(),
                (timestamp + self.block_time.as_secs()) / self.block_time.as_secs(),
            )
        }

        pub async fn latest_block(&self) -> RpcHeader {
            self.provider
                .get_block(BlockId::latest())
                .await
                .unwrap()
                .expect("latest block always exist")
                .header
        }

        pub fn define_session_keys(
            signer: &Signer,
            validators: Vec<PublicKey>,
        ) -> (Vec<ValidatorConfig>, VerifiableSecretSharingCommitment) {
            let max_signers: u16 = validators.len().try_into().expect("conversion failed");
            let min_signers = max_signers
                .checked_mul(2)
                .expect("multiplication failed")
                .div_ceil(3);

            let maybe_validator_identifiers: Result<Vec<_>, _> = validators
                .iter()
                .map(|public_key| {
                    Identifier::deserialize(&ActorId::from(public_key.to_address()).into_bytes())
                })
                .collect();
            let validator_identifiers = maybe_validator_identifiers.expect("conversion failed");
            let identifiers = IdentifierList::Custom(&validator_identifiers);

            let mut rng = StdRng::seed_from_u64(123);

            let secret = SigningKey::deserialize(&[0x01; 32]).expect("conversion failed");

            let (secret_shares, public_key_package1) =
                keys::split(&secret, max_signers, min_signers, identifiers, &mut rng)
                    .expect("key split failed");

            let verifiable_secret_sharing_commitment = secret_shares
                .values()
                .map(|secret_share| secret_share.commitment().clone())
                .next()
                .expect("conversion failed");

            let identifiers = validator_identifiers.clone().into_iter().collect();
            let public_key_package2 = PublicKeyPackage::from_commitment(
                &identifiers,
                &verifiable_secret_sharing_commitment,
            )
            .expect("conversion failed");
            assert_eq!(public_key_package1, public_key_package2);

            (
                validators
                    .into_iter()
                    .zip(validator_identifiers.iter())
                    .map(|(public_key, id)| {
                        let signing_share = *secret_shares[id].signing_share();
                        let private_key = PrivateKey::from(
                            <[u8; 32]>::try_from(signing_share.serialize()).unwrap(),
                        );
                        ValidatorConfig {
                            public_key,
                            session_public_key: signer.storage_mut().add_key(private_key).unwrap(),
                        }
                    })
                    .collect(),
                verifiable_secret_sharing_commitment,
            )
        }
    }

    pub struct ObserverEventsPublisher {
        broadcaster: Sender<ObserverEvent>,
        db: Database,
    }

    impl ObserverEventsPublisher {
        pub async fn subscribe(&self) -> ObserverEventsListener {
            ObserverEventsListener {
                receiver: self.broadcaster.subscribe(),
                db: self.db.clone(),
            }
        }
    }

    pub struct ObserverEventsListener {
        receiver: broadcast::Receiver<ObserverEvent>,
        db: Database,
    }

    impl Clone for ObserverEventsListener {
        fn clone(&self) -> Self {
            Self {
                receiver: self.receiver.resubscribe(),
                db: self.db.clone(),
            }
        }
    }

    impl ObserverEventsListener {
        pub async fn next_event(&mut self) -> Result<ObserverEvent> {
            self.receiver.recv().await.map_err(Into::into)
        }

        pub async fn apply_until<R: Sized>(
            &mut self,
            mut f: impl FnMut(ObserverEvent) -> Result<Option<R>>,
        ) -> Result<R> {
            loop {
                let event = self.next_event().await?;
                if let Some(res) = f(event)? {
                    return Ok(res);
                }
            }
        }

        pub async fn apply_until_block_event<R: Sized>(
            &mut self,
            mut f: impl FnMut(BlockEvent) -> Result<Option<R>>,
        ) -> Result<R> {
            self.apply_until_block_event_with_header(|e, _h| f(e)).await
        }

        // NOTE: skipped by observer blocks are not iterated (possible on reorgs).
        // If your test depends on events in skipped blocks, you need to improve this method.
        // TODO #4554: iterate thru skipped blocks.
        pub async fn apply_until_block_event_with_header<R: Sized>(
            &mut self,
            mut f: impl FnMut(BlockEvent, &SimpleBlockData) -> Result<Option<R>>,
        ) -> Result<R> {
            loop {
                let event = self.next_event().await?;

                let ObserverEvent::BlockSynced(data) = event else {
                    continue;
                };

                let header = OnChainStorage::block_header(&self.db, data.block_hash)
                    .expect("Block header not found");
                let events = OnChainStorage::block_events(&self.db, data.block_hash)
                    .expect("Block events not found");

                let block_data = SimpleBlockData {
                    hash: data.block_hash,
                    header,
                };

                for event in events {
                    if let Some(res) = f(event, &block_data)? {
                        return Ok(res);
                    }
                }
            }
        }
    }

    pub enum ValidatorsConfig {
        /// Take validator addresses from provided wallet, amount of validators is provided.
        PreDefined(usize),
        /// Custom validator eth-addresses in hex string format.
        #[allow(unused)]
        Custom(Vec<String>),
    }

    /// Configuration for the network service.
    pub enum EnvNetworkConfig {
        /// Network service is disabled.
        Disabled,
        /// Network service is enabled. Network address will be generated.
        Enabled,
        #[allow(unused)]
        /// Network service is enabled. Network address is provided as String.
        EnabledWithCustomAddress(String),
    }

    pub enum EnvRpcConfig {
        #[allow(unused)]
        ProvidedURL(String),
        CustomAnvil {
            slots_in_epoch: Option<u64>,
            genesis_timestamp: Option<u64>,
        },
    }

    pub struct TestEnvConfig {
        /// How many validators will be in deployed router.
        /// By default uses 1 auto generated validator.
        pub validators: ValidatorsConfig,
        /// By default uses 1 second block time.
        pub block_time: Duration,
        /// By default creates new anvil instance if rpc is not provided.
        pub rpc: EnvRpcConfig,
        /// By default uses anvil hardcoded wallets if custom wallets are not provided.
        pub wallets: Option<Vec<String>>,
        /// If None (by default) new router will be deployed.
        /// In case of Some(_), will connect to existing router contract.
        pub router_address: Option<String>,
        /// Identify whether networks works (or have to works) in continuous block generation mode, false by default.
        pub continuous_block_generation: bool,
        /// Network service configuration, disabled by default.
        pub network: EnvNetworkConfig,
    }

    impl Default for TestEnvConfig {
        fn default() -> Self {
            Self {
                validators: ValidatorsConfig::PreDefined(1),
                block_time: Duration::from_secs(1),
                rpc: EnvRpcConfig::CustomAnvil {
                    // speeds up block finalization, so we don't have to calculate
                    // when the next finalized block is produced, which is convenient for tests
                    slots_in_epoch: Some(1),
                    // For deterministic tests we need to set fixed genesis timestamp
                    genesis_timestamp: Some(1_000_000_000),
                },
                wallets: None,
                router_address: None,
                continuous_block_generation: false,
                network: EnvNetworkConfig::Disabled,
            }
        }
    }

    // TODO (breathx): consider to remove me in favor of crate::config::NodeConfig.
    #[derive(Default)]
    pub struct NodeConfig {
        /// Node name.
        pub name: Option<String>,
        /// Database, if not provided, will be created with MemDb.
        pub db: Option<Database>,
        /// Validator configuration, if provided then new node starts as validator.
        pub validator_config: Option<ValidatorConfig>,
        /// RPC configuration, if provided then new node starts with RPC service.
        pub rpc: Option<RpcConfig>,
        /// Do P2P database synchronization before the main loop
        pub fast_sync: bool,
    }

    impl NodeConfig {
        pub fn named(name: impl Into<String>) -> Self {
            Self {
                name: Some(name.into()),
                ..Default::default()
            }
        }

        #[allow(unused)]
        pub fn db(mut self, db: Database) -> Self {
            self.db = Some(db);
            self
        }

        pub fn validator(mut self, config: ValidatorConfig) -> Self {
            self.validator_config = Some(config);
            self
        }

        pub fn service_rpc(mut self, rpc_port: u16) -> Self {
            let service_rpc_config = RpcConfig {
                listen_addr: SocketAddr::new("127.0.0.1".parse().unwrap(), rpc_port),
                cors: None,
                dev: false,
            };
            self.rpc = Some(service_rpc_config);

            self
        }

        pub fn fast_sync(mut self) -> Self {
            self.fast_sync = true;
            self
        }
    }

    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub struct ValidatorConfig {
        /// Validator public key.
        pub public_key: PublicKey,
        /// Validator session public key.
        pub session_public_key: PublicKey,
    }

    /// Provides access to hardcoded anvil wallets or custom set wallets.
    pub struct Wallets {
        wallets: Vec<PublicKey>,
        next_wallet: usize,
    }

    impl Wallets {
        pub fn anvil(signer: &Signer) -> Self {
            let accounts = vec![
                "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80",
                "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d",
                "0x5de4111afa1a4b94908f83103eb1f1706367c2e68ca870fc3fb9a804cdab365a",
                "0x7c852118294e51e653712a81e05800f419141751be58f605c371e15141b007a6",
                "0x47e179ec197488593b187f80a00eb0da91f1b9d0b13f8733639f19c30a34926a",
                "0x8b3a350cf5c34c9194ca85829a2df0ec3153be0318b5e2d3348e872092edffba",
                "0x92db14e403b83dfe3df233f83dfa3a0d7096f21ca9b0d6d6b8d88b2b4ec1564e",
                "0x4bbbf85ce3377467afe5d46f804f221813b2bb87f24d81f60f1fcdbf7cbf4356",
                "0xdbda1821b80551c9d65939329250298aa3472ba22feea921c0cf5d620ea67b97",
                "0x2a871d0798f97d79848a013d4936a73bf4cc922c825d33c1cf7073dff6d409c6",
            ];

            Self::custom(signer, accounts)
        }

        pub fn custom<S: AsRef<str>>(signer: &Signer, accounts: Vec<S>) -> Self {
            Self {
                wallets: accounts
                    .into_iter()
                    .map(|s| {
                        signer
                            .storage_mut()
                            .add_key(s.as_ref().parse().unwrap())
                            .unwrap()
                    })
                    .collect(),
                next_wallet: 0,
            }
        }

        pub fn next(&mut self) -> PublicKey {
            let pub_key = self.wallets.get(self.next_wallet).expect("No more wallets");
            self.next_wallet += 1;
            *pub_key
        }
    }

    pub struct Node {
        pub name: Option<String>,
        pub db: Database,
        pub multiaddr: Option<String>,
        pub latest_fast_synced_block: Option<H256>,

        eth_cfg: EthereumConfig,
        receiver: Option<Receiver<Event>>,
        blob_reader: MockBlobReader,
        signer: Signer,
        threshold: u64,
        block_time: Duration,
        running_service_handle: Option<JoinHandle<()>>,
        validator_config: Option<ValidatorConfig>,
        network_address: Option<String>,
        network_bootstrap_address: Option<String>,
        service_rpc_config: Option<RpcConfig>,
        fast_sync: bool,
    }

    impl Node {
        pub async fn start_service(&mut self) {
            assert!(
                self.running_service_handle.is_none(),
                "Service is already running"
            );

            let processor = Processor::new(self.db.clone()).unwrap();

            let wait_for_network = self.network_bootstrap_address.is_some();

            let network = self.network_address.as_ref().map(|addr| {
                let config_path = tempfile::tempdir().unwrap().into_path();
                let multiaddr: Multiaddr = addr.parse().unwrap();

                let mut config = NetworkConfig::new_test(config_path);
                config.listen_addresses = [multiaddr.clone()].into();
                config.external_addresses = [multiaddr.clone()].into();
                if let Some(bootstrap_addr) = self.network_bootstrap_address.as_ref() {
                    let multiaddr = bootstrap_addr.parse().unwrap();
                    config.bootstrap_addresses = [multiaddr].into();
                }
                let network = NetworkService::new(config, &self.signer, self.db.clone()).unwrap();
                self.multiaddr = Some(format!("{addr}/p2p/{}", network.local_peer_id()));
                network
            });

            let consensus: Pin<Box<dyn ConsensusService>> =
                if let Some(config) = self.validator_config.as_ref() {
                    Box::pin(
                        ValidatorService::new(
                            self.signer.clone(),
                            self.db.clone(),
                            ethexe_consensus::ValidatorConfig {
                                ethereum_rpc: self.eth_cfg.rpc.clone(),
                                pub_key: config.public_key,
                                router_address: self.eth_cfg.router_address,
                                signatures_threshold: self.threshold,
                                slot_duration: self.block_time,
                            },
                        )
                        .await
                        .unwrap(),
                    )
                } else {
                    Box::pin(SimpleConnectService::new())
                };

            let (sender, receiver) = broadcast::channel(2048);

            let observer = ObserverService::new(
                &self.eth_cfg,
                u32::MAX,
                self.db.clone(),
                self.blob_reader.clone_boxed(),
            )
            .await
            .unwrap();

            let tx_pool_service = TxPoolService::new(self.db.clone());

            let rpc = self.service_rpc_config.as_ref().map(|service_rpc_config| {
                RpcService::new(service_rpc_config.clone(), self.db.clone(), None)
            });

            self.receiver = Some(receiver);

            let service = Service::new_from_parts(
                self.db.clone(),
                observer,
                processor,
                self.signer.clone(),
                tx_pool_service,
                consensus,
                network,
                None,
                rpc,
                Some(sender),
                self.fast_sync,
            );

            let name = self.name.clone();
            let handle = task::spawn(async move {
                service
                    .run()
                    .instrument(tracing::info_span!("node", name))
                    .await
                    .unwrap()
            });
            self.running_service_handle = Some(handle);

            if self.fast_sync {
                self.latest_fast_synced_block = self
                    .listener()
                    .apply_until(|e| {
                        if let Event::FastSyncDone(block) = e {
                            Ok(Some(block))
                        } else {
                            Ok(None)
                        }
                    })
                    .await
                    .map(Some)
                    .unwrap();
            }

            self.wait_for(|e| matches!(e, Event::ServiceStarted)).await;

            // fast sync implies network has connections
            if wait_for_network && !self.fast_sync {
                self.wait_for(|e| matches!(e, Event::Network(NetworkEvent::PeerConnected(_))))
                    .await;
            }
        }

        pub async fn stop_service(&mut self) {
            let handle = self
                .running_service_handle
                .take()
                .expect("Service is not running");
            handle.abort();

            assert!(handle.await.unwrap_err().is_cancelled());

            self.multiaddr = None;
            self.receiver = None;
        }

        pub fn rpc_client(&self) -> Option<RpcClient> {
            self.service_rpc_config
                .as_ref()
                .map(|rpc| RpcClient::new(format!("http://{}", rpc.listen_addr)))
        }

        pub fn listener(&mut self) -> ServiceEventsListener {
            ServiceEventsListener {
                receiver: self.receiver.as_mut().expect("channel isn't created"),
            }
        }

        // TODO(playX18): Tests that actually use Event broadcast channel extensively
        pub async fn wait_for(&mut self, f: impl Fn(Event) -> bool) {
            self.listener()
                .wait_for(|e| Ok(f(e)))
                .await
                .expect("infallible; always ok")
        }
    }

    impl Drop for Node {
        fn drop(&mut self) {
            if let Some(handle) = &self.running_service_handle {
                handle.abort();
            }
        }
    }

    pub struct ServiceEventsListener<'a> {
        receiver: &'a mut Receiver<Event>,
    }

    impl ServiceEventsListener<'_> {
        pub async fn next_event(&mut self) -> Result<Event> {
            self.receiver.recv().await.map_err(Into::into)
        }

        pub async fn wait_for(&mut self, f: impl Fn(Event) -> Result<bool>) -> Result<()> {
            self.apply_until(|e| if f(e)? { Ok(Some(())) } else { Ok(None) })
                .await
        }

        pub async fn wait_for_block_processed(&mut self, block_hash: H256) {
            self.wait_for(|event| {
                Ok(matches!(
                    event,
                    Event::Compute(ComputeEvent::BlockProcessed(BlockProcessed { block_hash: b })) if b == block_hash
                ))
            }).await.unwrap();
        }

        pub async fn apply_until<R: Sized>(
            &mut self,
            f: impl Fn(Event) -> Result<Option<R>>,
        ) -> Result<R> {
            loop {
                let event = self.next_event().await?;
                if let Some(res) = f(event)? {
                    return Ok(res);
                }
            }
        }
    }

    #[derive(Clone)]
    pub struct WaitForUploadCode {
        listener: ObserverEventsListener,
        pub code_id: CodeId,
    }

    #[derive(Debug)]
    pub struct UploadCodeInfo {
        pub code_id: CodeId,
        pub code: Vec<u8>,
        pub valid: bool,
    }

    impl WaitForUploadCode {
        pub async fn wait_for(mut self) -> Result<UploadCodeInfo> {
            log::info!("📗 Waiting for code upload, code_id {}", self.code_id);

            let mut code_info = None;
            let mut valid_info = None;

            self.listener
                .apply_until(|event| match event {
                    ObserverEvent::Blob(blob) if blob.code_id == self.code_id => {
                        code_info = Some(blob.code);
                        Ok(Some(()))
                    }
                    _ => Ok(None),
                })
                .await?;

            self.listener
                .apply_until_block_event(|event| match event {
                    BlockEvent::Router(RouterEvent::CodeGotValidated { code_id, valid })
                        if code_id == self.code_id =>
                    {
                        valid_info = Some(valid);
                        Ok(Some(()))
                    }
                    _ => Ok(None),
                })
                .await?;

            Ok(UploadCodeInfo {
                code_id: self.code_id,
                code: code_info.expect("Code must be set"),
                valid: valid_info.expect("Valid must be set"),
            })
        }
    }

    #[derive(Clone)]
    pub struct WaitForProgramCreation {
        listener: ObserverEventsListener,
        pub program_id: ActorId,
    }

    #[derive(Debug)]
    pub struct ProgramCreationInfo {
        pub program_id: ActorId,
        pub code_id: CodeId,
    }

    impl WaitForProgramCreation {
        pub async fn wait_for(mut self) -> Result<ProgramCreationInfo> {
            log::info!("📗 Waiting for program {} creation", self.program_id);

            let mut code_id_info = None;
            self.listener
                .apply_until_block_event(|event| {
                    match event {
                        BlockEvent::Router(RouterEvent::ProgramCreated { actor_id, code_id })
                            if actor_id == self.program_id =>
                        {
                            code_id_info = Some(code_id);
                            return Ok(Some(()));
                        }

                        _ => {}
                    }
                    Ok(None)
                })
                .await?;

            let code_id = code_id_info.expect("Code ID must be set");
            Ok(ProgramCreationInfo {
                program_id: self.program_id,
                code_id,
            })
        }
    }

    #[derive(Clone)]
    pub struct WaitForReplyTo {
        listener: ObserverEventsListener,
        pub message_id: MessageId,
    }

    #[derive(Debug)]
    pub struct ReplyInfo {
        pub message_id: MessageId,
        pub program_id: ActorId,
        pub payload: Vec<u8>,
        pub code: ReplyCode,
        pub value: u128,
    }

    impl WaitForReplyTo {
        pub async fn wait_for(mut self) -> Result<ReplyInfo> {
            log::info!("📗 Waiting for reply to message {}", self.message_id);

            let mut info = None;

            self.listener
                .apply_until_block_event(|event| match event {
                    BlockEvent::Mirror {
                        actor_id,
                        event:
                            MirrorEvent::Reply {
                                reply_to,
                                payload,
                                reply_code,
                                value,
                            },
                    } if reply_to == self.message_id => {
                        info = Some(ReplyInfo {
                            message_id: reply_to,
                            program_id: actor_id,
                            payload,
                            code: reply_code,
                            value,
                        });
                        Ok(Some(()))
                    }
                    _ => Ok(None),
                })
                .await?;

            Ok(info.expect("Reply info must be set"))
        }
    }
}
