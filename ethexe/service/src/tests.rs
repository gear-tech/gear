// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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
    Service,
};
use alloy::{
    node_bindings::{Anvil, AnvilInstance},
    providers::{ext::AnvilApi, Provider},
    rpc::types::anvil::MineOptions,
};
use anyhow::Result;
use ethexe_common::{
    db::CodesStorage,
    events::{BlockEvent, MirrorEvent, RouterEvent},
};
use ethexe_db::{BlockMetaStorage, Database, MemDb, ScheduledTask};
use ethexe_ethereum::{router::RouterQuery, Ethereum};
use ethexe_observer::{EthereumConfig, Event, MockBlobReader, Observer, Query};
use ethexe_processor::Processor;
use ethexe_prometheus::PrometheusConfig;
use ethexe_rpc::RpcConfig;
use ethexe_runtime_common::state::{Storage, ValueWithExpiry};
use ethexe_sequencer::Sequencer;
use ethexe_signer::Signer;
use ethexe_validator::Validator;
use gear_core::{
    ids::prelude::*,
    message::{ReplyCode, SuccessReplyReason},
};
use gear_core_errors::{ErrorReplyReason, SimpleExecutionError};
use gprimitives::{ActorId, CodeId, MessageId, H160, H256};
use parity_scale_codec::Encode;
use std::{
    collections::{BTreeMap, BTreeSet},
    net::{Ipv4Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};
use tempfile::tempdir;
use tokio::{
    sync::oneshot,
    task::{self, JoinHandle},
};
use utils::{NodeConfig, TestEnv, TestEnvConfig, ValidatorsConfig};

#[tokio::test]
async fn basics() {
    gear_utils::init_default_logger();

    let tmp_dir = tempdir().unwrap();
    let tmp_dir = tmp_dir.path().to_path_buf();

    let node_cfg = config::NodeConfig {
        database_path: tmp_dir.join("db"),
        key_path: tmp_dir.join("key"),
        sequencer: Default::default(),
        validator: Default::default(),
        max_commitment_depth: 1_000,
        worker_threads_override: None,
        virtual_threads: 16,
    };

    let eth_cfg = EthereumConfig {
        rpc: "wss://reth-rpc.gear-tech.io".into(),
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
    config.network = Some(ethexe_network::NetworkServiceConfig::new_local(
        tmp_dir.join("net"),
    ));

    config.rpc = Some(RpcConfig {
        listen_addr: SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 9944),
        cors: None,
    });

    config.prometheus = Some(PrometheusConfig::new_with_default_registry(
        "DevNode".into(),
        SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 9635),
    ));

    Service::new(&config).await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
#[ntest::timeout(60_000)]
async fn ping() {
    gear_utils::init_default_logger();

    let mut env = TestEnv::new(Default::default()).await.unwrap();

    let sequencer_public_key = env.wallets.next();
    let mut node = env
        .new_node(
            NodeConfig::default()
                .sequencer(sequencer_public_key)
                .validator(env.validators[0]),
        )
        .await;
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
        .create_program(code_id, b"PING", 0)
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();
    assert_eq!(res.code_id, code_id);
    assert_eq!(res.init_message_source, env.sender_id);
    assert_eq!(res.init_message_payload, b"PING");
    assert_eq!(res.init_message_value, 0);
    assert_eq!(res.reply_payload, b"PONG");
    assert_eq!(res.reply_value, 0);
    assert_eq!(
        res.reply_code,
        ReplyCode::Success(SuccessReplyReason::Manual)
    );

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
    assert_eq!(
        res.reply_code,
        ReplyCode::Success(SuccessReplyReason::Manual)
    );
    assert_eq!(res.reply_payload, b"PONG");
    assert_eq!(res.reply_value, 0);

    let res = env
        .send_message(ping_id, b"PUNK", 0)
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();
    assert_eq!(res.program_id, ping_id);
    assert_eq!(res.reply_code, ReplyCode::Success(SuccessReplyReason::Auto));
    assert_eq!(res.reply_payload, b"");
    assert_eq!(res.reply_value, 0);
}

#[tokio::test(flavor = "multi_thread")]
#[ntest::timeout(60_000)]
async fn uninitialized_program() {
    gear_utils::init_default_logger();

    let mut env = TestEnv::new(Default::default()).await.unwrap();

    let sequencer_public_key = env.wallets.next();
    let mut node = env
        .new_node(
            NodeConfig::default()
                .sequencer(sequencer_public_key)
                .validator(env.validators[0]),
        )
        .await;
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
            .create_program(code_id, &[], 0)
            .await
            .unwrap()
            .wait_for()
            .await
            .unwrap();

        let expected_err = ReplyCode::Error(SimpleExecutionError::UserspacePanic.into());
        assert_eq!(res.reply_code, expected_err);

        let res = env
            .send_message(res.program_id, &[], 0)
            .await
            .unwrap()
            .wait_for()
            .await
            .unwrap();

        let expected_err = ReplyCode::Error(ErrorReplyReason::InactiveActor);
        assert_eq!(res.reply_code, expected_err);
    }

    // Case #2: async init, replies are acceptable.
    {
        let init_payload = demo_async_init::InputArgs {
            approver_first: env.sender_id,
            approver_second: env.sender_id,
            approver_third: env.sender_id,
        }
        .encode();

        let mut listener = env.events_publisher().subscribe().await;

        let init_res = env.create_program(code_id, &init_payload, 0).await.unwrap();

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
        let expected_err = ReplyCode::Error(ErrorReplyReason::InactiveActor);
        assert_eq!(res.reply_code, expected_err);
        // Checking further initialisation.

        // Required replies.
        for mid in msgs_for_reply {
            mirror.send_reply(mid, [], 0).await.unwrap();
        }

        // Success end of initialisation.
        let reply_code = listener
            .apply_until_block_event(|event| match event {
                BlockEvent::Mirror {
                    actor_id,
                    event:
                        MirrorEvent::Reply {
                            reply_code,
                            reply_to,
                            ..
                        },
                } if actor_id == init_res.program_id && reply_to == init_res.message_id => {
                    Ok(Some(reply_code))
                }
                _ => Ok(None),
            })
            .await
            .unwrap();

        assert!(reply_code.is_success());

        // Handle message handled, but panicked due to incorrect payload as expected.
        let res = env
            .send_message(res.program_id, &[], 0)
            .await
            .unwrap()
            .wait_for()
            .await
            .unwrap();

        let expected_err = ReplyCode::Error(SimpleExecutionError::UserspacePanic.into());
        assert_eq!(res.reply_code, expected_err);
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ntest::timeout(60_000)]
async fn mailbox() {
    gear_utils::init_default_logger();

    let mut env = TestEnv::new(Default::default()).await.unwrap();

    let sequencer_public_key = env.wallets.next();
    let mut node = env
        .new_node(
            NodeConfig::default()
                .sequencer(sequencer_public_key)
                .validator(env.validators[0]),
        )
        .await;
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
        .create_program(code_id, &env.sender_id.encode(), 0)
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();

    assert_eq!(res.reply_code, ReplyCode::Success(SuccessReplyReason::Auto));

    let pid = res.program_id;

    env.approve_wvara(pid).await;

    let res = env
        .send_message(pid, &demo_async::Command::Mutex.encode(), 0)
        .await
        .unwrap();

    let original_mid = res.message_id;
    let mid_expected_message = MessageId::generate_outgoing(original_mid, 0);
    let ping_expected_message = MessageId::generate_outgoing(original_mid, 1);

    let mut listener = env.events_publisher().subscribe().await;
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
        .block_end_schedule(block_data.header.parent_hash)
        .expect("must exist");

    assert_eq!(schedule, expected_schedule);

    let expected_mailbox = BTreeMap::from_iter([(
        env.sender_id,
        BTreeMap::from_iter([
            (mid_expected_message, ValueWithExpiry { value: 0, expiry }),
            (ping_expected_message, ValueWithExpiry { value: 0, expiry }),
        ]),
    )]);

    let mirror = env.ethereum.mirror(pid.try_into().unwrap());
    let state_hash = mirror.query().state_hash().await.unwrap();

    let state = node.db.read_state(state_hash).unwrap();
    assert!(!state.mailbox_hash.is_empty());
    let mailbox = state
        .mailbox_hash
        .with_hash_or_default(|hash| node.db.read_mailbox(hash).unwrap());

    assert_eq!(mailbox.into_inner(), expected_mailbox);

    mirror
        .send_reply(ping_expected_message, "PONG", 0)
        .await
        .unwrap();

    let initial_message = res.message_id;
    let reply_info = res.wait_for().await.unwrap();
    assert_eq!(
        reply_info.reply_code,
        ReplyCode::Success(SuccessReplyReason::Manual)
    );
    assert_eq!(reply_info.reply_payload, initial_message.encode());

    let state_hash = mirror.query().state_hash().await.unwrap();

    let state = node.db.read_state(state_hash).unwrap();
    assert!(!state.mailbox_hash.is_empty());
    let mailbox = state
        .mailbox_hash
        .with_hash_or_default(|hash| node.db.read_mailbox(hash).unwrap());

    let expected_mailbox = BTreeMap::from_iter([(
        env.sender_id,
        BTreeMap::from_iter([(mid_expected_message, ValueWithExpiry { value: 0, expiry })]),
    )]);

    assert_eq!(mailbox.into_inner(), expected_mailbox);

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
        .block_end_schedule(block_data.header.parent_hash)
        .expect("must exist");
    assert!(schedule.is_empty(), "{:?}", schedule);
}

#[tokio::test(flavor = "multi_thread")]
#[ntest::timeout(60_000)]
async fn incoming_transfers() {
    gear_utils::init_default_logger();

    let mut env = TestEnv::new(Default::default()).await.unwrap();

    let sequencer_public_key = env.wallets.next();
    let mut node = env
        .new_node(
            NodeConfig::default()
                .sequencer(sequencer_public_key)
                .validator(env.validators[0]),
        )
        .await;
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
        .create_program(code_id, b"PING", 0)
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

    let mut listener = env.events_publisher().subscribe().await;

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

    assert_eq!(
        res.reply_code,
        ReplyCode::Success(SuccessReplyReason::Manual)
    );
    assert_eq!(res.reply_value, 0);

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
#[ntest::timeout(120_000)]
async fn ping_reorg() {
    gear_utils::init_default_logger();

    let mut env = TestEnv::new(Default::default()).await.unwrap();

    let sequencer_pub_key = env.wallets.next();
    let mut node = env
        .new_node(
            NodeConfig::default()
                .sequencer(sequencer_pub_key)
                .validator(env.validators[0]),
        )
        .await;
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

    let create_program = env.create_program(code_id, b"PING", 0).await.unwrap();

    // Mine some blocks to check missed blocks support
    env.skip_blocks(10).await;

    // Start new service
    node.start_service().await;

    // IMPORTANT: Mine one block to sent block event to the new service.
    env.force_new_block().await;

    let res = create_program.wait_for().await.unwrap();
    assert_eq!(res.code_id, code_id);
    assert_eq!(res.reply_payload, b"PONG");

    let ping_id = res.program_id;

    env.approve_wvara(ping_id).await;

    log::info!(
        "📗 Create snapshot for block: {}, where ping program is already created",
        env.observer.provider().get_block_number().await.unwrap()
    );
    let program_created_snapshot_id = env.observer.provider().anvil_snapshot().await.unwrap();

    let res = env
        .send_message(ping_id, b"PING", 0)
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();
    assert_eq!(res.program_id, ping_id);
    assert_eq!(res.reply_payload, b"PONG");

    log::info!("📗 Test after reverting to the program creation snapshot");
    env.observer
        .provider()
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
    assert_eq!(res.reply_payload, b"PONG");

    // The last step is to test correctness after db cleanup
    node.stop_service().await;
    node.db = Database::from_one(&MemDb::default(), env.router_address.0);

    log::info!("📗 Test after db cleanup and service shutting down");
    let send_message = env.send_message(ping_id, b"PING", 0).await.unwrap();

    // Skip some blocks to simulate long time without service
    env.skip_blocks(10).await;

    node.start_service().await;

    // Important: mine one block to sent block event to the new service.
    env.force_new_block().await;

    let res = send_message.wait_for().await.unwrap();
    assert_eq!(res.program_id, ping_id);
    assert_eq!(res.reply_payload, b"PONG");
}

// Mine 150 blocks - send message - mine 150 blocks.
// Deep sync must load chain in batch.
#[tokio::test(flavor = "multi_thread")]
#[ntest::timeout(60_000)]
async fn ping_deep_sync() {
    gear_utils::init_default_logger();

    let mut env = TestEnv::new(Default::default()).await.unwrap();

    let sequencer_pub_key = env.wallets.next();
    let mut node = env
        .new_node(
            NodeConfig::default()
                .sequencer(sequencer_pub_key)
                .validator(env.validators[0]),
        )
        .await;
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
        .create_program(code_id, b"PING", 0)
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();
    assert_eq!(res.code_id, code_id);
    assert_eq!(res.init_message_payload, b"PING");
    assert_eq!(res.init_message_value, 0);
    assert_eq!(res.reply_payload, b"PONG");
    assert_eq!(res.reply_value, 0);
    assert_eq!(
        res.reply_code,
        ReplyCode::Success(SuccessReplyReason::Manual)
    );

    let ping_id = res.program_id;

    // Mine some blocks to check deep sync.
    env.skip_blocks(150).await;

    env.approve_wvara(ping_id).await;

    let send_message = env.send_message(ping_id, b"PING", 0).await.unwrap();

    // Mine some blocks to check deep sync.
    env.skip_blocks(150).await;

    let res = send_message.wait_for().await.unwrap();
    assert_eq!(res.program_id, ping_id);
    assert_eq!(res.reply_payload, b"PONG");
    assert_eq!(res.reply_value, 0);
    assert_eq!(
        res.reply_code,
        ReplyCode::Success(SuccessReplyReason::Manual)
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ntest::timeout(120_000)]
async fn multiple_validators() {
    gear_utils::init_default_logger();

    let config = TestEnvConfig {
        validators: ValidatorsConfig::Generated(3),
        ..Default::default()
    };
    let mut env = TestEnv::new(config).await.unwrap();

    log::info!("📗 Starting sequencer");
    let sequencer_pub_key = env.wallets.next();
    let mut sequencer = env
        .new_node(
            NodeConfig::default()
                .sequencer(sequencer_pub_key)
                .network(None, None),
        )
        .await;
    sequencer.start_service().await;

    log::info!("📗 Starting validator 0");
    let mut validator0 = env
        .new_node(
            NodeConfig::default()
                .validator(env.validators[0])
                .network(None, sequencer.multiaddr.clone()),
        )
        .await;
    validator0.start_service().await;

    log::info!("📗 Starting validator 1");
    let mut validator1 = env
        .new_node(
            NodeConfig::default()
                .validator(env.validators[1])
                .network(None, sequencer.multiaddr.clone()),
        )
        .await;
    validator1.start_service().await;

    log::info!("📗 Starting validator 2");
    let mut validator2 = env
        .new_node(
            NodeConfig::default()
                .validator(env.validators[2])
                .network(None, sequencer.multiaddr.clone()),
        )
        .await;
    validator2.start_service().await;

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
        .create_program(ping_code_id, b"", 0)
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();
    assert_eq!(res.code_id, ping_code_id);
    assert_eq!(res.init_message_payload, b"");
    assert_eq!(res.init_message_value, 0);
    assert_eq!(res.reply_payload, b"");
    assert_eq!(res.reply_value, 0);
    assert_eq!(res.reply_code, ReplyCode::Success(SuccessReplyReason::Auto));

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
        .create_program(async_code_id, ping_id.encode().as_slice(), 0)
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();
    assert_eq!(res.code_id, async_code_id);
    assert_eq!(res.init_message_payload, ping_id.encode().as_slice());
    assert_eq!(res.init_message_value, 0);
    assert_eq!(res.reply_payload, b"");
    assert_eq!(res.reply_value, 0);
    assert_eq!(res.reply_code, ReplyCode::Success(SuccessReplyReason::Auto));

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
    assert_eq!(res.reply_payload, res.message_id.encode().as_slice());
    assert_eq!(res.reply_value, 0);
    assert_eq!(
        res.reply_code,
        ReplyCode::Success(SuccessReplyReason::Manual)
    );

    log::info!("📗 Stop validator 2 and check that all is still working");
    validator2.stop_service().await;
    let res = env
        .send_message(async_id, demo_async::Command::Common.encode().as_slice(), 0)
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();
    assert_eq!(res.reply_payload, res.message_id.encode().as_slice());

    log::info!("📗 Stop validator 1 and check that it's not working");
    validator1.stop_service().await;

    let wait_for_reply_to = env
        .send_message(async_id, demo_async::Command::Common.encode().as_slice(), 0)
        .await
        .unwrap();

    let _ = tokio::time::timeout(env.block_time * 5, wait_for_reply_to.clone().wait_for())
        .await
        .expect_err("Timeout expected");

    log::info!("📗 Start validator 2 and check that now is working, validator 1 is still stopped.");
    // TODO: impossible to restart validator 2 with the same network address, need to fix it #4210
    let mut validator2 = env
        .new_node(
            NodeConfig::default()
                .validator(env.validators[2])
                .network(None, sequencer.multiaddr.clone())
                .db(validator2.db),
        )
        .await;
    validator2.start_service().await;

    // IMPORTANT: mine one block to sent a new block event.
    env.force_new_block().await;

    let res = wait_for_reply_to.wait_for().await.unwrap();
    assert_eq!(res.reply_payload, res.message_id.encode().as_slice());
}

mod utils {
    use super::*;
    use ethexe_network::export::Multiaddr;
    use ethexe_observer::{ObserverService, SimpleBlockData};
    use futures::StreamExt;
    use gear_core::message::ReplyCode;
    use std::{
        ops::Mul,
        str::FromStr,
        sync::atomic::{AtomicUsize, Ordering},
    };
    use tokio::sync::{broadcast::Sender, Mutex};

    pub struct TestEnv {
        pub rpc_url: String,
        pub wallets: Wallets,
        pub observer: Observer,
        pub blob_reader: Arc<MockBlobReader>,
        pub ethereum: Ethereum,
        #[allow(unused)]
        pub router_query: RouterQuery,
        pub signer: Signer,
        pub validators: Vec<ethexe_signer::PublicKey>,
        pub router_address: ethexe_signer::Address,
        pub sender_id: ActorId,
        pub genesis_block_hash: H256,
        pub threshold: u64,
        pub block_time: Duration,
        pub continuous_block_generation: bool,

        /// In order to reduce amount of observers, we create only one observer and broadcast events to all subscribers.
        broadcaster: Arc<Mutex<Sender<Event>>>,
        _anvil: Option<AnvilInstance>,
        _events_stream: JoinHandle<()>,
    }

    impl TestEnv {
        pub async fn new(config: TestEnvConfig) -> Result<Self> {
            let TestEnvConfig {
                validators,
                block_time,
                rpc_url,
                wallets,
                router_address,
                continuous_block_generation,
            } = config;

            log::info!(
                "📗 Starting new test environment. Continuous block generation: {}",
                continuous_block_generation
            );

            let (rpc_url, anvil) = match rpc_url {
                Some(rpc_url) => {
                    log::info!("📍 Using provided RPC URL: {}", rpc_url);
                    (rpc_url, None)
                }
                None => {
                    let anvil = if continuous_block_generation {
                        Anvil::new().block_time(block_time.as_secs()).spawn()
                    } else {
                        Anvil::new().spawn()
                    };
                    log::info!("📍 Anvil started at {}", anvil.ws_endpoint());
                    (anvil.ws_endpoint(), Some(anvil))
                }
            };

            let signer = Signer::new(tempfile::tempdir()?.into_path())?;

            let mut wallets = if let Some(wallets) = wallets {
                Wallets::custom(&signer, wallets)
            } else {
                Wallets::anvil(&signer)
            };

            let validators: Vec<_> = match validators {
                ValidatorsConfig::Generated(amount) => (0..amount)
                    .map(|_| signer.generate_key().unwrap())
                    .collect(),
                ValidatorsConfig::Custom(keys) => keys
                    .iter()
                    .map(|k| {
                        let private_key = k.parse().unwrap();
                        signer.add_key(private_key).unwrap()
                    })
                    .collect(),
            };

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
                    validators.iter().map(|k| k.to_address()).collect(),
                    signer.clone(),
                    sender_address,
                )
                .await?
            };

            let router = ethereum.router();
            let router_query = router.query();
            let router_address = router.address();

            let blob_reader = Arc::new(MockBlobReader::new(block_time));

            let observer = Observer::new(&rpc_url, router_address, blob_reader.clone())
                .await
                .expect("failed to create observer");

            let (broadcaster, _events_stream) = {
                let mut observer = observer.clone();
                let (sender, mut receiver) = tokio::sync::broadcast::channel::<Event>(2048);
                let sender = Arc::new(Mutex::new(sender));
                let cloned_sender = sender.clone();

                let (send_subscription_created, receive_subscription_created) =
                    oneshot::channel::<()>();
                let handle = task::spawn(async move {
                    let observer_events = observer.events_all();
                    futures::pin_mut!(observer_events);

                    send_subscription_created.send(()).unwrap();

                    while let Some(event) = observer_events.next().await {
                        log::trace!(target: "test-event", "📗 Event: {:?}", event);

                        cloned_sender
                            .lock()
                            .await
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
                });
                receive_subscription_created.await.unwrap();

                (sender, handle)
            };

            let genesis_block_hash = router_query.genesis_block_hash().await?;
            let threshold = router_query.threshold().await?;

            Ok(TestEnv {
                rpc_url,
                wallets,
                observer,
                blob_reader,
                ethereum,
                router_query,
                signer,
                validators,
                router_address,
                sender_id: ActorId::from(H160::from(sender_address.0)),
                genesis_block_hash,
                threshold,
                block_time,
                continuous_block_generation,
                broadcaster,
                _anvil: anvil,
                _events_stream,
            })
        }

        pub async fn new_node(&mut self, config: NodeConfig) -> Node {
            let NodeConfig {
                db,
                sequencer_public_key,
                validator_public_key,
                network,
            } = config;

            let db =
                db.unwrap_or_else(|| Database::from_one(&MemDb::default(), self.router_address.0));

            let network_address = network.as_ref().map(|network| {
                network.address.clone().unwrap_or_else(|| {
                    static NONCE: AtomicUsize = AtomicUsize::new(1);
                    let nonce = NONCE.fetch_add(1, Ordering::Relaxed);
                    format!("/memory/{nonce}")
                })
            });

            let network_bootstrap_address = network.and_then(|network| network.bootstrap_address);

            let eth_cfg = EthereumConfig {
                rpc: self.rpc_url.clone(),
                beacon_rpc: Default::default(),
                router_address: self.router_address,
                block_time: self.block_time,
            };
            let observer_service =
                ObserverService::new_with_blobs(&eth_cfg, self.blob_reader.clone())
                    .await
                    .unwrap();

            Node {
                db,
                multiaddr: None,
                rpc_url: self.rpc_url.clone(),
                genesis_block_hash: self.genesis_block_hash,
                blob_reader: self.blob_reader.clone(),
                observer: observer_service,
                signer: self.signer.clone(),
                block_time: self.block_time,
                validators: self.validators.iter().map(|k| k.to_address()).collect(),
                threshold: self.threshold,
                router_address: self.router_address,
                running_service_handle: None,
                sequencer_public_key,
                validator_public_key,
                network_address,
                network_bootstrap_address,
            }
        }

        pub async fn upload_code(&self, code: &[u8]) -> Result<WaitForUploadCode> {
            log::info!("📗 Upload code, len {}", code.len());

            let code_id = CodeId::generate(code);
            let blob_tx = H256::random();

            let listener = self.events_publisher().subscribe().await;

            self.blob_reader
                .add_blob_transaction(blob_tx, code.to_vec())
                .await;
            let _tx_hash = self
                .ethereum
                .router()
                .request_code_validation(code_id, blob_tx)
                .await?;

            Ok(WaitForUploadCode { listener, code_id })
        }

        // TODO (breathx): split it into different functions WITHIN THE PR.
        pub async fn create_program(
            &self,
            code_id: CodeId,
            payload: &[u8],
            value: u128,
        ) -> Result<WaitForProgramCreation> {
            const EXECUTABLE_BALANCE: u128 = 500_000_000_000_000;

            log::info!(
                "📗 Create program, code_id {code_id}, payload len {}",
                payload.len()
            );

            let listener = self.events_publisher().subscribe().await;

            let router = self.ethereum.router();

            let (_, program_id) = router.create_program(code_id, H256::random()).await?;

            let program_address = program_id.to_address_lossy().0.into();

            router
                .wvara()
                .approve(program_address, value + EXECUTABLE_BALANCE)
                .await?;

            let mirror = self.ethereum.mirror(program_address.into_array().into());

            mirror.executable_balance_top_up(EXECUTABLE_BALANCE).await?;

            let (_, message_id) = mirror.send_message(payload, value).await?;

            Ok(WaitForProgramCreation {
                listener,
                program_id,
                message_id,
            })
        }

        pub async fn send_message(
            &self,
            target: ActorId,
            payload: &[u8],
            value: u128,
        ) -> Result<WaitForReplyTo> {
            log::info!("📗 Send message to {target}, payload len {}", payload.len());

            let listener = self.events_publisher().subscribe().await;

            let program_address = ethexe_signer::Address::try_from(target)?;
            let program = self.ethereum.mirror(program_address);

            let (_, message_id) = program.send_message(payload, value).await?;

            Ok(WaitForReplyTo {
                listener,
                message_id,
            })
        }

        pub async fn approve_wvara(&self, program_id: ActorId) {
            log::info!("📗 Approving WVara for {program_id}");

            let program_address = ethexe_signer::Address::try_from(program_id).unwrap();
            let wvara = self.ethereum.router().wvara();
            wvara.approve_all(program_address.0.into()).await.unwrap();
        }

        pub async fn transfer_wvara(&self, program_id: ActorId, value: u128) {
            log::info!("📗 Transferring {value} WVara to {program_id}");

            let program_address = ethexe_signer::Address::try_from(program_id).unwrap();
            let wvara = self.ethereum.router().wvara();
            wvara
                .transfer(program_address.0.into(), value)
                .await
                .unwrap();
        }

        pub fn events_publisher(&self) -> EventsPublisher {
            EventsPublisher {
                broadcaster: self.broadcaster.clone(),
            }
        }

        pub async fn force_new_block(&self) {
            if self.continuous_block_generation {
                // nothing to do: new block will be generated automatically
            } else {
                self.observer.provider().evm_mine(None).await.unwrap();
            }
        }

        pub async fn skip_blocks(&self, blocks_amount: u32) {
            if self.continuous_block_generation {
                tokio::time::sleep(self.block_time.mul(blocks_amount)).await;
            } else {
                self.observer
                    .provider()
                    .evm_mine(Some(MineOptions::Options {
                        timestamp: None,
                        blocks: Some(blocks_amount.into()),
                    }))
                    .await
                    .unwrap();
            }
        }

        #[allow(unused)]
        pub async fn process_already_uploaded_code(
            &self,
            code: &[u8],
            blob_tx_hash: &str,
        ) -> CodeId {
            let code_id = CodeId::generate(code);
            let blob_tx_hash = H256::from_str(blob_tx_hash).unwrap();
            self.blob_reader
                .add_blob_transaction(blob_tx_hash, code.to_vec())
                .await;
            code_id
        }
    }

    pub enum ValidatorsConfig {
        /// Auto generate validators, amount of validators is provided.
        Generated(usize),
        /// Custom validator eth-addresses in hex string format.
        #[allow(unused)]
        Custom(Vec<String>),
    }

    pub struct TestEnvConfig {
        /// How many validators will be in deployed router.
        /// By default uses 1 auto generated validator.
        pub validators: ValidatorsConfig,
        /// By default uses 1 second block time.
        pub block_time: Duration,
        /// By default creates new anvil instance if rpc is not provided.
        pub rpc_url: Option<String>,
        /// By default uses anvil hardcoded wallets if wallets are not provided.
        pub wallets: Option<Vec<String>>,
        /// If None (by default) new router will be deployed.
        /// In case of Some(_), will connect to existing router contract.
        pub router_address: Option<String>,
        /// Identify whether networks works (or have to works) in continuous block generation mode.
        pub continuous_block_generation: bool,
    }

    impl Default for TestEnvConfig {
        fn default() -> Self {
            Self {
                validators: ValidatorsConfig::Generated(1),
                block_time: Duration::from_secs(1),
                rpc_url: None,
                wallets: None,
                router_address: None,
                continuous_block_generation: false,
            }
        }
    }

    // TODO (breathx): consider to remove me in favor of crate::config::NodeConfig.
    #[derive(Default)]
    pub struct NodeConfig {
        /// Database, if not provided, will be created with MemDb.
        pub db: Option<Database>,
        /// Sequencer public key, if provided then new node starts as sequencer.
        pub sequencer_public_key: Option<ethexe_signer::PublicKey>,
        /// Validator public key, if provided then new node starts as validator.
        pub validator_public_key: Option<ethexe_signer::PublicKey>,
        /// Network configuration, if provided then new node starts with network.
        pub network: Option<NodeNetworkConfig>,
    }

    impl NodeConfig {
        pub fn db(mut self, db: Database) -> Self {
            self.db = Some(db);
            self
        }

        pub fn sequencer(mut self, sequencer_public_key: ethexe_signer::PublicKey) -> Self {
            self.sequencer_public_key = Some(sequencer_public_key);
            self
        }

        pub fn validator(mut self, validator_public_key: ethexe_signer::PublicKey) -> Self {
            self.validator_public_key = Some(validator_public_key);
            self
        }

        pub fn network(
            mut self,
            address: Option<String>,
            bootstrap_address: Option<String>,
        ) -> Self {
            self.network = Some(NodeNetworkConfig {
                address,
                bootstrap_address,
            });
            self
        }
    }

    #[derive(Default)]
    pub struct NodeNetworkConfig {
        /// Network address, if not provided, will be generated by test env.
        pub address: Option<String>,
        /// Network bootstrap address, if not provided, then no bootstrap address will be used.
        pub bootstrap_address: Option<String>,
    }

    pub struct EventsPublisher {
        broadcaster: Arc<Mutex<Sender<Event>>>,
    }

    impl EventsPublisher {
        pub async fn subscribe(&self) -> EventsListener {
            EventsListener {
                receiver: self.broadcaster.lock().await.subscribe(),
            }
        }
    }

    pub struct EventsListener {
        receiver: tokio::sync::broadcast::Receiver<Event>,
    }

    impl Clone for EventsListener {
        fn clone(&self) -> Self {
            Self {
                receiver: self.receiver.resubscribe(),
            }
        }
    }

    impl EventsListener {
        pub async fn next_event(&mut self) -> Result<Event> {
            self.receiver.recv().await.map_err(Into::into)
        }

        pub async fn apply_until<R: Sized>(
            &mut self,
            mut f: impl FnMut(Event) -> Result<Option<R>>,
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

        pub async fn apply_until_block_event_with_header<R: Sized>(
            &mut self,
            mut f: impl FnMut(BlockEvent, &SimpleBlockData) -> Result<Option<R>>,
        ) -> Result<R> {
            loop {
                let event = self.next_event().await?;

                let Event::Block(block) = event else {
                    continue;
                };

                let block_data = block.as_simple();

                for event in block.events {
                    if let Some(res) = f(event, &block_data)? {
                        return Ok(res);
                    }
                }
            }
        }
    }

    /// Provides access to hardcoded anvil wallets or custom set wallets.
    pub struct Wallets {
        wallets: Vec<ethexe_signer::PublicKey>,
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
                    .map(|s| signer.add_key(s.as_ref().parse().unwrap()).unwrap())
                    .collect(),
                next_wallet: 0,
            }
        }

        pub fn next(&mut self) -> ethexe_signer::PublicKey {
            let pub_key = self.wallets.get(self.next_wallet).expect("No more wallets");
            self.next_wallet += 1;
            *pub_key
        }
    }

    pub struct Node {
        pub db: Database,
        pub multiaddr: Option<String>,

        rpc_url: String,
        genesis_block_hash: H256,
        blob_reader: Arc<MockBlobReader>,
        observer: ObserverService,
        signer: Signer,
        validators: Vec<ethexe_signer::Address>,
        threshold: u64,
        router_address: ethexe_signer::Address,
        block_time: Duration,
        running_service_handle: Option<JoinHandle<Result<()>>>,
        sequencer_public_key: Option<ethexe_signer::PublicKey>,
        validator_public_key: Option<ethexe_signer::PublicKey>,
        network_address: Option<String>,
        network_bootstrap_address: Option<String>,
    }

    impl Node {
        pub async fn start_service(&mut self) {
            assert!(
                self.running_service_handle.is_none(),
                "Service is already running"
            );

            let processor = Processor::new(self.db.clone()).unwrap();

            let query = Query::new(
                Arc::new(self.db.clone()),
                &self.rpc_url,
                self.router_address,
                self.genesis_block_hash,
                self.blob_reader.clone(),
                10000,
            )
            .await
            .unwrap();

            let router_query = RouterQuery::new(&self.rpc_url, self.router_address)
                .await
                .unwrap();

            let network = self.network_address.as_ref().map(|addr| {
                let config_path = tempfile::tempdir().unwrap().into_path();
                let multiaddr: Multiaddr = addr.parse().unwrap();

                let mut config = ethexe_network::NetworkServiceConfig::new_test(config_path);
                config.listen_addresses = [multiaddr.clone()].into();
                config.external_addresses = [multiaddr.clone()].into();
                if let Some(bootstrap_addr) = self.network_bootstrap_address.as_ref() {
                    let multiaddr = bootstrap_addr.parse().unwrap();
                    config.bootstrap_addresses = [multiaddr].into();
                }
                let network =
                    ethexe_network::NetworkService::new(config, &self.signer, self.db.clone())
                        .unwrap();
                self.multiaddr = Some(format!("{addr}/p2p/{}", network.local_peer_id()));
                network
            });

            let sequencer = match self.sequencer_public_key.as_ref() {
                Some(key) => Some(
                    Sequencer::new(
                        &ethexe_sequencer::Config {
                            ethereum_rpc: self.rpc_url.clone(),
                            sign_tx_public: *key,
                            router_address: self.router_address,
                            validators: self.validators.clone(),
                            threshold: self.threshold,
                        },
                        self.signer.clone(),
                        Box::new(self.db.clone()),
                    )
                    .await
                    .unwrap(),
                ),
                None => None,
            };

            let validator = match self.validator_public_key.as_ref() {
                Some(key) => Some(Validator::new(
                    &ethexe_validator::Config {
                        pub_key: *key,
                        router_address: self.router_address,
                    },
                    self.signer.clone(),
                )),
                None => None,
            };

            let service = Service::new_from_parts(
                self.db.clone(),
                self.observer.cloned().await.unwrap(),
                query,
                router_query,
                processor,
                self.signer.clone(),
                self.block_time,
                network,
                sequencer,
                validator,
                None,
            );
            let handle = task::spawn(service.run());
            self.running_service_handle = Some(handle);

            // Sleep to wait for the new service to start
            tokio::time::sleep(Duration::from_secs(1)).await;
        }

        pub async fn stop_service(&mut self) {
            let handle = self
                .running_service_handle
                .take()
                .expect("Service is not running");
            handle.abort();
            let _ = handle.await;
            self.multiaddr = None;
        }
    }

    #[derive(Clone)]
    pub struct WaitForUploadCode {
        listener: EventsListener,
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
                    Event::CodeLoaded {
                        code_id: loaded_id,
                        code,
                    } if loaded_id == self.code_id => {
                        code_info = Some(code);
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
        listener: EventsListener,
        pub program_id: ActorId,
        pub message_id: MessageId,
    }

    #[derive(Debug)]
    pub struct ProgramCreationInfo {
        pub program_id: ActorId,
        pub code_id: CodeId,
        pub init_message_source: ActorId,
        pub init_message_payload: Vec<u8>,
        pub init_message_value: u128,
        pub reply_payload: Vec<u8>,
        pub reply_code: ReplyCode,
        pub reply_value: u128,
    }

    impl WaitForProgramCreation {
        pub async fn wait_for(mut self) -> Result<ProgramCreationInfo> {
            log::info!("📗 Waiting for program {} creation", self.program_id);

            let mut code_id_info = None;
            let mut init_message_info = None;
            let mut reply_info = None;

            self.listener
                .apply_until_block_event(|event| {
                    match event {
                        BlockEvent::Router(RouterEvent::ProgramCreated { actor_id, code_id })
                            if actor_id == self.program_id =>
                        {
                            code_id_info = Some(code_id);
                        }
                        BlockEvent::Mirror { actor_id, event } if actor_id == self.program_id => {
                            match event {
                                MirrorEvent::MessageQueueingRequested {
                                    source,
                                    payload,
                                    value,
                                    ..
                                } => {
                                    init_message_info = Some((source, payload, value));
                                }
                                MirrorEvent::Reply {
                                    payload,
                                    reply_to,
                                    reply_code,
                                    value,
                                } if self.message_id == reply_to => {
                                    reply_info = Some((payload, reply_code, value));
                                    return Ok(Some(()));
                                }
                                _ => {}
                            }
                        }
                        _ => {}
                    }
                    Ok(None)
                })
                .await?;

            let code_id = code_id_info.expect("Code ID must be set");
            let (init_message_source, init_message_payload, init_message_value) =
                init_message_info.expect("Init message info must be set");
            let (reply_payload, reply_code, reply_value) =
                reply_info.expect("Reply info must be set");

            Ok(ProgramCreationInfo {
                program_id: self.program_id,
                code_id,
                init_message_source,
                init_message_payload,
                init_message_value,
                reply_payload,
                reply_code,
                reply_value,
            })
        }
    }

    #[derive(Clone)]
    pub struct WaitForReplyTo {
        listener: EventsListener,
        pub message_id: MessageId,
    }

    #[derive(Debug)]
    pub struct ReplyInfo {
        pub message_id: MessageId,
        pub program_id: ActorId,
        pub reply_payload: Vec<u8>,
        pub reply_code: ReplyCode,
        pub reply_value: u128,
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
                            reply_payload: payload,
                            reply_code,
                            reply_value: value,
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
