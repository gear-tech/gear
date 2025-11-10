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
        EnvNetworkConfig, Node, NodeConfig, TestEnv, TestEnvConfig, TestingEvent, ValidatorsConfig,
        Wallets, init_logger,
    },
};
use alloy::{
    primitives::U256,
    providers::{Provider as _, WalletProvider, ext::AnvilApi},
};
use ethexe_common::{
    Announce, HashOf, ScheduledTask,
    db::*,
    events::{BlockEvent, MirrorEvent, RouterEvent},
    gear::{CANONICAL_QUARANTINE, MessageType},
    mock::*,
    network::ValidatorMessage,
};
use ethexe_consensus::ConsensusEvent;
use ethexe_compute::ComputeConfig;
use ethexe_db::{Database, verifier::IntegrityVerifier};
use ethexe_ethereum::deploy::ContractsDeploymentParams;
use ethexe_observer::{EthereumConfig, ObserverEvent};
use ethexe_processor::{DEFAULT_BLOCK_GAS_LIMIT_MULTIPLIER, RunnerConfig};
use ethexe_prometheus::PrometheusConfig;
use ethexe_rpc::{RpcConfig, test_utils::JsonRpcResponse};
use ethexe_runtime_common::state::{Expiring, MailboxMessage, PayloadLookup, Storage};
use ethexe_signer::Signer;
use ethexe_tx_pool::{OffchainTransaction, RawOffchainTransaction, TxPoolEvent};
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

const ETHER: u128 = 1_000_000_000_000_000_000;

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
        canonical_quarantine: 0,
        dev: true,
        fast_sync: false,
    };

    let eth_cfg = EthereumConfig {
        rpc: "wss://hoodi-reth-rpc.gear-tech.io/ws".into(),
        beacon_rpc: "https://hoodi-lighthouse-rpc.gear-tech.io".into(),
        router_address: "0x61e49a1B6e387060Da92b1Cd85d640011acAeF26"
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

    let service = Service::new(&config).await.unwrap();

    // Enable all optional services
    let network_key = service.signer.generate_key().unwrap();
    config.network = Some(ethexe_network::NetworkConfig::new_local(
        network_key,
        config.ethereum.router_address,
    ));

    let runner_config = RunnerConfig::overlay(
        config.node.chunk_processing_threads,
        config.node.block_gas_limit,
        DEFAULT_BLOCK_GAS_LIMIT_MULTIPLIER,
    );
    config.rpc = Some(RpcConfig {
        listen_addr: SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 9944),
        cors: None,
        dev: true,
        runner_config,
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

    let async_pid = res.program_id;

    env.approve_wvara(async_pid).await;

    let mut listener = env.observer_events_publisher().subscribe().await;

    let wait_for_mutex_request_command_reply = env
        .send_message(async_pid, &demo_async::Command::Mutex.encode(), 0)
        .await
        .unwrap();

    let original_mid = wait_for_mutex_request_command_reply.message_id;
    let mid_expected_message_id = MessageId::generate_outgoing(original_mid, 0);
    let ping_expected_message_id = MessageId::generate_outgoing(original_mid, 1);

    log::info!("📗 Waiting for announce with PING message committed");
    let (mut block, mut announce_hash) = (None, None);
    listener
        .apply_until_block_event_with_header(|event, block_data| match event {
            BlockEvent::Mirror {
                actor_id,
                event:
                    MirrorEvent::Message {
                        id,
                        destination,
                        payload,
                        ..
                    },
            } if actor_id == async_pid => {
                assert_eq!(destination, env.sender_id);

                if id == mid_expected_message_id {
                    assert_eq!(payload, original_mid.encode());
                } else if id == ping_expected_message_id {
                    assert_eq!(payload, b"PING");
                    block = Some(block_data.clone());
                } else {
                    panic!("Unexpected message id {id}");
                }

                Ok(None)
            }
            BlockEvent::Router(RouterEvent::AnnouncesCommitted(ah)) if block.is_some() => {
                announce_hash = Some(ah);
                Ok(Some(()))
            }
            _ => Ok(None),
        })
        .await
        .unwrap();

    let block = block.expect("must be set");
    let announce_hash = announce_hash.expect("must be set");

    // -1 bcs execution took place in previous block, not the one that emits events.
    let wake_expiry = block.header.height - 1 + 100; // 100 is default wait for.
    let expiry = block.header.height - 1 + ethexe_runtime_common::state::MAILBOX_VALIDITY;

    let expected_schedule = BTreeMap::from_iter([
        (
            wake_expiry,
            BTreeSet::from_iter([ScheduledTask::WakeMessage(async_pid, original_mid)]),
        ),
        (
            expiry,
            BTreeSet::from_iter([
                ScheduledTask::RemoveFromMailbox(
                    (async_pid, env.sender_id),
                    mid_expected_message_id,
                ),
                ScheduledTask::RemoveFromMailbox(
                    (async_pid, env.sender_id),
                    ping_expected_message_id,
                ),
            ]),
        ),
    ]);

    let schedule = node
        .db
        .announce_schedule(announce_hash)
        .expect("must exist");

    assert_eq!(schedule, expected_schedule);

    let mid_payload = PayloadLookup::Direct(original_mid.into_bytes().to_vec().try_into().unwrap());
    let ping_payload = PayloadLookup::Direct(b"PING".to_vec().try_into().unwrap());

    let expected_mailbox = BTreeMap::from_iter([(
        env.sender_id,
        BTreeMap::from_iter([
            (
                mid_expected_message_id,
                Expiring {
                    value: MailboxMessage {
                        payload: mid_payload.clone(),
                        value: 0,
                        message_type: MessageType::Canonical,
                    },
                    expiry,
                },
            ),
            (
                ping_expected_message_id,
                Expiring {
                    value: MailboxMessage {
                        payload: ping_payload,
                        value: 0,
                        message_type: MessageType::Canonical,
                    },
                    expiry,
                },
            ),
        ]),
    )]);

    let mirror = env.ethereum.mirror(async_pid.try_into().unwrap());
    let state_hash = mirror.query().state_hash().await.unwrap();

    let state = node.db.program_state(state_hash).unwrap();
    assert!(!state.mailbox_hash.is_empty());
    let mailbox = state
        .mailbox_hash
        .map_or_default(|hash| node.db.mailbox(hash).unwrap());

    assert_eq!(mailbox.into_values(&node.db), expected_mailbox);

    mirror
        .send_reply(ping_expected_message_id, "PONG", 0)
        .await
        .unwrap();

    let reply_info = wait_for_mutex_request_command_reply
        .wait_for()
        .await
        .unwrap();
    assert_eq!(
        reply_info.code,
        ReplyCode::Success(SuccessReplyReason::Manual)
    );
    assert_eq!(reply_info.payload, original_mid.encode());

    let state_hash = mirror.query().state_hash().await.unwrap();

    let state = node.db.program_state(state_hash).unwrap();
    assert!(!state.mailbox_hash.is_empty());
    let mailbox = state
        .mailbox_hash
        .map_or_default(|hash| node.db.mailbox(hash).unwrap());

    let expected_mailbox = BTreeMap::from_iter([(
        env.sender_id,
        BTreeMap::from_iter([(
            mid_expected_message_id,
            Expiring {
                value: MailboxMessage {
                    payload: mid_payload,
                    value: 0,
                    message_type: MessageType::Canonical,
                },
                expiry,
            },
        )]),
    )]);

    assert_eq!(mailbox.into_values(&node.db), expected_mailbox);

    log::info!("📗 Claiming value for message {mid_expected_message_id}");
    mirror.claim_value(mid_expected_message_id).await.unwrap();

    let mut claimed = false;
    let announce_hash = listener
        .apply_until_block_event(|event| match event {
            BlockEvent::Mirror {
                actor_id,
                event: MirrorEvent::ValueClaimed { claimed_id, .. },
            } if actor_id == async_pid && claimed_id == mid_expected_message_id => {
                claimed = true;
                Ok(None)
            }
            BlockEvent::Router(RouterEvent::AnnouncesCommitted(ah)) if claimed => Ok(Some(ah)),
            _ => Ok(None),
        })
        .await
        .unwrap();
    assert!(claimed, "Value must be claimed");

    let state_hash = mirror.query().state_hash().await.unwrap();

    let state = node.db.program_state(state_hash).unwrap();
    assert!(state.mailbox_hash.is_empty());

    let schedule = node
        .db
        .announce_schedule(announce_hash)
        .expect("must exist");
    assert!(schedule.is_empty(), "{schedule:?}");
}

#[tokio::test(flavor = "multi_thread")]
#[ntest::timeout(60_000)]
async fn value_in_reply() {
    init_logger();

    let mut env = TestEnv::new(Default::default()).await.unwrap();

    let mut node = env.new_node(NodeConfig::default().validator(env.validators[0]));
    node.start_service().await;

    let res = env
        .upload_code(demo_piggy_bank::WASM_BINARY)
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
        .send_message(res.program_id, b"", 0)
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();

    let piggy_bank_id = res.program_id;

    let wvara = env.ethereum.router().wvara();

    assert_eq!(wvara.query().decimals().await.unwrap(), 12);

    let piggy_bank = env.ethereum.mirror(piggy_bank_id.to_address_lossy().into());

    let on_eth_balance = piggy_bank.get_balance().await.unwrap();
    assert_eq!(on_eth_balance, 0);

    let state_hash = piggy_bank.query().state_hash().await.unwrap();
    let local_balance = node.db.program_state(state_hash).unwrap().balance;
    assert_eq!(local_balance, 0);

    // 1_000 ETH
    const VALUE_SENT: u128 = 1_000 * ETHER;

    let mut listener = env.observer_events_publisher().subscribe().await;

    piggy_bank.owned_balance_top_up(VALUE_SENT).await.unwrap();

    listener
        .apply_until_block_event(|e| {
            Ok(matches!(e, BlockEvent::Router(RouterEvent::BatchCommitted { .. })).then_some(()))
        })
        .await
        .unwrap();

    let on_eth_balance = piggy_bank.get_balance().await.unwrap();
    assert_eq!(on_eth_balance, VALUE_SENT);

    let state_hash = piggy_bank.query().state_hash().await.unwrap();
    let local_balance = node.db.program_state(state_hash).unwrap().balance;
    assert_eq!(local_balance, VALUE_SENT);

    env.approve_wvara(piggy_bank_id).await;

    let res = env
        .send_message(piggy_bank_id, b"smash_with_reply", 0)
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();

    assert_eq!(res.code, ReplyCode::Success(SuccessReplyReason::Manual));
    assert_eq!(res.value, VALUE_SENT);

    let on_eth_balance = piggy_bank.get_balance().await.unwrap();
    assert_eq!(on_eth_balance, 0);

    let state_hash = piggy_bank.query().state_hash().await.unwrap();
    let local_balance = node.db.program_state(state_hash).unwrap().balance;
    assert_eq!(local_balance, 0);

    let sender_address = env.ethereum.provider().default_signer_address();
    let measurement_error: U256 = (ETHER / 50).try_into().unwrap(); // 0.02 ETH for gas costs
    let default_anvil_balance: U256 = (10_000 * ETHER).try_into().unwrap();
    let balance = env
        .ethereum
        .provider()
        .get_balance(sender_address)
        .await
        .unwrap();
    assert!(default_anvil_balance - balance <= measurement_error);
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

    let on_eth_balance = ping.get_balance().await.unwrap();
    assert_eq!(on_eth_balance, 0);

    let state_hash = ping.query().state_hash().await.unwrap();
    let local_balance = node.db.program_state(state_hash).unwrap().balance;
    assert_eq!(local_balance, 0);

    // 1_000 ETH
    const VALUE_SENT: u128 = 1_000 * ETHER;

    let mut listener = env.observer_events_publisher().subscribe().await;

    ping.owned_balance_top_up(VALUE_SENT).await.unwrap();

    listener
        .apply_until_block_event(|e| {
            Ok(matches!(e, BlockEvent::Router(RouterEvent::BatchCommitted { .. })).then_some(()))
        })
        .await
        .unwrap();

    let on_eth_balance = ping.get_balance().await.unwrap();
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

    let on_eth_balance = ping.get_balance().await.unwrap();
    assert_eq!(on_eth_balance, 2 * VALUE_SENT);

    let state_hash = ping.query().state_hash().await.unwrap();
    let local_balance = node.db.program_state(state_hash).unwrap().balance;
    assert_eq!(local_balance, 2 * VALUE_SENT);
}

#[tokio::test(flavor = "multi_thread")]
#[ntest::timeout(60_000)]
async fn ping_reorg() {
    init_logger();

    let mut env = TestEnv::new(TestEnvConfig {
        network: EnvNetworkConfig::Enabled,
        ..Default::default()
    })
    .await
    .unwrap();

    // Start a separate connect node, to be able to request missed announces.
    let mut connect_node = env.new_node(NodeConfig::named("connect"));
    connect_node.start_service().await;

    let mut node = env.new_node(NodeConfig::named("validator").validator(env.validators[0]));
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
    validators[1].stop_service().await;

    while env.next_block_producer_index().await != 2 {
        log::info!("📗 Skip one block to be sure validator 2 is a producer for next block");
        env.skip_blocks(1).await;
    }

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

    // IMPORTANT: mine some blocks
    // to force validator 0 and validator 2 to have the same announces chain.
    // While validator 0 and 1 were down, validator 2 produced announce alone
    // and supposed that best chain is its own, but as soon as this announce is not committed
    // to ethereum yet, other validators don't see it and have different best chain.
    // To avoid such situation, we just mine few blocks to be sure validators would be on the same chain.
    for _ in 0..env.commitment_delay_limit {
        env.force_new_block().await;
    }

    if env.next_block_producer_index().await == 1 {
        log::info!("📗 Skip one block to be sure validator 1 is not a producer for next block");
        env.force_new_block().await;
    }

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
        .latest_data()
        .expect("latest data not found")
        .prepared_block_hash;

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

    // Tx executable validation takes time, so wait for event.
    node1
        .listener()
        .wait_for(|event| {
            Ok(matches!(
                event,
                TestingEvent::TxPool(TxPoolEvent::PublishOffchainTransaction(_))
            ))
        })
        .await
        .unwrap();

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

        let alice_latest_data = alice.db.latest_data().expect("latest data not found");
        let bob_latest_data = bob.db.latest_data().expect("latest data not found");
        assert_eq!(
            alice_latest_data.computed_announce_hash,
            bob_latest_data.computed_announce_hash
        );
        assert_eq!(
            alice_latest_data.synced_block_height,
            bob_latest_data.synced_block_height
        );
        assert_eq!(
            alice_latest_data.prepared_block_hash,
            bob_latest_data.prepared_block_hash
        );
        assert_eq!(
            alice_latest_data.genesis_block_hash,
            bob_latest_data.genesis_block_hash
        );
        assert_eq!(
            alice_latest_data.genesis_announce_hash,
            bob_latest_data.genesis_announce_hash
        );

        let mut block = latest_block;
        loop {
            if fast_synced_block == block {
                break;
            }

            log::trace!("assert block {block}");

            // Check block meta, exclude codes_queue and announces, which can vary, and it's ok
            let alice_meta = alice.db.block_meta(block);
            let bob_meta = bob.db.block_meta(block);
            assert!(
                alice_meta.prepared && bob_meta.prepared,
                "Block {block} is not prepared for alice or bob"
            );
            assert_eq!(
                alice_meta.last_committed_announce,
                bob_meta.last_committed_announce
            );
            assert_eq!(
                alice_meta.last_committed_batch,
                bob_meta.last_committed_batch
            );

            let Some((alice_announces, bob_announces)) =
                alice_meta.announces.zip(bob_meta.announces)
            else {
                panic!("alice or bob has no announces");
            };

            for &announce_hash in alice_announces.intersection(&bob_announces) {
                if alice.db.announce_meta(announce_hash).computed
                    != bob.db.announce_meta(announce_hash).computed
                {
                    continue;
                }

                assert_eq!(
                    alice.db.announce_program_states(announce_hash),
                    bob.db.announce_program_states(announce_hash)
                );
                assert_eq!(
                    alice.db.announce_outcome(announce_hash),
                    bob.db.announce_outcome(announce_hash)
                );
                assert_eq!(
                    alice.db.announce_outcome(announce_hash),
                    bob.db.announce_outcome(announce_hash)
                );
            }

            assert_eq!(alice.db.block_header(block), bob.db.block_header(block));
            assert_eq!(alice.db.block_events(block), bob.db.block_events(block));
            assert_eq!(alice.db.block_synced(block), bob.db.block_synced(block));

            let header = alice.db.block_header(block).unwrap();
            block = header.parent_hash;
        }
    };

    let config = TestEnvConfig {
        network: EnvNetworkConfig::Enabled,
        ..Default::default()
    };
    let mut env = TestEnv::new(config).await.unwrap();

    log::info!("📗 Starting Alice");
    let mut alice = env.new_node(NodeConfig::named("Alice").validator(env.validators[0]));
    alice.start_service().await;

    log::info!("📗 Creating `demo-autoreply` programs");

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

    let latest_block: H256 = env.latest_block().await.hash.0.into();
    alice
        .listener()
        .wait_for_announce_computed(latest_block)
        .await;

    log::info!("Starting Bob (fast-sync)");
    let mut bob = env.new_node(NodeConfig::named("Bob").fast_sync());

    bob.start_service().await;

    log::info!("📗 Sending messages to programs");

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
        .wait_for_announce_computed(latest_block)
        .await;
    bob.listener()
        .wait_for_announce_computed(latest_block)
        .await;

    log::info!("📗 Stopping Bob");
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

    let latest_block: H256 = env.latest_block().await.hash.0.into();
    alice
        .listener()
        .wait_for_announce_computed(latest_block)
        .await;

    log::info!("📗 Starting Bob again to check how it handles partially empty database");
    bob.start_service().await;

    // Mine some blocks so Bob can produce the event we will wait for.
    // We mine several blocks here to ensure that Bob and Alice would converge to the same chain of announces.
    // Why do we need that? Because Bob was disabled he missed some announces that Alice produced,
    // this announces was not committed, so Bob would not see them during fast-sync
    // and would not have them in his database. This is normal situation, after a few blocks Bob and Alice should
    // converge to the same chain of announces.
    for _ in 0..env.commitment_delay_limit {
        env.skip_blocks(1).await;
    }

    let latest_block = env.latest_block().await.hash.0.into();
    alice
        .listener()
        .wait_for_announce_computed(latest_block)
        .await;
    bob.listener()
        .wait_for_announce_computed(latest_block)
        .await;

    assert_chain(
        latest_block,
        bob.latest_fast_synced_block.take().unwrap(),
        &alice,
        &bob,
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ntest::timeout(60_000)]
async fn validators_election() {
    init_logger();

    // Setup test environment

    let election_ts = 20 * 60 * 60;
    let era_duration = 24 * 60 * 60;
    let deploy_params = ContractsDeploymentParams {
        with_middleware: true,
        era_duration,
        election_duration: era_duration - election_ts,
    };

    let signer = Signer::memory();
    // 10 wallets - hardcoded in anvil
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

    // Start initial validators
    let mut validators = vec![];
    for (i, v) in env.validators.clone().into_iter().enumerate() {
        log::info!("📗 Starting validator-{i}");
        let mut validator = env.new_node(NodeConfig::named(format!("validator-{i}")).validator(v));
        validator.start_service().await;
        validators.push(validator);
    }

    // Setup next validators to be elected for previous era
    let (next_validators_configs, _commitment) =
        TestEnv::define_session_keys(&signer, next_validators);

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

    // Force creation new block in election period
    env.provider
        .anvil_set_next_block_timestamp(election_ts + genesis_ts)
        .await
        .unwrap();
    env.force_new_block().await;

    let mut listener = env.observer_events_publisher().subscribe().await;
    listener
        .apply_until_block_event(|event| {
            Ok(matches!(
                event,
                BlockEvent::Router(RouterEvent::ValidatorsCommittedForEra { era_index: _ })
            )
            .then_some(()))
        })
        .await
        .unwrap();

    tracing::info!("📗 Next validators successfully committed");

    // Stop previous validators
    for mut node in validators.into_iter() {
        node.stop_service().await;
    }

    // Check that next validators can submit transactions
    env.validators = next_validators_configs;
    let mut new_validators = vec![];
    for (i, v) in env.validators.clone().into_iter().enumerate() {
        log::info!("📗 Starting validator-{i}");
        let mut validator = env.new_node(NodeConfig::named(format!("validator-{i}")).validator(v));
        validator.start_service().await;
        new_validators.push(validator);
    }

    env.provider
        .anvil_set_next_block_timestamp(era_duration + genesis_ts)
        .await
        .unwrap();
    env.force_new_block().await;

    let res = env
        .upload_code(demo_ping::WASM_BINARY)
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();
    assert!(res.valid);
}

#[tokio::test(flavor = "multi_thread")]
#[ntest::timeout(30_000)]
async fn execution_with_canonical_events_quarantine() {
    init_logger();

    let config = TestEnvConfig {
        compute_config: ComputeConfig::new(CANONICAL_QUARANTINE),
        ..Default::default()
    };
    let mut env = TestEnv::new(config).await.unwrap();

    log::info!("📗 Starting validator");
    let mut validator = env.new_node(NodeConfig::default().validator(env.validators[0]));
    validator.start_service().await;

    let uploaded_code = env
        .upload_code(demo_ping::WASM_BINARY)
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();
    assert!(uploaded_code.valid);

    let res = env
        .create_program(uploaded_code.code_id, 500_000_000_000_000)
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();
    assert_eq!(res.code_id, uploaded_code.code_id);

    let canonical_quarantine = env.compute_config.canonical_quarantine();
    let message_id = env
        .send_message(res.program_id, b"PING", 0)
        .await
        .unwrap()
        .message_id;

    env.provider.anvil_mine(Some(1), None).await.unwrap();

    let mut listener = env.observer_events_publisher().subscribe().await;

    // Skipping events to reach canonical events maturity
    let mut skipped_blocks = 0;
    while skipped_blocks < canonical_quarantine {
        env.provider.anvil_mine(Some(1), None).await.unwrap();
        tokio::time::sleep(Duration::from_millis(150)).await;

        if let ObserverEvent::BlockSynced(..) = listener.next_event().await.unwrap() {
            skipped_blocks += 1
        };
    }

    // Now waiting for the PONG reply
    loop {
        let synced_block = match listener.next_event().await.unwrap() {
            ObserverEvent::BlockSynced(block_hash) => block_hash,
            _ => {
                continue;
            }
        };

        let Some(block_events) = validator.db.block_events(synced_block) else {
            continue;
        };

        for block_event in block_events {
            if let BlockEvent::Mirror {
                actor_id: _,
                event:
                    MirrorEvent::Reply {
                        payload,
                        value: _,
                        reply_to,
                        reply_code: _,
                    },
            } = block_event
                && reply_to == message_id
                && payload == b"PONG"
            {
                return;
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ntest::timeout(120_000)]
async fn announces_conflicts() {
    init_logger();

    let mut env = TestEnv::new(TestEnvConfig {
        validators: ValidatorsConfig::PreDefined(7),
        network: EnvNetworkConfig::Enabled,
        ..Default::default()
    })
    .await
    .unwrap();

    let mut validators = vec![];
    for (i, v) in env.validators.clone().into_iter().enumerate() {
        log::info!("📗 Starting validator-{i}");
        let mut validator = env.new_node(NodeConfig::named(format!("validator-{i}")).validator(v));
        validator.start_service().await;
        validators.push(validator);
    }

    let ping_code_id = env
        .upload_code(demo_ping::WASM_BINARY)
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap()
        .tap(|res| assert!(res.valid))
        .code_id;

    let ping_id = env
        .create_program(ping_code_id, 500_000_000_000_000)
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap()
        .tap(|res| assert_eq!(res.code_id, ping_code_id))
        .program_id;

    env.send_message(ping_id, b"", 0)
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap()
        .tap(|res| {
            assert_eq!(res.program_id, ping_id);
            assert_eq!(res.payload, b"");
            assert_eq!(res.value, 0);
            assert_eq!(res.code, ReplyCode::Success(SuccessReplyReason::Auto));
        });

    {
        log::info!("📗 Case 1: all validators works normally");

        env.send_message(ping_id, b"PING", 0)
            .await
            .unwrap()
            .wait_for()
            .await
            .unwrap()
            .tap(|res| {
                assert_eq!(res.program_id, ping_id);
                assert_eq!(res.payload, b"PONG");
                assert_eq!(res.value, 0);
                assert_eq!(res.code, ReplyCode::Success(SuccessReplyReason::Manual));
            });
    }

    let (mut listeners, validator0, wait_for_pong) = {
        log::info!("📗 Case 2: stop validator 0, and publish incorrect announce manually");

        env.wait_for_next_producer_index(0).await;

        let mut validator0 = validators.remove(0);
        validator0.stop_service().await;

        let mut listeners = validators
            .iter_mut()
            .map(|node| node.listener())
            .collect::<Vec<_>>();

        let wait_for_pong = env.send_message(ping_id, b"PING", 0).await.unwrap();

        let block = env.latest_block().await;
        let announce = Announce::with_default_gas(block.hash, HashOf::random());
        let announce_hash = announce.to_hash();
        validator0
            .publish_validator_message(ValidatorMessage {
                block: block.hash,
                payload: announce,
            })
            .await;

        // Validators 1..=6 must reject this announce
        futures::future::join_all(listeners.iter_mut().map(|l| {
            l.apply_until(|event| {
                Ok(matches!(
                    event,
                    TestingEvent::Consensus(ConsensusEvent::AnnounceRejected(rejected_announce_hash))
                        if rejected_announce_hash == announce_hash
                )
                .then_some(()))
            })
        }))
        .await
        .into_iter()
        .collect::<Result<Vec<()>, _>>()
        .unwrap();

        (listeners, validator0, wait_for_pong)
    };

    let latest_computed_announce_hash = {
        log::info!(
            "📗 Case 3: next block producer must be validator 1, so reply PONG must be delivered"
        );

        assert_eq!(env.next_block_producer_index().await, 1);
        env.force_new_block().await;
        wait_for_pong.wait_for().await.unwrap().tap(|res| {
            assert_eq!(res.program_id, ping_id);
            assert_eq!(res.payload, b"PONG");
            assert_eq!(res.value, 0);
            assert_eq!(res.code, ReplyCode::Success(SuccessReplyReason::Manual));
        });

        // Wait till all validators accept announce for the latest block
        let latest_block = env.latest_block().await.hash;
        let mut latest_computed_announce_hash = HashOf::zero();
        for listener in &mut listeners {
            let announce_hash = listener.wait_for_announce_computed(latest_block).await;
            assert!(
                latest_computed_announce_hash == HashOf::zero()
                    || latest_computed_announce_hash == announce_hash,
                "All validators must compute the same announce for the latest block"
            );
            latest_computed_announce_hash = announce_hash;
        }

        latest_computed_announce_hash
    };

    let wait_for_pong = {
        // Skip validators 3, 4, 5 (increasing timestamp). Stop validator 6,
        // and emulate correct announce6 publishing from validator 6,
        // but do not aggregate commitments.
        // After that emulate validators 0 (which is already stopped before)
        // send correct announce7 for the next block,
        // but announce7 is from different chain than announce6, so announce7 must be rejected.
        log::info!("📗 Case 4: announce chains conflict");

        // because of commitment processing from previous step - next producer is 3
        assert_eq!(env.next_block_producer_index().await, 3);

        // skip slots for validators 3, 4, 5 and go to the timestamp, where next block producer is validator 6
        env.provider
            .anvil_set_next_block_timestamp(
                env.latest_block().await.header.timestamp + env.block_time.as_secs() * 4,
            )
            .await
            .unwrap();

        // Get access to validator 1 db, to be able to access fresh announces
        let validator1_db = validators[1].db.clone();

        // Stop validator 6
        // Note: index - 1, because validator 0 is already removed
        let mut validator6 = validators.remove(6 - 1);
        validator6.stop_service().await;

        // Listeners for validators 1..=5
        let mut listeners = validators
            .iter_mut()
            .map(|node| node.listener())
            .collect::<Vec<_>>();

        let _ = env.send_message(ping_id, b"PING", 0).await.unwrap();

        // Next block producer is validator 0 - because validators 3, 4, 5 were skipped and 6 is current
        assert_eq!(env.next_block_producer_index().await, 0);

        // Send announce from stopped validator 6
        let block = env.latest_block().await;
        let announce6 = Announce::with_default_gas(block.hash, latest_computed_announce_hash);
        let announce6_hash = announce6.to_hash();
        validator6
            .publish_validator_message(ValidatorMessage {
                block: block.hash,
                payload: announce6,
            })
            .await;
        for listener in &mut listeners {
            listener.wait_for_announce_computed(announce6_hash).await;
        }

        // Commitment does not sent by validator 6,
        // so now next producer is the next in order - validator 0
        assert_eq!(env.next_block_producer_index().await, 0);

        let wait_for_pong = env.send_message(ping_id, b"PING", 0).await.unwrap();

        // Ignore announce6 and build announce7 on top of base announce from parent block
        // Announce is not on top of announce6 (already accepted),
        // so must be rejected by validators 1..=5
        let block = env.latest_block().await;
        let parent = validator1_db
            .block_meta(block.header.parent_hash)
            .announces
            .into_iter()
            .flatten()
            .find(|&announce_hash| validator1_db.announce(announce_hash).unwrap().is_base())
            .expect("base announces not found");
        let announce7 = Announce::with_default_gas(block.hash, parent);
        let announce7_hash = announce7.to_hash();
        validator0
            .publish_validator_message(ValidatorMessage {
                block: block.hash,
                payload: announce7,
            })
            .await;

        // Validators 1..=5 must reject this announce
        futures::future::join_all(listeners.iter_mut().map(|l| {
            l.apply_until(|event| {
                Ok(matches!(
                    event,
                    TestingEvent::Consensus(ConsensusEvent::AnnounceRejected(announce_hash))
                        if announce_hash == announce7_hash
                )
                .then_some(()))
            })
        }))
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

        wait_for_pong
    };

    {
        log::info!(
            "📗 Case 5: announce from validator 0 was rejected but still validator 1 could process all in the next block"
        );

        assert_eq!(env.next_block_producer_index().await, 1);
        env.force_new_block().await;
        wait_for_pong.wait_for().await.unwrap().tap(|res| {
            assert_eq!(res.program_id, ping_id);
            assert_eq!(res.payload, b"PONG");
            assert_eq!(res.value, 0);
            assert_eq!(res.code, ReplyCode::Success(SuccessReplyReason::Manual));
        });
    }
}
