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
        AnnounceId, EnvNetworkConfig, Node, NodeConfig, TestEnv, TestEnvConfig, TestingEvent,
        TestingRpcEvent, ValidatorsConfig, WaitForReplyTo, Wallets, init_logger,
    },
};
use alloy::{
    primitives::U256,
    providers::{Provider as _, WalletProvider, ext::AnvilApi},
};
use ethexe_common::{
    Announce, HashOf, ScheduledTask, ToDigest,
    consensus::{DEFAULT_CHAIN_DEEPNESS_THRESHOLD, DEFAULT_VALIDATE_CHAIN_DEEPNESS_LIMIT},
    db::*,
    ecdsa::ContractSignature,
    events::{BlockEvent, MirrorEvent, RouterEvent},
    gear::{BatchCommitment, CANONICAL_QUARANTINE, MessageType},
    injected::{InjectedTransaction, RpcOrNetworkInjectedTx},
    mock::*,
    network::ValidatorMessage,
};
use ethexe_compute::ComputeConfig;
use ethexe_consensus::{BatchCommitter, ConsensusEvent};
use ethexe_db::{Database, verifier::IntegrityVerifier};
use ethexe_ethereum::{TryGetReceipt, deploy::ContractsDeploymentParams, router::Router};
use ethexe_observer::{EthereumConfig, ObserverEvent};
use ethexe_processor::{DEFAULT_BLOCK_GAS_LIMIT_MULTIPLIER, RunnerConfig};
use ethexe_prometheus::PrometheusConfig;
use ethexe_rpc::{InjectedClient, InjectedTransactionAcceptance, RpcConfig};
use ethexe_runtime_common::state::{Expiring, MailboxMessage, PayloadLookup, Storage};
use ethexe_signer::Signer;
use gear_core::{
    ids::prelude::*,
    message::{ReplyCode, SuccessReplyReason},
};
use gear_core_errors::{ErrorReplyReason, SimpleExecutionError, SimpleUnavailableActorError};
use gprimitives::{ActorId, H160, H256, MessageId};
use parity_scale_codec::Encode;
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    net::{Ipv4Addr, SocketAddr},
    ops::ControlFlow,
    sync::Arc,
    time::Duration,
};
use tempfile::tempdir;
use tokio::sync::{
    Mutex,
    mpsc::{self, UnboundedReceiver, UnboundedSender},
};

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
        dev: false,
        fast_sync: false,
        validate_chain_deepness_limit: DEFAULT_VALIDATE_CHAIN_DEEPNESS_LIMIT,
        chain_deepness_threshold: DEFAULT_CHAIN_DEEPNESS_THRESHOLD,
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
        .send_message(res.program_id, b"PING")
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();

    assert_eq!(res.code, ReplyCode::Success(SuccessReplyReason::Manual));
    assert_eq!(res.payload, b"PONG");
    assert_eq!(res.value, 0);

    let ping_id = res.program_id;

    let res = env
        .send_message(ping_id, b"PING")
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
        .send_message(ping_id, b"PUNK")
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
            .send_message(res.program_id, &[])
            .await
            .unwrap()
            .wait_for()
            .await
            .unwrap();

        let expected_err = ReplyCode::Error(SimpleExecutionError::UserspacePanic.into());
        assert_eq!(reply.code, expected_err);

        let res = env
            .send_message(res.program_id, &[])
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

        let mut listener = env.observer_events_publisher().subscribe();

        let init_res = env
            .create_program_with_params(code_id, H256([0x11; 32]), None, 500_000_000_000_000)
            .await
            .unwrap()
            .wait_for()
            .await
            .unwrap();
        let init_reply = env
            .send_message(init_res.program_id, &init_payload)
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
                        Ok(ControlFlow::Break(()))
                    } else {
                        Ok(ControlFlow::Continue(()))
                    }
                }
                _ => Ok(ControlFlow::Continue(())),
            })
            .await
            .unwrap();

        // Handle message to uninitialized program.
        let res = env
            .send_message(init_res.program_id, &[])
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
                    Ok(ControlFlow::Break(reply_code))
                }
                _ => Ok(ControlFlow::Continue(())),
            })
            .await
            .unwrap();

        assert!(code.is_success());

        // Handle message handled, but panicked due to incorrect payload as expected.
        let res = env
            .send_message(res.program_id, &[])
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
        .send_message(res.program_id, &env.sender_id.encode())
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();
    assert_eq!(init_res.code, ReplyCode::Success(SuccessReplyReason::Auto));

    let async_pid = res.program_id;

    let mut listener = env.observer_events_publisher().subscribe();

    let wait_for_mutex_request_command_reply = env
        .send_message(async_pid, &demo_async::Command::Mutex.encode())
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
                    block = Some(*block_data);
                } else {
                    panic!("Unexpected message id {id}");
                }

                Ok(ControlFlow::Continue(()))
            }
            BlockEvent::Router(RouterEvent::AnnouncesCommitted(ah)) if block.is_some() => {
                announce_hash = Some(ah);
                Ok(ControlFlow::Break(()))
            }
            _ => Ok(ControlFlow::Continue(())),
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
                Ok(ControlFlow::Continue(()))
            }
            BlockEvent::Router(RouterEvent::AnnouncesCommitted(ah)) if claimed => {
                Ok(ControlFlow::Break(ah))
            }
            _ => Ok(ControlFlow::Continue(())),
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
async fn value_reply_program_to_user() {
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
        .send_message(res.program_id, b"")
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

    let mut listener = env.observer_events_publisher().subscribe();

    piggy_bank.owned_balance_top_up(VALUE_SENT).await.unwrap();

    listener
        .apply_until_block_event(|e| {
            if matches!(e, BlockEvent::Router(RouterEvent::BatchCommitted { .. })) {
                Ok(ControlFlow::Break(()))
            } else {
                Ok(ControlFlow::Continue(()))
            }
        })
        .await
        .unwrap();

    let on_eth_balance = piggy_bank.get_balance().await.unwrap();
    assert_eq!(on_eth_balance, VALUE_SENT);

    let state_hash = piggy_bank.query().state_hash().await.unwrap();
    let local_balance = node.db.program_state(state_hash).unwrap().balance;
    assert_eq!(local_balance, VALUE_SENT);

    let res = env
        .send_message(piggy_bank_id, b"smash_with_reply")
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
async fn value_send_program_to_user_and_claimed() {
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
        .send_message(res.program_id, b"")
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

    let mut listener = env.observer_events_publisher().subscribe();

    piggy_bank.owned_balance_top_up(VALUE_SENT).await.unwrap();

    listener
        .apply_until_block_event(|e| {
            if matches!(e, BlockEvent::Router(RouterEvent::BatchCommitted { .. })) {
                Ok(ControlFlow::Break(()))
            } else {
                Ok(ControlFlow::Continue(()))
            }
        })
        .await
        .unwrap();

    let on_eth_balance = piggy_bank.get_balance().await.unwrap();
    assert_eq!(on_eth_balance, VALUE_SENT);

    let state_hash = piggy_bank.query().state_hash().await.unwrap();
    let local_balance = node.db.program_state(state_hash).unwrap().balance;
    assert_eq!(local_balance, VALUE_SENT);

    let res = env
        .send_message(piggy_bank_id, b"smash")
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();

    assert_eq!(res.code, ReplyCode::Success(SuccessReplyReason::Auto));
    assert_eq!(res.value, 0);

    let on_eth_balance = piggy_bank.get_balance().await.unwrap();
    assert_eq!(on_eth_balance, 0);

    let state_hash = piggy_bank.query().state_hash().await.unwrap();
    let local_balance = node.db.program_state(state_hash).unwrap().balance;
    assert_eq!(local_balance, 0);

    let router_address = env.ethereum.router().address();
    let router_balance = env
        .ethereum
        .provider()
        .get_balance(router_address.into())
        .await
        .map(ethexe_ethereum::abi::utils::uint256_to_u128_lossy)
        .unwrap();

    assert_eq!(router_balance, VALUE_SENT);

    let sender_address = env.ethereum.provider().default_signer_address();

    let program_state = node.db.program_state(state_hash).unwrap();
    let mailbox = node
        .db
        .mailbox(program_state.mailbox_hash.to_inner().unwrap())
        .unwrap();
    let user_mailbox = mailbox.into_values(&node.db)[&sender_address.into()].clone();
    let mailboxed_msg_id = user_mailbox.into_keys().next().unwrap();

    piggy_bank.claim_value(mailboxed_msg_id).await.unwrap();

    listener
        .apply_until_block_event(|e| match e {
            BlockEvent::Mirror {
                actor_id,
                event: MirrorEvent::ValueClaimed { claimed_id, .. },
            } if actor_id == piggy_bank_id && claimed_id == mailboxed_msg_id => {
                Ok(ControlFlow::Break(()))
            }
            _ => Ok(ControlFlow::Continue(())),
        })
        .await
        .unwrap();

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
async fn value_send_program_to_user_and_replied() {
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
        .send_message(res.program_id, b"")
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

    let mut listener = env.observer_events_publisher().subscribe();

    piggy_bank.owned_balance_top_up(VALUE_SENT).await.unwrap();

    listener
        .apply_until_block_event(|e| {
            if matches!(e, BlockEvent::Router(RouterEvent::BatchCommitted { .. })) {
                Ok(ControlFlow::Break(()))
            } else {
                Ok(ControlFlow::Continue(()))
            }
        })
        .await
        .unwrap();

    let on_eth_balance = piggy_bank.get_balance().await.unwrap();
    assert_eq!(on_eth_balance, VALUE_SENT);

    let state_hash = piggy_bank.query().state_hash().await.unwrap();
    let local_balance = node.db.program_state(state_hash).unwrap().balance;
    assert_eq!(local_balance, VALUE_SENT);

    let res = env
        .send_message(piggy_bank_id, b"smash")
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();

    assert_eq!(res.code, ReplyCode::Success(SuccessReplyReason::Auto));
    assert_eq!(res.value, 0);

    let on_eth_balance = piggy_bank.get_balance().await.unwrap();
    assert_eq!(on_eth_balance, 0);

    let state_hash = piggy_bank.query().state_hash().await.unwrap();
    let local_balance = node.db.program_state(state_hash).unwrap().balance;
    assert_eq!(local_balance, 0);

    let router_address = env.ethereum.router().address();
    let router_balance = env
        .ethereum
        .provider()
        .get_balance(router_address.into())
        .await
        .map(ethexe_ethereum::abi::utils::uint256_to_u128_lossy)
        .unwrap();

    assert_eq!(router_balance, VALUE_SENT);

    let sender_address = env.ethereum.provider().default_signer_address();

    let program_state = node.db.program_state(state_hash).unwrap();
    let mailbox = node
        .db
        .mailbox(program_state.mailbox_hash.to_inner().unwrap())
        .unwrap();
    let user_mailbox = mailbox.into_values(&node.db)[&sender_address.into()].clone();
    let mailboxed_msg_id = user_mailbox.into_keys().next().unwrap();

    piggy_bank
        .send_reply(mailboxed_msg_id, "", 0)
        .await
        .unwrap();

    listener
        .apply_until_block_event(|e| match e {
            BlockEvent::Mirror {
                actor_id,
                event: MirrorEvent::ValueClaimed { claimed_id, .. },
            } if actor_id == piggy_bank_id && claimed_id == mailboxed_msg_id => {
                Ok(ControlFlow::Break(()))
            }
            _ => Ok(ControlFlow::Continue(())),
        })
        .await
        .unwrap();

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
        .send_message(res.program_id, &env.sender_id.encode())
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

    let mut listener = env.observer_events_publisher().subscribe();

    ping.owned_balance_top_up(VALUE_SENT).await.unwrap();

    listener
        .apply_until_block_event(|e| {
            if matches!(e, BlockEvent::Router(RouterEvent::BatchCommitted { .. })) {
                Ok(ControlFlow::Break(()))
            } else {
                Ok(ControlFlow::Continue(()))
            }
        })
        .await
        .unwrap();

    let on_eth_balance = ping.get_balance().await.unwrap();
    assert_eq!(on_eth_balance, VALUE_SENT);

    let state_hash = ping.query().state_hash().await.unwrap();
    let local_balance = node.db.program_state(state_hash).unwrap().balance;
    assert_eq!(local_balance, VALUE_SENT);

    let res = env
        .send_message_with_params(ping_id, b"PING", VALUE_SENT, false)
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
        .send_message(create_program.program_id, b"PING")
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

    log::info!(
        "📗 Create snapshot for block: {}, where ping program is already created",
        env.provider.get_block_number().await.unwrap()
    );
    let program_created_snapshot_id = env.provider.anvil_snapshot().await.unwrap();

    let res = env
        .send_message(ping_id, b"PING")
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
        .send_message(ping_id, b"PING")
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
    let send_message = env.send_message(ping_id, b"PING").await.unwrap();

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
        .send_message(res.program_id, b"PING")
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

    let send_message = env.send_message(ping_id, b"PING").await.unwrap();

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
        .send_message(res.program_id, b"")
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
        .send_message(res.program_id, ping_id.encode().as_slice())
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

    let res = env
        .send_message(async_id, demo_async::Command::Common.encode().as_slice())
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
        .send_message(async_id, demo_async::Command::Common.encode().as_slice())
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
        .send_message(async_id, demo_async::Command::Common.encode().as_slice())
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
async fn send_injected_tx() {
    init_logger();

    let test_env_config = TestEnvConfig {
        validators: ValidatorsConfig::PreDefined(2),
        network: EnvNetworkConfig::Enabled,
        ..Default::default()
    };

    // Setup env of 2 nodes, one of them knows about the other one.
    let mut env = TestEnv::new(test_env_config).await.unwrap();

    let validator0_pubkey = env.validators[0].public_key;
    let validator1_pubkey = env.validators[1].public_key;

    log::info!("📗 Starting node 0");
    let mut node0 = env.new_node(
        NodeConfig::default()
            .validator(env.validators[0])
            .service_rpc(9505),
    );
    node0.start_service().await;

    log::info!("📗 Starting node 1");
    let mut node1 = env.new_node(
        NodeConfig::default()
            .service_rpc(9506)
            .validator(env.validators[1]),
    );
    node1.start_service().await;

    log::info!("Populate node-0 and node-1 with 2 valid blocks");

    env.force_new_block().await;
    env.force_new_block().await;

    // Give some time for nodes to process the blocks
    let reference_block = node0
        .db
        .latest_data()
        .expect("latest data not found")
        .prepared_block_hash;

    // Prepare tx data
    let tx = InjectedTransaction {
        destination: ActorId::from(H160::random()),
        payload: H256::random().0.to_vec().into(),
        value: 0,
        reference_block,
        salt: H256::random().0.to_vec().into(),
    };

    let tx_for_node1 = RpcOrNetworkInjectedTx {
        recipient: validator1_pubkey.to_address(),
        tx: env
            .signer
            .signed_data(validator0_pubkey, tx.clone())
            .unwrap(),
    };

    // Send request
    log::info!("Sending tx pool request to node-1");
    let _r = node1
        .rpc_http_client()
        .unwrap()
        .send_transaction(tx_for_node1.clone())
        .await
        .expect("rpc server is set");

    // Tx executable validation takes time, so wait for event.
    node1
        .listener()
        .wait_for(|event| {
            // TODO kuzmindev: after validators discovery will be done replace to wait for inclusion tx into announce from node1
            if let TestingEvent::Rpc(TestingRpcEvent::InjectedTransaction { transaction }) = event
                && transaction == tx_for_node1
            {
                return Ok(true);
            }
            Ok(false)
        })
        .await
        .unwrap();

    // Check that node-1 save received tx.
    let node1_db_tx = node1
        .db
        .injected_transaction(tx.to_hash())
        .expect("tx not found");
    assert_eq!(node1_db_tx, tx_for_node1.tx);
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
        assert_eq!(alice_latest_data.synced_block, bob_latest_data.synced_block);
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
            .create_program_with_params(code_id, H256([i as u8; 32]), None, 500_000_000_000_000)
            .await
            .unwrap()
            .wait_for()
            .await
            .unwrap();

        *program_id = program_info.program_id;

        let value = i as u64 % 3;
        let _reply_info = env
            .send_message(program_info.program_id, &value.encode())
            .await
            .unwrap()
            .wait_for()
            .await
            .unwrap();
    }

    let latest_block = env.latest_block().await.hash;
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
            .send_message(program_id, &(i as u64).encode())
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

    let latest_block = env.latest_block().await.hash;
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
            .send_message(program_id, &i.encode())
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

    let latest_block = env.latest_block().await.hash;
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

    let latest_block = env.latest_block().await.hash;
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

    let mut listener = env.observer_events_publisher().subscribe();
    listener
        .apply_until_block_event(|event| {
            if matches!(
                event,
                BlockEvent::Router(RouterEvent::ValidatorsCommittedForEra { era_index: _ })
            ) {
                Ok(ControlFlow::Break(()))
            } else {
                Ok(ControlFlow::Continue(()))
            }
        })
        .await
        .unwrap();

    tracing::info!("📗 Next validators successfully committed");

    // Upload code when next validators committed and next are not active.
    // Checks, that another validators commitment not happen.
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

#[tokio::test(flavor = "multi_thread")]
#[ntest::timeout(50_000)]
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

    // Skip blocks to finish quarantine for program creation and balance top-up
    // 0 - ProgramCreated event
    // 1 - ExecutableBalanceTopUpRequested event
    // 2..canonical_quarantine + 2 - quarantine period
    env.skip_blocks(env.compute_config.canonical_quarantine() as u32 + 2)
        .await;

    env.observer_events_publisher()
        .subscribe()
        .apply_until_block_event(|event| {
            if let BlockEvent::Mirror {
                event: MirrorEvent::StateChanged { .. },
                ..
            } = event
            {
                Ok(ControlFlow::Break(()))
            } else {
                Ok(ControlFlow::Continue(()))
            }
        })
        .await
        .unwrap();

    // Wait till validator stops processing for the latest block (where commitment with program creation is present)
    let latest_block: H256 = env.latest_block().await.hash.0.into();
    log::info!("📗 waiting announce for block {latest_block} computed");
    validator
        .listener()
        .wait_for_announce_computed(latest_block)
        .await;

    let validator_db = validator.db.clone();
    let mut listener = validator.listener();
    let canonical_quarantine = env.compute_config.canonical_quarantine();
    let message_id = env
        .send_message(res.program_id, b"PING")
        .await
        .unwrap()
        .message_id;

    let check_for_pong = |block_hash| {
        let block_events = validator_db.block_events(block_hash).unwrap();
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
                return true;
            }
        }

        false
    };

    // 0 - message sent
    // 0..canonical_quarantine - quarantine period
    // canonical_quarantine - Process event
    // canonical_quarantine + 1 - PONG must be present
    for _ in 0..canonical_quarantine {
        let block_hash = listener.wait_for_block_synced().await;

        assert!(!check_for_pong(block_hash), "PONG received too early");

        listener.wait_for_announce_computed(block_hash).await;
        env.force_new_block().await;
    }

    // wait for block synced with PING msg processing
    let _ = listener.wait_for_block_synced().await;

    // wait for block with PONG
    let block_hash = listener.wait_for_block_synced().await;
    assert!(
        check_for_pong(block_hash),
        "PONG not received after quarantine"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ntest::timeout(60_000)]
async fn value_send_program_to_program() {
    // 1_000 ETH
    const VALUE_SENT: u128 = 1_000 * ETHER;

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

    // Send init message to value receiver program (demo_ping)
    let _ = env
        .send_message(res.program_id, &[])
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();

    let value_receiver_id = res.program_id;
    let value_receiver = env
        .ethereum
        .mirror(value_receiver_id.to_address_lossy().into());

    let value_receiver_on_eth_balance = value_receiver.get_balance().await.unwrap();
    assert_eq!(value_receiver_on_eth_balance, 0);

    let value_receiver_state_hash = value_receiver.query().state_hash().await.unwrap();
    let value_receiver_local_balance = node
        .db
        .program_state(value_receiver_state_hash)
        .unwrap()
        .balance;
    assert_eq!(value_receiver_local_balance, 0);

    let res = env
        .upload_code(demo_value_sender_ethexe::WASM_BINARY)
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

    // Send init message to value sender program with value to be sent to value receiver
    let res = env
        .send_message_with_params(
            res.program_id,
            &value_receiver_id.encode(),
            VALUE_SENT,
            false,
        )
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();

    assert_eq!(res.code, ReplyCode::Success(SuccessReplyReason::Auto));
    assert_eq!(res.value, 0);

    let value_sender_id = res.program_id;
    let value_sender = env
        .ethereum
        .mirror(value_sender_id.to_address_lossy().into());

    let value_sender_on_eth_balance = value_sender.get_balance().await.unwrap();
    assert_eq!(value_sender_on_eth_balance, VALUE_SENT);

    let value_sender_state_hash = value_sender.query().state_hash().await.unwrap();
    let value_sender_local_balance = node
        .db
        .program_state(value_sender_state_hash)
        .unwrap()
        .balance;
    assert_eq!(value_sender_local_balance, VALUE_SENT);

    let res = env
        .send_message(value_sender_id, &(0_u64, VALUE_SENT).encode())
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();

    assert_eq!(res.code, ReplyCode::Success(SuccessReplyReason::Auto));
    assert_eq!(res.value, 0);

    let value_sender_on_eth_balance = value_sender.get_balance().await.unwrap();
    assert_eq!(value_sender_on_eth_balance, 0);

    let value_sender_state_hash = value_sender.query().state_hash().await.unwrap();
    let value_sender_local_balance = node
        .db
        .program_state(value_sender_state_hash)
        .unwrap()
        .balance;
    assert_eq!(value_sender_local_balance, 0);

    let value_receiver_on_eth_balance = value_receiver.get_balance().await.unwrap();
    assert_eq!(value_receiver_on_eth_balance, VALUE_SENT);

    let value_receiver_state_hash = value_receiver.query().state_hash().await.unwrap();
    let value_receiver_local_balance = node
        .db
        .program_state(value_receiver_state_hash)
        .unwrap()
        .balance;
    assert_eq!(value_receiver_local_balance, VALUE_SENT);

    // get router balance
    let router_address = env.ethereum.router().address();
    let router_balance = env
        .ethereum
        .provider()
        .get_balance(router_address.into())
        .await
        .map(ethexe_ethereum::abi::utils::uint256_to_u128_lossy)
        .unwrap();

    assert_eq!(router_balance, 0);
}

#[tokio::test(flavor = "multi_thread")]
#[ntest::timeout(60_000)]
async fn value_send_delayed() {
    // 1_000 ETH
    const VALUE_SENT: u128 = 1_000 * ETHER;

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

    // Send init message to value receiver program (demo_ping)
    let _ = env
        .send_message(res.program_id, &[])
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();

    let value_receiver_id = res.program_id;
    let value_receiver = env
        .ethereum
        .mirror(value_receiver_id.to_address_lossy().into());

    let value_receiver_on_eth_balance = value_receiver.get_balance().await.unwrap();
    assert_eq!(value_receiver_on_eth_balance, 0);

    let value_receiver_state_hash = value_receiver.query().state_hash().await.unwrap();
    let value_receiver_local_balance = node
        .db
        .program_state(value_receiver_state_hash)
        .unwrap()
        .balance;
    assert_eq!(value_receiver_local_balance, 0);

    let res = env
        .upload_code(demo_delayed_sender_ethexe::WASM_BINARY)
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

    // Send init message to value sender which sends value to receiver with delay
    let res = env
        .send_message_with_params(
            res.program_id,
            &value_receiver_id.encode(),
            VALUE_SENT,
            false,
        )
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();

    assert_eq!(res.code, ReplyCode::Success(SuccessReplyReason::Auto));
    assert_eq!(res.value, 0);

    let value_sender_id = res.program_id;
    let value_sender = env
        .ethereum
        .mirror(value_sender_id.to_address_lossy().into());

    // Sender should not have the value, because it was just sent to receiver with delay
    let value_sender_on_eth_balance = value_sender.get_balance().await.unwrap();
    assert_eq!(value_sender_on_eth_balance, 0);

    let value_sender_state_hash = value_sender.query().state_hash().await.unwrap();
    let value_sender_local_balance = node
        .db
        .program_state(value_sender_state_hash)
        .unwrap()
        .balance;
    assert_eq!(value_sender_local_balance, 0);

    // Check receiver don't have the value yet
    let value_receiver_on_eth_balance = value_receiver.get_balance().await.unwrap();
    assert_eq!(value_receiver_on_eth_balance, 0);

    let value_receiver_state_hash = value_receiver.query().state_hash().await.unwrap();
    let value_receiver_local_balance = node
        .db
        .program_state(value_receiver_state_hash)
        .unwrap()
        .balance;
    assert_eq!(value_receiver_local_balance, 0);

    // Router should have the value temporarily
    let router_address = env.ethereum.router().address();
    let router_balance = env
        .ethereum
        .provider()
        .get_balance(router_address.into())
        .await
        .map(ethexe_ethereum::abi::utils::uint256_to_u128_lossy)
        .unwrap();

    assert_eq!(router_balance, VALUE_SENT);

    let mut listener = env.observer_events_publisher().subscribe();

    // Skip blocks to pass the delay
    env.provider
        .anvil_mine(Some((demo_delayed_sender_ethexe::DELAY).into()), None)
        .await
        .unwrap();
    listener
        .apply_until_block_event(|e| {
            if matches!(e, BlockEvent::Router(RouterEvent::BatchCommitted { .. })) {
                Ok(ControlFlow::Break(()))
            } else {
                Ok(ControlFlow::Continue(()))
            }
        })
        .await
        .unwrap();

    // Receiver should have the value now
    let value_receiver_on_eth_balance = value_receiver.get_balance().await.unwrap();
    assert_eq!(value_receiver_on_eth_balance, VALUE_SENT);

    let value_receiver_state_hash = value_receiver.query().state_hash().await.unwrap();
    let value_receiver_local_balance = node
        .db
        .program_state(value_receiver_state_hash)
        .unwrap()
        .balance;
    assert_eq!(value_receiver_local_balance, VALUE_SENT);

    // Sender still don't have the value
    let value_sender_on_eth_balance = value_sender.get_balance().await.unwrap();
    assert_eq!(value_sender_on_eth_balance, 0);

    let value_sender_state_hash = value_sender.query().state_hash().await.unwrap();
    let value_sender_local_balance = node
        .db
        .program_state(value_sender_state_hash)
        .unwrap()
        .balance;
    assert_eq!(value_sender_local_balance, 0);

    // get router balance
    let router_balance = env
        .ethereum
        .provider()
        .get_balance(router_address.into())
        .await
        .map(ethexe_ethereum::abi::utils::uint256_to_u128_lossy)
        .unwrap();

    assert_eq!(router_balance, 0);
}

#[tokio::test(flavor = "multi_thread")]
#[ntest::timeout(60_000)]
async fn injected_tx_fungible_token() {
    init_logger();

    let env_config = TestEnvConfig {
        network: EnvNetworkConfig::Enabled,
        compute_config: ComputeConfig::without_quarantine(),
        ..Default::default()
    };

    let mut env = TestEnv::new(env_config).await.unwrap();

    let pubkey = env.validators[0].public_key;
    let mut node = env.new_node(
        NodeConfig::default()
            .service_rpc(8090)
            .validator(env.validators[0]),
    );
    node.start_service().await;
    let rpc_client = node.rpc_http_client().expect("RPC client provide by node");

    // 1. Create Fungible token config
    let token_config = demo_fungible_token::InitConfig {
        name: "USD Tether".to_string(),
        symbol: "USDT".to_string(),
        decimals: 10,
        initial_capacity: None,
    };

    // 2. Uploading code and creating program
    let res = env
        .upload_code(demo_fungible_token::WASM_BINARY)
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

    let usdt_actor_id = res.program_id;

    // 3. Initialize program
    let init_reply = env
        .send_message(usdt_actor_id, &token_config.encode())
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();

    assert_eq!(init_reply.program_id, usdt_actor_id);
    assert_eq!(init_reply.value, 0);
    assert_eq!(
        init_reply.code,
        ReplyCode::Success(SuccessReplyReason::Auto)
    );
    assert!(
        init_reply.payload.is_empty(),
        "Expect empty payload, because of initializing Fungible Token returns nothing"
    );

    tracing::info!("✅ Fungible token successfully initialized");

    // 4. Try ming some tokens
    let amount: u128 = 5_000_000_000;
    let mint_action = demo_fungible_token::FTAction::Mint(amount);

    let mint_tx = InjectedTransaction {
        destination: usdt_actor_id,
        payload: mint_action.encode().into(),
        value: 0,
        reference_block: node.db.latest_data().unwrap().prepared_block_hash,
        salt: vec![1u8].into(),
    };

    let rpc_tx = RpcOrNetworkInjectedTx {
        recipient: pubkey.to_address(),
        tx: env.signer.signed_data(pubkey, mint_tx.clone()).unwrap(),
    };

    let acceptance = rpc_client
        .send_transaction(rpc_tx)
        .await
        .expect("successfully send transaction to RPC");
    assert!(matches!(acceptance, InjectedTransactionAcceptance::Accept));

    let expected_event = demo_fungible_token::FTEvent::Transfer {
        from: ActorId::new([0u8; 32]),
        to: pubkey.to_address().into(),
        amount,
    };

    // Listen for inclusion and check the expected payload.
    node.listener()
        .apply_until(|event| {
            if let TestingEvent::Consensus(ConsensusEvent::Promise(promise)) = event {
                let promise = promise.into_data();
                assert_eq!(promise.reply.payload, expected_event.encode());
                assert_eq!(
                    promise.reply.code,
                    ReplyCode::Success(SuccessReplyReason::Manual)
                );
                assert_eq!(promise.reply.value, 0);

                return Ok(ControlFlow::Break(()));
            }

            Ok(ControlFlow::Continue(()))
        })
        .await
        .unwrap();
    tracing::info!("✅ Tokens mint successfully");

    let db = node.db.clone();
    node.listener()
        .apply_until(|event| {
            if let TestingEvent::Observer(ObserverEvent::BlockSynced(synced_block)) = event {
                let Some(block_events) = db.block_events(synced_block) else {
                    return Ok(ControlFlow::Continue(()));
                };

                for block_event in block_events {
                    if let BlockEvent::Mirror {
                        actor_id,
                        event: MirrorEvent::StateChanged { state_hash },
                    } = block_event
                        && actor_id == mint_tx.destination
                    {
                        let state = db.program_state(state_hash).expect("state should be exist");
                        assert_eq!(state.balance, 0);
                        assert_eq!(state.injected_queue.cached_queue_size, 0);
                        assert_eq!(state.canonical_queue.cached_queue_size, 0);
                        return Ok(ControlFlow::Break(()));
                    }
                }
            }

            Ok(ControlFlow::Continue(()))
        })
        .await
        .unwrap();
    tracing::info!("✅ State successfully changed on Ethereum");

    // 5. Transfer some token and wait for promise.
    let random_actor = ActorId::new(H256::random().0);
    let transfer_amount = 100_000;
    let transfer_action = demo_fungible_token::FTAction::Transfer {
        from: pubkey.to_address().into(),
        to: random_actor,
        amount: transfer_amount,
    };
    let transfer_tx = InjectedTransaction {
        destination: usdt_actor_id,
        payload: transfer_action.encode().into(),
        value: 0,
        reference_block: node.db.latest_data().unwrap().prepared_block_hash,
        salt: vec![1u8, 2u8, 3u8].into(),
    };

    let rpc_tx = RpcOrNetworkInjectedTx {
        recipient: pubkey.to_address(),
        tx: env.signer.signed_data(pubkey, transfer_tx.clone()).unwrap(),
    };
    let ws_client = node
        .rpc_ws_client()
        .await
        .expect("RPC WS client provide by node");

    let mut subscription = ws_client
        .send_transaction_and_watch(rpc_tx)
        .await
        .expect("successfully subscribe for transaction promise");

    let promise = subscription
        .next()
        .await
        .expect("promise from subscription")
        .expect("transaction promise")
        .into_data();

    assert_eq!(promise.tx_hash, transfer_tx.to_hash());

    let expected_payload = demo_fungible_token::FTEvent::Transfer {
        from: pubkey.to_address().into(),
        to: random_actor,
        amount: transfer_amount,
    };
    assert_eq!(promise.reply.payload, expected_payload.encode());
    assert_eq!(promise.reply.value, 0);

    // Check unsubscribe from subscription
    subscription
        .unsubscribe()
        .await
        .expect("successfully unsubscribe for promise");

    tracing::info!("✅ Promise successfully received from RPC subscription");
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

    env.send_message(ping_id, b"")
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

        env.send_message(ping_id, b"PING")
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

        let wait_for_pong = env.send_message(ping_id, b"PING").await.unwrap();

        let block = env.latest_block().await;
        let timelines = env.db.protocol_timelines().unwrap();
        let era_index = timelines.era_from_ts(block.header.timestamp);
        let announce = Announce::with_default_gas(block.hash, HashOf::random());
        let announce_hash = announce.to_hash();
        validator0
            .publish_validator_message(ValidatorMessage {
                era_index,
                payload: announce,
            })
            .await;

        // Validators 1..=6 must reject this announce
        futures::future::join_all(listeners.iter_mut().map(|l| {
            l.apply_until(|event| {
                if matches!(
                    event,
                    TestingEvent::Consensus(ConsensusEvent::AnnounceRejected(rejected_announce_hash))
                        if rejected_announce_hash == announce_hash
                ) {
                    Ok(ControlFlow::Break(()))
                } else {
                    Ok(ControlFlow::Continue(()))
                }
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

        let _ = env.send_message(ping_id, b"PING").await.unwrap();

        // Next block producer is validator 0 - because validators 3, 4, 5 were skipped and 6 is current
        assert_eq!(env.next_block_producer_index().await, 0);

        // Send announce from stopped validator 6
        let block = env.latest_block().await;
        let timelines = env.db.protocol_timelines().unwrap();
        let era_index = timelines.era_from_ts(block.header.timestamp);
        let announce6 = Announce::with_default_gas(block.hash, latest_computed_announce_hash);
        let announce6_hash = announce6.to_hash();
        validator6
            .publish_validator_message(ValidatorMessage {
                era_index,
                payload: announce6,
            })
            .await;
        for listener in &mut listeners {
            listener.wait_for_announce_computed(announce6_hash).await;
        }

        // Commitment does not sent by validator 6,
        // so now next producer is the next in order - validator 0
        assert_eq!(env.next_block_producer_index().await, 0);

        let wait_for_pong = env.send_message(ping_id, b"PING").await.unwrap();

        // Ignore announce6 and build announce7 on top of base announce from parent block
        // Announce is not on top of announce6 (already accepted),
        // so must be rejected by validators 1..=5
        let block = env.latest_block().await;
        let timelines = env.db.protocol_timelines().unwrap();
        let era_index = timelines.era_from_ts(block.header.timestamp);
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
                era_index,
                payload: announce7,
            })
            .await;

        // Validators 1..=5 must accept this announce, as soon as parent is known base announce
        futures::future::join_all(listeners.iter_mut().map(|l| {
            l.apply_until(|event| {
                if matches!(
                    event,
                    TestingEvent::Consensus(ConsensusEvent::AnnounceAccepted(announce_hash))
                        if announce_hash == announce7_hash
                ) {
                    Ok(ControlFlow::Break(()))
                } else {
                    Ok(ControlFlow::Continue(()))
                }
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
            "📗 Case 5: validator 0 does not commit changes, because it's stopped, so validator 1 could do this in the next block"
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

#[tokio::test(flavor = "multi_thread")]
#[ntest::timeout(120_000)]
async fn whole_network_restore() {
    init_logger();

    let config = TestEnvConfig {
        validators: ValidatorsConfig::PreDefined(4),
        network: EnvNetworkConfig::Enabled,
        continuous_block_generation: true,
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

    // make sure we receive unique messages and not repeated ones
    let mut seen_messages = HashSet::new();

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
    let ping_id = res.program_id;

    let init_res = env
        .send_message(res.program_id, b"")
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();
    assert_eq!(res.code_id, ping_code_id);
    assert_eq!(init_res.payload, b"");
    assert_eq!(init_res.value, 0);
    assert_eq!(init_res.code, ReplyCode::Success(SuccessReplyReason::Auto));
    assert!(seen_messages.insert(init_res.message_id));

    for (i, v) in validators.iter_mut().enumerate() {
        log::info!("📗 Stopping validator-{i}");
        v.stop_service().await;
    }

    let ping_wait_for = env.send_message(ping_id, b"PING").await.unwrap();

    let async_code_upload = env.upload_code(demo_async::WASM_BINARY).await.unwrap();

    log::info!("📗 Skipping 20 blocks");
    env.skip_blocks(20).await;

    for (i, v) in validators.iter_mut().enumerate() {
        log::info!("📗 Starting validator-{i} again");
        v.start_service().await;
    }

    let res = ping_wait_for.wait_for().await.unwrap();
    assert_eq!(res.code, ReplyCode::Success(SuccessReplyReason::Manual));
    assert_eq!(res.payload, b"PONG");
    assert_eq!(res.value, 0);
    assert!(seen_messages.insert(res.message_id));

    let res = async_code_upload.wait_for().await.unwrap();
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
        .send_message(res.program_id, ping_id.encode().as_slice())
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();
    assert_eq!(res.code_id, async_code_id);
    assert_eq!(init_res.payload, b"");
    assert_eq!(init_res.value, 0);
    assert_eq!(init_res.code, ReplyCode::Success(SuccessReplyReason::Auto));
    assert!(seen_messages.insert(init_res.message_id));
}

#[tokio::test(flavor = "multi_thread")]
#[ntest::timeout(60_000)]
async fn catch_up_3() {
    catch_up_test_case(3).await;
}

#[tokio::test(flavor = "multi_thread")]
#[ntest::timeout(60_000)]
async fn catch_up_5() {
    catch_up_test_case(5).await;
}

async fn catch_up_test_case(commitment_delay_limit: u32) {
    init_logger();

    assert!(
        commitment_delay_limit == 3 || commitment_delay_limit == 5,
        "Only 3 or 5 commitment delay limit is supported for catch-up test"
    );

    #[derive(Clone)]
    struct LateCommitter {
        router: Router,
        commit_signal_receiver: Arc<Mutex<UnboundedReceiver<()>>>,
        wait_signal_sender: Arc<UnboundedSender<()>>,
    }

    #[async_trait::async_trait]
    impl BatchCommitter for LateCommitter {
        fn clone_boxed(&self) -> Box<dyn BatchCommitter> {
            Box::new(self.clone())
        }

        async fn commit(
            mut self: Box<Self>,
            batch: BatchCommitment,
            signatures: Vec<ContractSignature>,
        ) -> anyhow::Result<H256> {
            log::info!("📗 LateCommitter wait for signal to commit ...");
            self.wait_signal_sender.send(()).unwrap();
            self.commit_signal_receiver
                .lock()
                .await
                .recv()
                .await
                .unwrap();

            log::info!(
                "📗 LateCommitter committing batch {}: {:?}",
                batch.to_digest(),
                batch
            );
            let pending = self.router.commit_batch_pending(batch, signatures).await;

            // Notify that commitment is sent
            self.wait_signal_sender.send(()).unwrap();

            log::info!("📗 LateCommitter waiting for transaction to be applied ...");
            pending?
                .try_get_receipt_check_reverted()
                .await
                .map(|r| r.transaction_hash.0.into())
        }
    }

    let config = TestEnvConfig {
        network: EnvNetworkConfig::Enabled,
        commitment_delay_limit,
        ..Default::default()
    };
    let mut env = TestEnv::new(config).await.unwrap();

    log::info!("📗 Starting Alice");
    let mut alice = env.new_node(NodeConfig::named("Alice").validator(env.validators[0]));
    alice.start_service().await;

    log::info!("📗 Starting Bob");
    let mut bob = env.new_node(NodeConfig::named("Bob"));
    bob.start_service().await;

    let ping_code_id = env
        .upload_code(demo_ping::WASM_BINARY)
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap()
        .code_id;

    let ping_id = env
        .create_program(ping_code_id, 500_000_000_000_000)
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap()
        .program_id;

    // Wait until both stops processing
    let latest_block = env.latest_block().await.hash;
    let latest_announce_hash = bob
        .listener()
        .wait_for_announce_computed(latest_block)
        .await;
    assert_eq!(
        alice
            .listener()
            .wait_for_announce_computed(latest_block)
            .await,
        latest_announce_hash
    );

    log::info!("📗 Stopping Bob");
    bob.stop_service().await;

    log::info!("📗 Sending first PING message, so that Alice will leave Bob behind");
    env.send_message(ping_id, b"PING")
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();

    // Wait until Alice stop processing
    let latest_block = env.latest_block().await.hash;
    alice
        .listener()
        .wait_for_announce_computed(latest_block)
        .await;

    log::info!("📗 Stopping Alice");
    alice.stop_service().await;

    log::info!("📗 Setting LateCommitter for Alice and starting Alice again");
    let (commit_signal_sender, commit_signal_receiver) = mpsc::unbounded_channel();
    let (wait_signal_sender, mut wait_signal_receiver) = mpsc::unbounded_channel();
    alice.custom_committer = Some(Box::new(LateCommitter {
        router: env.ethereum.router().clone(),
        commit_signal_receiver: Arc::new(Mutex::new(commit_signal_receiver)),
        wait_signal_sender: Arc::new(wait_signal_sender),
    }));
    alice.start_service().await;

    log::info!("📗 Starting Bob");
    bob.start_service().await;

    log::info!("📗 Disable auto mining");
    env.provider.anvil_set_auto_mine(false).await.unwrap();

    log::info!("📗 Sending second PING message, Bob tries to catch up Alice");
    {
        let listener = env.observer_events_publisher().subscribe();
        let pending = env
            .ethereum
            .mirror(ping_id.try_into().unwrap())
            .send_message_pending(b"PING", 0, false)
            .await
            .unwrap();
        env.force_new_block().await;
        let wait_for = WaitForReplyTo::from_raw_parts(
            listener,
            pending.try_get_message_send_receipt().await.unwrap().1,
        );

        // Waiting until Alice is ready for commitment1
        wait_signal_receiver.recv().await.unwrap();

        // Force new block, so that commitment1 would skip this block
        env.force_new_block().await;

        // Send signal to make commitment1 and wait until it's sent
        commit_signal_sender.send(()).unwrap();
        wait_signal_receiver.recv().await.unwrap();

        // Wait until Alice is ready for next commitment2
        wait_signal_receiver.recv().await.unwrap();

        // Force new block to commit commitment1
        env.force_new_block().await;

        // Send signal to make commitment2,
        // but commitment would not be applied because it's not above previous one
        commit_signal_sender.send(()).unwrap();
        wait_signal_receiver.recv().await.unwrap();

        // Now commitment1 must be applied in the forced block
        wait_for.wait_for().await.unwrap();
    }

    log::info!("📗 Waiting for two rejected announces from Bob");
    for _ in 0..2 {
        bob.listener()
            .wait_for_announce_rejected(AnnounceId::Any)
            .await;
    }

    log::info!("📗 Sending third PING message, one more attempt for Bob to catch up Alice");
    {
        let listener = env.observer_events_publisher().subscribe();
        let pending = env
            .ethereum
            .mirror(ping_id.try_into().unwrap())
            .send_message_pending(b"PING", 0, false)
            .await
            .unwrap();
        env.force_new_block().await;
        let wait_for = WaitForReplyTo::from_raw_parts(
            listener,
            pending.try_get_message_send_receipt().await.unwrap().1,
        );

        // Waiting until Alice is ready for commitment1
        wait_signal_receiver.recv().await.unwrap();

        // Force new block, so that commitment1 would skip this block
        env.force_new_block().await;

        // Send signal to make commitment1 and wait until it's sent
        commit_signal_sender.send(()).unwrap();
        wait_signal_receiver.recv().await.unwrap();

        // Wait until Alice is ready for next commitment2
        wait_signal_receiver.recv().await.unwrap();

        // Force new block to commit commitment1
        // if commitment_delay_limit == 3 => commitment1 would fail because contains expired announces
        // if commitment_delay_limit == 5 => commitment1 would succeed
        env.force_new_block().await;

        if commitment_delay_limit == 3 {
            // Waiting until Alice is ready for commitment2
            wait_signal_receiver.recv().await.unwrap();

            // Send signal to make commitment2 and wait until it's sent
            commit_signal_sender.send(()).unwrap();
            wait_signal_receiver.recv().await.unwrap();

            // Force new block to commit commitment2, succeed
            env.force_new_block().await;
        } else if commitment_delay_limit == 5 {
            // commitment1 already committed, so Alice would not commit commitment2, because it's empty
        } else {
            unreachable!();
        }

        // Now commitment1 or commitment2 must be applied in the forced blocks
        wait_for.wait_for().await.unwrap();
    }

    let latest_block = env.latest_block().await.hash;
    let latest_announce_hash = alice
        .listener()
        .wait_for_announce_computed(latest_block)
        .await;

    if commitment_delay_limit == 3 {
        log::info!("📗 Bob accepts announce from Alice at last");
        bob.listener()
            .wait_for_announce_accepted(latest_announce_hash)
            .await;
    } else if commitment_delay_limit == 5 {
        log::info!("📗 Bob still rejects announce from Alice");
        bob.listener()
            .wait_for_announce_rejected(latest_announce_hash)
            .await;
    } else {
        unreachable!();
    }
}
