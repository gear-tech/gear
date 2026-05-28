// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Integration tests.

pub(crate) mod utils;

use crate::tests::utils::{
    EnvNetworkConfig, InfiniteStreamExt, NodeConfig, TestEnv, TestEnvConfig, TestingEvent,
    TestingNetworkEvent, TestingRpcEvent, ValidatorsConfig, init_logger, stop_nodes, test_info,
};
use alloy::{
    primitives::U256,
    providers::{Provider as _, WalletProvider, ext::AnvilApi},
};
use ethexe_common::{
    db::{CodesStorageRO, GlobalsStorageRO, InjectedStorageRO, MbStorageRO, OnChainStorageRO},
    ecdsa::ContractSignature,
    events::{
        BlockEvent, MirrorEvent,
        mirror::{MessageEvent, ReplyEvent},
    },
    gear::BatchCommitment,
    injected::{
        AddressedInjectedTransaction, InjectedTransaction, InjectedTransactionAcceptance, Receipt,
        TransactionPurgedReason,
    },
    mock::*,
};
use ethexe_consensus::BatchCommitter;
use ethexe_ethereum::{EthereumBuilder, TryGetReceipt, router::Router};
use ethexe_rpc::InjectedClient;
use ethexe_runtime_common::state::Storage;
use gear_core::{
    ids::prelude::MessageIdExt,
    message::{ReplyCode, SuccessReplyReason},
};
use gear_core_errors::{ErrorReplyReason, SimpleExecutionError, SimpleUnavailableActorError};
use gprimitives::{ActorId, H160, H256, MessageId};
use gsigner::secp256k1::{Secp256k1SignerExt, Signer};
use parity_scale_codec::{Decode, Encode};
use std::{collections::HashSet, sync::Arc, time::Duration};
use tokio::sync::Mutex;

const ETHER: u128 = 1_000_000_000_000_000_000;

#[derive(Clone)]
struct RecordingCommitter {
    router: Router,
    committed_batches: Arc<Mutex<Vec<BatchCommitment>>>,
}

#[async_trait::async_trait]
impl BatchCommitter for RecordingCommitter {
    fn clone_boxed(&self) -> Box<dyn BatchCommitter> {
        Box::new(self.clone())
    }

    async fn commit(
        self: Box<Self>,
        batch: BatchCommitment,
        signatures: Vec<ContractSignature>,
    ) -> anyhow::Result<H256> {
        self.committed_batches.lock().await.push(batch.clone());
        Box::new(self.router.clone())
            .commit(batch, signatures)
            .await
    }
}

#[tokio::test]
#[ntest::timeout(30_000)]
async fn invalid_code() {
    init_logger();

    let mut env = TestEnv::default().await;

    let mut node = env
        .new_node(NodeConfig::default().validator(env.validators[0]))
        .await;
    node.start_service().await;

    let wasm_binary = [1; 10]; // Invalid WASM binary
    let res = env
        .upload_code(&wasm_binary)
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();
    assert!(!res.valid);

    // Graceful shutdown so the malachite engine releases its
    // RocksDB lock + libp2p listener — without this nextest's leak
    // detector flags the test as leaky on fast paths.
    stop_nodes([node]).await;
}

#[tokio::test]
#[ntest::timeout(60_000)]
async fn write_memory_to_last_byte() {
    init_logger();

    let mut env = TestEnv::default().await;

    let mut node = env
        .new_node(NodeConfig::default().validator(env.validators[0]))
        .await;
    node.start_service().await;

    let wat = r#"
(module
    (import "env" "memory" (memory 32768))
    (export "init" (func $init))
    (func $init
        (i32.store8
            (i32.const 2147483647)
            (i32.const 0xff)
        )
    )
)"#;
    let wasm_binary = wat::parse_str(wat).expect("failed to parse module");
    let res = env
        .upload_code(&wasm_binary)
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
    assert_eq!(code, wasm_binary);

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
        .send_message(res.program_id, &[])
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();

    assert_eq!(res.code, ReplyCode::Success(SuccessReplyReason::Auto));
    assert!(res.payload.is_empty());
    assert_eq!(res.value, 0);

    stop_nodes([node]).await;
}

#[tokio::test]
#[ntest::timeout(60_000)]
async fn ping() {
    init_logger();

    let mut env = TestEnv::default().await;

    let mut node = env
        .new_node(NodeConfig::default().validator(env.validators[0]))
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

    stop_nodes([node]).await;
}

/// Minimal multi-validator smoke: 3 validators, single ping round-trip.
#[tokio::test]
#[ntest::timeout(60_000)]
async fn multiple_validators_ping() {
    init_logger();

    let config = TestEnvConfig {
        validators: ValidatorsConfig::PreDefined(3),
        network: EnvNetworkConfig::Enabled,
        ..Default::default()
    };
    let mut env = TestEnv::new(config).await.unwrap();

    let mut validators = vec![];
    for (i, v) in env.validators.clone().into_iter().enumerate() {
        test_info!("📗 Starting validator-{i}");
        let mut validator = env
            .new_node(NodeConfig::named(format!("validator-{i}")).validator(v))
            .await;
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
    let ping_id = res.program_id;

    let res = env
        .send_message(ping_id, b"PING")
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();
    assert_eq!(res.code, ReplyCode::Success(SuccessReplyReason::Manual));
    assert_eq!(res.payload, b"PONG");

    stop_nodes(validators).await;
}

/// Init-failure paths: panic-in-init then synchronous handle to the
/// uninitialized program (UnavailableActor::InitializationFailure), and
/// async-init handshake with three approval messages then a final reply.
#[tokio::test]
#[ntest::timeout(60_000)]
async fn uninitialized_program() {
    init_logger();

    let mut env = TestEnv::default().await;

    let mut node = env
        .new_node(NodeConfig::default().validator(env.validators[0]))
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

    test_info!("Case #1: Init failed due to panic in init (decoding)");
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

    test_info!("Case #2: async init, replies are acceptable.");
    {
        let init_payload = demo_async_init::InputArgs {
            approver_first: env.sender_id,
            approver_second: env.sender_id,
            approver_third: env.sender_id,
        }
        .encode();

        let receiver = env.new_observer_events();

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
        let mirror = env.ethereum.mirror(init_res.program_id);

        let mut messages_stream = receiver.clone().filter_map_block_synced();
        let mut msgs_for_reply = Vec::new();
        for _ in 0..3 {
            let msg_id = messages_stream
                .find_map(|event| match event {
                    BlockEvent::Mirror {
                        actor_id,
                        event:
                            MirrorEvent::Message(MessageEvent {
                                id, destination, ..
                            }),
                    } if actor_id == init_res.program_id && destination == env.sender_id => {
                        Some(id)
                    }
                    _ => None,
                })
                .await;
            msgs_for_reply.push(msg_id);
        }

        test_info!("Handle message to uninitialized program.");
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

        test_info!("Success end of initialization.");
        init_reply.wait_for().await.unwrap().tap(|reply_info| {
            assert!(reply_info.code.is_success());
        });

        test_info!("Handle message handled, but panicked due to incorrect payload as expected.");
        let res = env
            .send_message(init_res.program_id, &[])
            .await
            .unwrap()
            .wait_for()
            .await
            .unwrap();

        let expected_err = ReplyCode::Error(SimpleExecutionError::UserspacePanic.into());
        assert_eq!(res.code, expected_err);
    }

    stop_nodes([node]).await;
}

/// Mailbox round-trip with demo_async: Mutex command writes the original mid
/// and a PING into the mailbox, sender replies, value gets claimed.
#[tokio::test]
#[ntest::timeout(60_000)]
async fn mailbox() {
    init_logger();

    let mut env = TestEnv::default().await;

    let mut node = env
        .new_node(NodeConfig::default().validator(env.validators[0]))
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

    let receiver = env.new_observer_events();

    let wait_for_mutex_request_command_reply = env
        .send_message(async_pid, &demo_async::Command::Mutex.encode())
        .await
        .unwrap();

    let original_mid = wait_for_mutex_request_command_reply.message_id;
    let mid_expected_message_id = MessageId::generate_outgoing(original_mid, 0);
    let ping_expected_message_id = MessageId::generate_outgoing(original_mid, 1);

    test_info!("📗 Waiting for MB with PING message committed");
    let (mut block, mut mb_hash_opt) = (None, None);
    receiver
        .clone()
        .filter_map_block_synced_with_header()
        .find(|(event, block_data)| match event {
            BlockEvent::Mirror {
                actor_id,
                event:
                    MirrorEvent::Message(MessageEvent {
                        id,
                        destination,
                        payload,
                        ..
                    }),
            } if *actor_id == async_pid => {
                assert_eq!(*destination, env.sender_id);

                if *id == mid_expected_message_id {
                    assert_eq!(*payload, original_mid.encode());
                } else if *id == ping_expected_message_id {
                    assert_eq!(*payload, b"PING");
                    block = Some(*block_data);
                } else {
                    panic!("Unexpected message id {id}");
                }

                false
            }
            BlockEvent::Router(ethexe_common::events::RouterEvent::MBCommitted(ah))
                if block.is_some() =>
            {
                mb_hash_opt = Some(ah.clone());
                true
            }
            _ => false,
        })
        .await;

    let block = block.expect("must be set");
    let ethexe_common::events::router::MBCommittedEvent(mb_hash) =
        mb_hash_opt.expect("must be set");

    // In MB-driven flow the synthetic block height that the executor sees
    // is `last_advanced_eth_block.height`, which is one Eth block behind
    // the block that emitted the Mirror events (advance-then-event chain
    // adds one block of distance). Schedule expiries are computed against
    // that synthetic height.
    let wake_expiry = block.header.height - 2 + 100;
    let expiry = block.header.height - 2 + ethexe_runtime_common::state::MAILBOX_VALIDITY;

    let expected_schedule = std::collections::BTreeMap::from_iter([
        (
            wake_expiry,
            std::collections::BTreeSet::from_iter([ethexe_common::ScheduledTask::WakeMessage(
                async_pid,
                original_mid,
            )]),
        ),
        (
            expiry,
            std::collections::BTreeSet::from_iter([
                ethexe_common::ScheduledTask::RemoveFromMailbox(
                    (async_pid, env.sender_id),
                    mid_expected_message_id,
                ),
                ethexe_common::ScheduledTask::RemoveFromMailbox(
                    (async_pid, env.sender_id),
                    ping_expected_message_id,
                ),
            ]),
        ),
    ]);

    let schedule = node
        .db
        .mb_schedule(mb_hash)
        .expect("MB schedule must exist");
    assert_eq!(schedule, expected_schedule);

    let mid_payload = ethexe_runtime_common::state::PayloadLookup::Direct(
        original_mid.into_bytes().to_vec().try_into().unwrap(),
    );
    let ping_payload =
        ethexe_runtime_common::state::PayloadLookup::Direct(b"PING".to_vec().try_into().unwrap());

    let expected_mailbox = std::collections::BTreeMap::from_iter([(
        env.sender_id,
        std::collections::BTreeMap::from_iter([
            (
                mid_expected_message_id,
                ethexe_runtime_common::state::Expiring {
                    value: ethexe_runtime_common::state::MailboxMessage {
                        payload: mid_payload.clone(),
                        value: 0,
                        message_type: ethexe_common::gear::MessageType::Canonical,
                    },
                    expiry,
                },
            ),
            (
                ping_expected_message_id,
                ethexe_runtime_common::state::Expiring {
                    value: ethexe_runtime_common::state::MailboxMessage {
                        payload: ping_payload,
                        value: 0,
                        message_type: ethexe_common::gear::MessageType::Canonical,
                    },
                    expiry,
                },
            ),
        ]),
    )]);

    let mirror = env.ethereum.mirror(async_pid);
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

    let expected_mailbox = std::collections::BTreeMap::from_iter([(
        env.sender_id,
        std::collections::BTreeMap::from_iter([(
            mid_expected_message_id,
            ethexe_runtime_common::state::Expiring {
                value: ethexe_runtime_common::state::MailboxMessage {
                    payload: mid_payload,
                    value: 0,
                    message_type: ethexe_common::gear::MessageType::Canonical,
                },
                expiry,
            },
        )]),
    )]);

    assert_eq!(mailbox.into_values(&node.db), expected_mailbox);

    test_info!("📗 Claiming value for message {mid_expected_message_id}");
    mirror.claim_value(mid_expected_message_id).await.unwrap();

    let mut claimed = false;
    let mb_hash = receiver
        .filter_map_block_synced()
        .find_map(|event| match event {
            BlockEvent::Mirror {
                actor_id,
                event:
                    MirrorEvent::ValueClaimed(ethexe_common::events::mirror::ValueClaimedEvent {
                        claimed_id,
                        ..
                    }),
            } if actor_id == async_pid && claimed_id == mid_expected_message_id => {
                claimed = true;
                None
            }
            BlockEvent::Router(ethexe_common::events::RouterEvent::MBCommitted(
                ethexe_common::events::router::MBCommittedEvent(ah),
            )) if claimed => Some(ah),
            _ => None,
        })
        .await;
    assert!(claimed, "Value must be claimed");

    let state_hash = mirror.query().state_hash().await.unwrap();
    let state = node.db.program_state(state_hash).unwrap();
    assert!(state.mailbox_hash.is_empty());

    let schedule = node
        .db
        .mb_schedule(mb_hash)
        .expect("MB schedule must exist");
    assert!(schedule.is_empty(), "{schedule:?}");

    stop_nodes([node]).await;
}

#[tokio::test]
#[ntest::timeout(120_000)]
async fn value_reply_program_to_user() {
    init_logger();

    let mut env = TestEnv::default().await;

    let mut node = env
        .new_node(NodeConfig::default().validator(env.validators[0]))
        .await;
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

    let on_eth_balance = piggy_bank.query().balance().await.unwrap();
    assert_eq!(on_eth_balance, 0);

    let state_hash = piggy_bank.query().state_hash().await.unwrap();
    let local_balance = node.db.program_state(state_hash).unwrap().balance;
    assert_eq!(local_balance, 0);

    // 1_000 ETH
    const VALUE_SENT: u128 = 1_000 * ETHER;

    piggy_bank.owned_balance_top_up(VALUE_SENT).await.unwrap();

    // Force the validator to advance past the top-up Eth event by
    // sending a no-op `b""` message and waiting for its reply. By
    // the time the reply lands, the deposit has been folded into a
    // finalised MB and committed on-chain.
    let res = env
        .send_message(piggy_bank_id, b"")
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();
    assert_eq!(res.code, ReplyCode::Success(SuccessReplyReason::Auto));

    let on_eth_balance = piggy_bank.query().balance().await.unwrap();
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

    let on_eth_balance = piggy_bank.query().balance().await.unwrap();
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

    stop_nodes([node]).await;
}

#[tokio::test]
#[ntest::timeout(120_000)]
async fn value_send_program_to_user_and_claimed() {
    init_logger();

    let mut env = TestEnv::default().await;

    let mut node = env
        .new_node(NodeConfig::default().validator(env.validators[0]))
        .await;
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

    let on_eth_balance = piggy_bank.query().balance().await.unwrap();
    assert_eq!(on_eth_balance, 0);

    let state_hash = piggy_bank.query().state_hash().await.unwrap();
    let local_balance = node.db.program_state(state_hash).unwrap().balance;
    assert_eq!(local_balance, 0);

    // 1_000 ETH
    const VALUE_SENT: u128 = 1_000 * ETHER;

    piggy_bank.owned_balance_top_up(VALUE_SENT).await.unwrap();

    // Force the validator to fold the deposit into a finalised
    // MB by sending a no-op message and waiting for the reply.
    let res = env
        .send_message(piggy_bank_id, b"")
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();
    assert_eq!(res.code, ReplyCode::Success(SuccessReplyReason::Auto));

    let on_eth_balance = piggy_bank.query().balance().await.unwrap();
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

    let on_eth_balance = piggy_bank.query().balance().await.unwrap();
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

    // Force-process the claim by sending a follow-up no-op message
    // through the program. Once its reply lands, the claim has been
    // executed in the executor and committed to the mirror.
    let _ = env
        .send_message(piggy_bank_id, b"")
        .await
        .unwrap()
        .wait_for()
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

    stop_nodes([node]).await;
}

#[tokio::test]
#[ntest::timeout(120_000)]
async fn value_send_program_to_user_and_replied() {
    init_logger();

    let mut env = TestEnv::default().await;

    let mut node = env
        .new_node(NodeConfig::default().validator(env.validators[0]))
        .await;
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

    let on_eth_balance = piggy_bank.query().balance().await.unwrap();
    assert_eq!(on_eth_balance, 0);

    let state_hash = piggy_bank.query().state_hash().await.unwrap();
    let local_balance = node.db.program_state(state_hash).unwrap().balance;
    assert_eq!(local_balance, 0);

    // 1_000 ETH
    const VALUE_SENT: u128 = 1_000 * ETHER;

    piggy_bank.owned_balance_top_up(VALUE_SENT).await.unwrap();

    // Force-fold the deposit into the next finalised MB.
    let res = env
        .send_message(piggy_bank_id, b"")
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();
    assert_eq!(res.code, ReplyCode::Success(SuccessReplyReason::Auto));

    let on_eth_balance = piggy_bank.query().balance().await.unwrap();
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

    let on_eth_balance = piggy_bank.query().balance().await.unwrap();
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

    // Force-process the reply by sending a follow-up no-op message.
    let _ = env
        .send_message(piggy_bank_id, b"")
        .await
        .unwrap()
        .wait_for()
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

    stop_nodes([node]).await;
}

#[tokio::test]
#[ntest::timeout(60_000)]
async fn batch_commitment_squashes_repeated_ping_transitions() {
    init_logger();

    let mut env = TestEnv::default().await;

    let committed_batches = Arc::new(Mutex::new(Vec::new()));
    let recording_committer = RecordingCommitter {
        router: EthereumBuilder::default()
            .rpc_url(&env.eth_cfg.rpc)
            .router_address(env.eth_cfg.router_address)
            .signer(env.signer.clone())
            .sender_address(env.validators[0].public_key.to_address())
            .eip1559_fee_increase_percentage(env.eth_cfg.eip1559_fee_increase_percentage)
            .blob_gas_multiplier(env.eth_cfg.blob_gas_multiplier)
            .build()
            .await
            .unwrap()
            .router(),
        committed_batches: committed_batches.clone(),
    };

    let mut node = env
        .new_node(NodeConfig::default().validator(env.validators[0]))
        .await;
    node.custom_committer = Some(Box::new(recording_committer.clone()));
    node.start_service().await;

    let uploaded_code = env
        .upload_code(demo_ping::WASM_BINARY)
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();
    assert!(uploaded_code.valid);

    let program = env
        .create_program(uploaded_code.code_id, 500_000_000_000_000)
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();
    let ping_id = program.program_id;

    committed_batches.lock().await.clear();

    node.stop_service().await;

    let first_ping = env.send_message(ping_id, b"PING").await.unwrap();
    let second_ping = env.send_message(ping_id, b"PING").await.unwrap();

    env.skip_blocks(env.commitment_delay_limit.get() as u32 + 2)
        .await;

    node.custom_committer = Some(Box::new(recording_committer));
    node.start_service().await;
    env.force_new_block().await;

    let first_reply = first_ping.wait_for().await.unwrap();
    assert_eq!(first_reply.program_id, ping_id);
    assert_eq!(
        first_reply.code,
        ReplyCode::Success(SuccessReplyReason::Manual)
    );
    assert_eq!(first_reply.payload, b"PONG");

    let second_reply = second_ping.wait_for().await.unwrap();
    assert_eq!(second_reply.program_id, ping_id);
    assert_eq!(
        second_reply.code,
        ReplyCode::Success(SuccessReplyReason::Manual)
    );
    assert_eq!(second_reply.payload, b"PONG");

    let committed_batches = committed_batches.lock().await.clone();
    let matching_batch = committed_batches
        .iter()
        .find(|batch| {
            batch.chain_commitment.as_ref().is_some_and(|chain| {
                chain.transitions.iter().any(|transition| {
                    transition.actor_id == ping_id && transition.messages.len() == 2
                })
            })
        })
        .expect("expected committed batch with a squashed ping program transition");
    let chain_commitment = matching_batch
        .chain_commitment
        .as_ref()
        .expect("expected chain commitment");

    assert_eq!(
        chain_commitment
            .transitions
            .iter()
            .filter(|transition| transition.actor_id == ping_id)
            .count(),
        1,
        "repeated transitions for the same actor must be squashed before commit"
    );

    let squashed_transition = chain_commitment
        .transitions
        .iter()
        .find(|transition| transition.actor_id == ping_id)
        .expect("expected squashed transition for ping actor");
    assert_eq!(
        squashed_transition.messages.len(),
        2,
        "squashed transition must carry both reply messages"
    );
    assert!(
        squashed_transition
            .messages
            .iter()
            .all(|message| message.payload == b"PONG"),
        "expected both outgoing messages to be PONG replies"
    );

    stop_nodes([node]).await;
}

#[tokio::test]
#[ntest::timeout(60_000)]
async fn incoming_transfers() {
    init_logger();

    let mut env = TestEnv::default().await;

    let mut node = env
        .new_node(NodeConfig::default().validator(env.validators[0]))
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

    let on_eth_balance = ping.query().balance().await.unwrap();
    assert_eq!(on_eth_balance, 0);

    let state_hash = ping.query().state_hash().await.unwrap();
    let local_balance = node.db.program_state(state_hash).unwrap().balance;
    assert_eq!(local_balance, 0);

    // 1_000 ETH
    const VALUE_SENT: u128 = 1_000 * ETHER;

    ping.owned_balance_top_up(VALUE_SENT).await.unwrap();

    // Force the validator to advance past the top-up Eth event by
    // sending a PING and waiting for its reply. By the time the
    // reply lands, every prior Eth event (including the top-up
    // we just submitted) has been folded into a finalised MB and
    // the resulting batch committed on-chain.
    let res = env
        .send_message(ping_id, b"PING")
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();
    assert_eq!(res.code, ReplyCode::Success(SuccessReplyReason::Manual));

    let on_eth_balance = ping.query().balance().await.unwrap();
    assert_eq!(on_eth_balance, VALUE_SENT);

    let state_hash = ping.query().state_hash().await.unwrap();
    let local_balance = node.db.program_state(state_hash).unwrap().balance;
    assert_eq!(local_balance, VALUE_SENT);

    let res = env
        .send_message_with_params(ping_id, b"PING", VALUE_SENT)
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();

    assert_eq!(res.code, ReplyCode::Success(SuccessReplyReason::Manual));
    assert_eq!(res.value, 0);

    let on_eth_balance = ping.query().balance().await.unwrap();
    assert_eq!(on_eth_balance, 2 * VALUE_SENT);

    let state_hash = ping.query().state_hash().await.unwrap();
    let local_balance = node.db.program_state(state_hash).unwrap().balance;
    assert_eq!(local_balance, 2 * VALUE_SENT);

    stop_nodes([node]).await;
}

/// Ping survives a small Anvil reorg and a DB cleanup. The reorg depth in the
/// test stays *within* `canonical_quarantine`, so the network must not enter
/// the diverging-finalized-MB regime.
///
/// Currently `#[ignore]`d: with malachite producing MBs continuously, the
/// validator advances its finalized MB to Eth blocks beyond the snapshot
/// boundary, so `anvil_revert` orphans the advance and the post-revert
/// coordinator refuses to commit (correct per the canonical-advance check).
/// Re-enable once bad-block compensation is implemented.
#[tokio::test]
#[ntest::timeout(60_000)]
async fn reorg_within_quarantine() {
    init_logger();

    let mut env = TestEnv::new(TestEnvConfig {
        network: EnvNetworkConfig::Enabled,
        // Quarantine large enough that small Anvil reorgs sit inside it.
        canonical_quarantine: 8,
        ..Default::default()
    })
    .await
    .unwrap();

    let mut connect_node = env.new_node(NodeConfig::named("connect")).await;
    connect_node.start_service().await;

    let mut node = env
        .new_node(NodeConfig::named("validator").validator(env.validators[0]))
        .await;
    node.start_service().await;

    let code_id = env
        .upload_code(demo_ping::WASM_BINARY)
        .await
        .unwrap()
        .wait_for()
        .await
        .map(|res| {
            assert!(res.valid);
            res.code_id
        })
        .unwrap();

    let latest_block = env.latest_block().await;
    connect_node
        .events()
        .find_block_prepared(latest_block.hash)
        .await;

    test_info!("📗 Stop validator service to simulate node block skipping");
    node.stop_service().await;

    let wait_for_program_creation = env
        .create_program(code_id, 500_000_000_000_000)
        .await
        .unwrap();
    let wait_for_init_reply = env
        .send_message(wait_for_program_creation.program_id, b"PING")
        .await
        .unwrap();

    env.skip_blocks(10).await;

    test_info!("Start service after 10 blocks skipping");
    node.start_service().await;

    let res = wait_for_program_creation.wait_for().await.unwrap();
    let init_res = wait_for_init_reply.wait_for().await.unwrap();
    assert_eq!(res.code_id, code_id);
    assert_eq!(init_res.payload, b"PONG");

    let ping_id = res.program_id;

    let wait_for_reply_to_ping = env.send_message(ping_id, b"PING").await.unwrap();

    let latest_block = env.latest_block().await;
    test_info!("📗 Create snapshot at {latest_block}",);
    let program_created_snapshot_id = env.provider.anvil_snapshot().await.unwrap();

    // Add more blocks for reorg
    env.skip_blocks(2).await;

    let latest_block1 = env.latest_block().await;
    test_info!("📗 Reverting from {latest_block1} to {latest_block} — small reorg");
    env.provider
        .anvil_revert(program_created_snapshot_id)
        .await
        .map(|res| assert!(res))
        .unwrap();

    // Skip quarantine to receive reply faster
    env.skip_blocks(8).await;

    let res = wait_for_reply_to_ping.wait_for().await.unwrap();
    assert_eq!(res.code, ReplyCode::Success(SuccessReplyReason::Manual));
    assert_eq!(res.payload, b"PONG");

    stop_nodes([connect_node, node]).await;
}

/// Deep reorg — past the `canonical_quarantine` window.
#[tokio::test]
#[ntest::timeout(80_000)]
async fn reorg_deeper_than_quarantine() {
    init_logger();

    let mut env = TestEnv::new(TestEnvConfig {
        // Tiny quarantine so an Anvil snapshot/revert easily surpasses it.
        canonical_quarantine: 2,
        ..Default::default()
    })
    .await
    .unwrap();

    let mut node = env
        .new_node(NodeConfig::default().validator(env.validators[0]))
        .await;
    node.start_service().await;

    test_info!("Upload, create and initialize the demo-ping program");
    let code = env
        .upload_code(demo_ping::WASM_BINARY)
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();
    let prog = env
        .create_program(code.code_id, 500_000_000_000_000)
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();
    let r = env
        .send_message(prog.program_id, b"PING")
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();
    assert_eq!(r.payload, b"PONG");

    // Snapshot the program is initialized and finalized in mb.
    let snap = env.provider.anvil_snapshot().await.unwrap();
    test_info!(
        "Snapshot taken at block {}",
        env.provider.get_block_number().await.unwrap()
    );

    env.skip_blocks(10).await;

    let latest_block = env.latest_block().await;
    test_info!("Waiting for {latest_block} to be finalized in MB");
    node.events()
        .wait_till_eth_block_finalized_in_mb(latest_block.hash)
        .await;

    test_info!("📗 Reverting Anvil to deep snapshot — past quarantine");
    env.provider
        .anvil_revert(snap)
        .await
        .map(|res| assert!(res))
        .unwrap();

    env.skip_blocks(20).await;

    let latest_block = env.latest_block().await;
    test_info!(
        "waiting 20 seconds: {latest_block} must not be finalized, because deep reorg breaks mb chain continuity"
    );
    let mut receiver = node.new_events();
    // Here we take in account kicking stream - latest_block is not passed quarantine yet,
    // but kicks will generate new anvil blocks, but still this block cannot be finalized in mb, because branch is broken.
    let waiting_future = receiver.wait_till_eth_block_finalized_in_mb(latest_block.hash);
    tokio::time::timeout(Duration::from_secs(20), waiting_future)
        .await
        .expect_err("block should not be finalized within 20 seconds after deep reorg");

    stop_nodes([node]).await;
}

#[tokio::test]
#[ntest::timeout(60_000)]
async fn ping_deep_sync() {
    init_logger();

    let mut env = TestEnv::default().await;

    let mut node = env
        .new_node(NodeConfig::default().validator(env.validators[0]))
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

    stop_nodes([node]).await;
}

/// Multi-validator end-to-end smoke. Boots four validators, runs
/// upload+create+message round-trips against `demo-ping` and
/// `demo-async`, then exercises liveness while validators are
/// stopped/restarted to check the BFT quorum bookkeeping.
///
/// Tendermint quorum is strictly > 2/3 of voting power, so with
/// N=3 even one failure halts BFT. We use N=4 (quorum = 3) so the
/// "stop one validator and keep going" half of the test remains
/// meaningful, while "stop two" still falls below quorum.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ntest::timeout(120_000)]
async fn multiple_validators() {
    init_logger();

    let config = TestEnvConfig {
        validators: ValidatorsConfig::PreDefined(4),
        network: EnvNetworkConfig::Enabled,
        ..Default::default()
    };
    let mut env = TestEnv::new(config).await.unwrap();

    assert_eq!(
        env.validators.len(),
        4,
        "Currently only 4 validators are supported for this test"
    );
    assert!(
        !env.continuous_block_generation,
        "Currently continuous block generation is not supported for this test"
    );

    let mut validators = vec![];
    for (i, v) in env.validators.clone().into_iter().enumerate() {
        test_info!("📗 Starting validator-{i}");
        let mut validator = env
            .new_node(NodeConfig::named(format!("validator-{i}")).validator(v))
            .await;
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

    test_info!("📗 Stop validator 0 and check that ethexe is still working with 2/3 quorum");
    validators[0].stop_service().await;

    let res = env
        .send_message(async_id, demo_async::Command::Common.encode().as_slice())
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();
    assert_eq!(res.payload, res.message_id.encode().as_slice());

    test_info!("📗 Stop validator 1 and check that ethexe is not working below threshold");
    validators[1].stop_service().await;

    let wait_for_reply_to = env
        .send_message(async_id, demo_async::Command::Common.encode().as_slice())
        .await
        .unwrap();

    tokio::time::timeout(
        env.eth_cfg.block_time * 5,
        wait_for_reply_to.clone().wait_for(),
    )
    .await
    .expect_err("Timeout expected — only 1/3 validators alive");

    test_info!("📗 Re-start validator 0; with 2/3 alive ethexe should make progress again");
    validators[0].start_service().await;

    let res = wait_for_reply_to.wait_for().await.unwrap();
    assert_eq!(res.payload, res.message_id.encode().as_slice());
}

#[tokio::test]
#[ntest::timeout(120_000)]
async fn many_validators_repeated_ping() {
    init_logger();

    const VALIDATORS_COUNT: usize = 8;
    const PING_ROUNDS: usize = 4;

    test_info!(
        "📗 Starting many_validators_repeated_ping with {VALIDATORS_COUNT} validators and {PING_ROUNDS} ping rounds"
    );

    let signer = Signer::memory();
    let validators: Vec<_> = (0..VALIDATORS_COUNT)
        .map(|_| signer.generate().expect("must generate validator key"))
        .collect();

    let config = TestEnvConfig {
        validators: ValidatorsConfig::ProvidedValidators(validators),
        network: EnvNetworkConfig::Enabled,
        signer: signer.clone(),
        ..Default::default()
    };
    let mut env = TestEnv::new(config).await.unwrap();

    test_info!("📗 Top-up balances for all validator accounts");
    let validator_balance: U256 = (10_000 * ETHER).try_into().unwrap();
    for validator in &env.validators {
        env.provider
            .anvil_set_balance(validator.public_key.to_address().into(), validator_balance)
            .await
            .unwrap();
    }

    let mut running_validators = Vec::with_capacity(VALIDATORS_COUNT);
    for (i, validator_cfg) in env.validators.clone().into_iter().enumerate() {
        test_info!("📗 Starting validator-{i}");
        let mut node = env
            .new_node(NodeConfig::named(format!("validator-{i}")).validator(validator_cfg))
            .await;
        node.start_service().await;
        running_validators.push(node);
    }

    test_info!("📗 Upload demo_ping code");
    let uploaded_code = env
        .upload_code(demo_ping::WASM_BINARY)
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();
    assert!(uploaded_code.valid);

    test_info!("📗 Create demo_ping program");
    let program = env
        .create_program(uploaded_code.code_id, 500_000_000_000_000)
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();

    let ping_id = program.program_id;
    for i in 0..PING_ROUNDS {
        test_info!("📗 PING round {}/{}", i + 1, PING_ROUNDS);
        let reply = env
            .send_message(ping_id, b"PING")
            .await
            .unwrap()
            .wait_for()
            .await
            .unwrap();

        assert_eq!(
            reply.program_id, ping_id,
            "unexpected program for round {i}"
        );
        assert_eq!(
            reply.code,
            ReplyCode::Success(SuccessReplyReason::Manual),
            "unexpected reply code for round {i}"
        );
        assert_eq!(reply.payload, b"PONG", "unexpected payload for round {i}");
        assert_eq!(reply.value, 0, "unexpected value for round {i}");
    }

    test_info!("📗 Completed all ping rounds successfully");

    assert_eq!(running_validators.len(), VALIDATORS_COUNT);

    stop_nodes(running_validators).await;
}

#[tokio::test]
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

    test_info!("📗 Starting node 0");
    let mut node0 = env
        .new_node(
            NodeConfig::default()
                .validator(env.validators[0])
                .service_rpc(9505),
        )
        .await;
    node0.start_service().await;

    test_info!("📗 Starting node 1");
    let mut node1 = env
        .new_node(
            NodeConfig::default()
                .service_rpc(9506)
                .validator(env.validators[1]),
        )
        .await;
    node1.start_service().await;

    test_info!("Populate node-0 and node-1 with 2 valid blocks");

    env.force_new_block().await;
    env.force_new_block().await;

    // Give some time for nodes to process the blocks
    let reference_block = node0.db.globals().latest_prepared_eb_hash;

    // Prepare tx data
    let tx = InjectedTransaction {
        destination: ActorId::from(H160::random()),
        payload: H256::random().0.to_vec().try_into().unwrap(),
        value: 0,
        reference_block,
        salt: vec![1].try_into().unwrap(),
    };

    let tx_for_node1 = AddressedInjectedTransaction {
        recipient: validator1_pubkey.to_address(),
        tx: env
            .signer
            .signed_message(validator0_pubkey, tx.clone(), None)
            .unwrap(),
    };

    // Send request
    test_info!("Sending transaction to node-1");
    let acceptance = node1
        .rpc_http_client()
        .unwrap()
        .send_transaction(tx_for_node1.clone())
        .await
        .expect("rpc server is set");
    assert_eq!(acceptance, InjectedTransactionAcceptance::Accept);

    // Tx executable validation takes time, so wait for event.
    node1
        .events()
        .find(|event| {
            // RPC fan-out emits one InjectedTransaction event per
            // validator, so match on the v1-targeted one — that's
            // the one whose recipient equals `tx_for_node1.recipient`.
            if let TestingEvent::Rpc(TestingRpcEvent::InjectedTransaction { transaction }) = event
                && *transaction == tx_for_node1
            {
                true
            } else {
                false
            }
        })
        .await;

    // Check that node-1 save received tx.
    let node1_db_tx = node1
        .db
        .injected_transaction(tx.to_hash())
        .expect("tx not found");
    assert_eq!(node1_db_tx, tx_for_node1.tx);

    stop_nodes([node0, node1]).await;
}

#[tokio::test]
#[ntest::timeout(60_000)]
async fn injected_tx_purged_receipt() {
    init_logger();

    let test_env_config = TestEnvConfig {
        network: EnvNetworkConfig::Enabled,
        ..Default::default()
    };
    let mut env = TestEnv::new(test_env_config).await.unwrap();

    let pubkey = env.validators[0].public_key;
    let mut node = env
        .new_node(
            NodeConfig::default()
                .service_rpc(9507)
                .validator(env.validators[0]),
        )
        .await;
    node.start_service().await;

    let rpc_client = node
        .rpc_ws_client()
        .await
        .expect("RPC client provide by node");

    let tx = InjectedTransaction {
        destination: ActorId::from(H160::random()),
        payload: vec![].try_into().unwrap(),
        value: 0,
        reference_block: H256::zero(),
        salt: vec![1].try_into().unwrap(),
    };
    let tx_hash = tx.to_hash();
    let rpc_tx = AddressedInjectedTransaction {
        recipient: pubkey.to_address(),
        tx: env.signer.signed_message(pubkey, tx, None).unwrap(),
    };

    let mut subscription = rpc_client
        .send_transaction_and_watch(rpc_tx)
        .await
        .expect("successfully subscribe for transaction receipt");

    env.force_new_block().await;

    let subscription_receipt = subscription
        .next()
        .await
        .expect("subscription produces a receipt")
        .expect("no RPC subscription error");
    let Receipt::Purged(purged) = subscription_receipt.data() else {
        panic!(
            "expected purged receipt, got {:?}",
            subscription_receipt.data()
        );
    };
    assert_eq!(purged.tx_hash, tx_hash);
    assert_eq!(
        purged.reason,
        TransactionPurgedReason::UnknownReferenceBlock
    );

    let stored_receipt = rpc_client
        .get_transaction_receipt(tx_hash)
        .await
        .expect("receipt lookup succeeds")
        .expect("receipt is stored");
    assert_eq!(stored_receipt, subscription_receipt);

    stop_nodes([node]).await;
}

/// 5+5 validator election handover: stage next validator set during the
/// election window of era N, fire one `ValidatorsCommittedForEra`, swap
/// validators when era N+1 starts, and verify the new set can serve PING.
#[tokio::test]
#[ntest::timeout(60_000)]
async fn validators_election() {
    init_logger();
    use crate::tests::utils::Wallets;
    use ethexe_common::events::{RouterEvent, router::ValidatorsCommittedForEraEvent};
    use ethexe_ethereum::deploy::ContractsDeploymentParams;

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
        test_info!("📗 Starting validator-{i}");
        let mut validator = env
            .new_node(NodeConfig::named(format!("validator-{i}")).validator(v))
            .await;
        validator.start_service().await;
        validators.push(validator);
    }

    // Setup next validators to be elected for previous era
    let next_validators_configs = TestEnv::define_session_keys(next_validators);

    let next_validator_addrs: Vec<_> = next_validators_configs
        .iter()
        .map(|cfg| cfg.public_key.to_address())
        .collect();

    env.election_provider
        .set_predefined_election_at(
            election_ts + genesis_ts,
            next_validator_addrs.try_into().unwrap(),
        )
        .await;

    env.provider
        .anvil_set_next_block_timestamp(election_ts + genesis_ts)
        .await
        .unwrap();
    env.force_new_block().await;

    env.new_observer_events()
        .filter_map_block_synced()
        .find(|event| {
            matches!(
                event,
                BlockEvent::Router(RouterEvent::ValidatorsCommittedForEra(
                    ValidatorsCommittedForEraEvent { era_index: _ }
                ))
            )
        })
        .await;

    test_info!("📗 Next validators successfully committed");

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

    stop_nodes(validators).await;

    env.extend_malachite_endpoints(&next_validators_configs);
    env.validators = next_validators_configs;
    let mut new_validators = vec![];
    for (i, v) in env.validators.clone().into_iter().enumerate() {
        test_info!("📗 Starting next validator-{i}");
        let mut validator = env
            .new_node(NodeConfig::named(format!("validator-{i}")).validator(v))
            .await;
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

    stop_nodes(new_validators).await;
}

/// Validators must NOT fold an Ethereum event into MB execution before the
/// event has aged past `canonical_quarantine`. Send PING, watch the next
/// `canonical_quarantine` blocks, assert no PONG appears, then poll the
/// following blocks for PONG.
#[tokio::test]
#[ntest::timeout(120_000)]
async fn execution_with_canonical_events_quarantine() {
    init_logger();

    // Production uses 16; 4 keeps the test fast while still exercising > 1
    // block of quarantine.
    let config = TestEnvConfig {
        canonical_quarantine: 4,
        ..Default::default()
    };
    let mut env = TestEnv::new(config).await.unwrap();

    let mut validator = env
        .new_node(NodeConfig::default().validator(env.validators[0]))
        .await;
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

    let canonical_quarantine = env.canonical_quarantine as u32;
    env.skip_blocks(canonical_quarantine + 2).await;

    env.new_observer_events()
        .filter_map_block_synced()
        .find(|event| {
            matches!(
                event,
                BlockEvent::Mirror {
                    event: MirrorEvent::StateChanged { .. },
                    ..
                }
            )
        })
        .await;

    let latest_block: H256 = env.latest_block().await.hash;
    test_info!("📗 waiting block-prepared for block {latest_block}");
    validator.events().find_block_prepared(latest_block).await;

    let mut receiver = validator.new_events();
    let validator_db = validator.db.clone();
    let message_id = env
        .send_message(res.program_id, b"PING")
        .await
        .unwrap()
        .message_id;

    let check_for_pong = |block_hash| {
        let block_events = validator_db.block_events(block_hash).unwrap_or_default();
        for block_event in block_events {
            if let BlockEvent::Mirror {
                actor_id: _,
                event:
                    MirrorEvent::Reply(ReplyEvent {
                        payload,
                        value: _,
                        reply_to,
                        reply_code: _,
                    }),
            } = block_event
                && reply_to == message_id
                && payload == b"PONG"
            {
                return true;
            }
        }
        false
    };

    for _ in 0..canonical_quarantine {
        let block_hash = receiver.find_block_synced().await;
        assert!(!check_for_pong(block_hash), "PONG received too early");
        receiver.find_block_prepared(block_hash).await;
        env.force_new_block().await;
    }

    // Past quarantine: MB needs more chain heads to advance through the
    // canonical-quarantine window and commit the PING reply. The receiver's
    // built-in kick mines a fresh Anvil block after `kicking_per_blocks` of
    // stream silence, so each `find_block_synced` here is also a chance for
    // the validator to make progress. Poll up to a generous budget instead of
    // assuming PONG lands in the very next block.
    const POST_QUARANTINE_BUDGET: usize = 20;
    let mut pong_block = None;
    for _ in 0..POST_QUARANTINE_BUDGET {
        let block_hash = receiver.find_block_synced().await;
        if check_for_pong(block_hash) {
            pong_block = Some(block_hash);
            break;
        }
    }
    assert!(
        pong_block.is_some(),
        "PONG not received within {POST_QUARANTINE_BUDGET} blocks after quarantine"
    );

    stop_nodes([validator]).await;
}

#[tokio::test]
#[ntest::timeout(60_000)]
async fn value_send_program_to_program() {
    // 1_000 ETH
    const VALUE_SENT: u128 = 1_000 * ETHER;

    init_logger();

    let mut env = TestEnv::default().await;

    let mut node = env
        .new_node(NodeConfig::default().validator(env.validators[0]))
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

    let value_receiver_on_eth_balance = value_receiver.query().balance().await.unwrap();
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
        .send_message_with_params(res.program_id, &value_receiver_id.encode(), VALUE_SENT)
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

    let value_sender_on_eth_balance = value_sender.query().balance().await.unwrap();
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

    let value_sender_on_eth_balance = value_sender.query().balance().await.unwrap();
    assert_eq!(value_sender_on_eth_balance, 0);

    let value_sender_state_hash = value_sender.query().state_hash().await.unwrap();
    let value_sender_local_balance = node
        .db
        .program_state(value_sender_state_hash)
        .unwrap()
        .balance;
    assert_eq!(value_sender_local_balance, 0);

    let value_receiver_on_eth_balance = value_receiver.query().balance().await.unwrap();
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

    stop_nodes([node]).await;
}

/// Delayed value send: program A queues a `send_value(receiver, V)` with a
/// non-zero delay, value sits at the Router contract until the delay
/// elapses, then lands on the receiver's program balance + Eth-side mirror.
#[tokio::test]
#[ntest::timeout(120_000)]
async fn value_send_delayed() {
    use ethexe_common::events::RouterEvent;

    const VALUE_SENT: u128 = 1_000 * ETHER;

    init_logger();

    let mut env = TestEnv::default().await;

    let mut node = env
        .new_node(NodeConfig::default().validator(env.validators[0]))
        .await;
    node.start_service().await;

    test_info!("Upload, create and initialize the value receiver contract demo-ping");
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
    let value_receiver_on_eth_balance = value_receiver.query().balance().await.unwrap();
    assert_eq!(value_receiver_on_eth_balance, 0);
    let value_receiver_state_hash = value_receiver.query().state_hash().await.unwrap();
    let value_receiver_local_balance = node
        .db
        .program_state(value_receiver_state_hash)
        .unwrap()
        .balance;
    assert_eq!(value_receiver_local_balance, 0);

    test_info!("Upload, create and initialize the delayed sender contract demo-delayed-sender");
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
    let res = env
        .send_message_with_params(res.program_id, &value_receiver_id.encode(), VALUE_SENT)
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
    let value_sender_on_eth_balance = value_sender.query().balance().await.unwrap();
    assert_eq!(value_sender_on_eth_balance, 0);
    let value_sender_state_hash = value_sender.query().state_hash().await.unwrap();
    let value_sender_local_balance = node
        .db
        .program_state(value_sender_state_hash)
        .unwrap()
        .balance;
    assert_eq!(value_sender_local_balance, 0);
    let value_receiver_on_eth_balance = value_receiver.query().balance().await.unwrap();
    assert_eq!(value_receiver_on_eth_balance, 0);
    let router_address = env.ethereum.router().address();
    let router_balance = env
        .ethereum
        .provider()
        .get_balance(router_address.into())
        .await
        .map(ethexe_ethereum::abi::utils::uint256_to_u128_lossy)
        .unwrap();
    assert_eq!(router_balance, VALUE_SENT);

    test_info!("Mine blocks until the delayed value lands on the receiver");
    let receiver = env.new_observer_events();
    env.provider
        .anvil_mine(Some(demo_delayed_sender_ethexe::DELAY.into()), None)
        .await
        .unwrap();
    receiver
        .filter_map_block_synced()
        .find(|e| matches!(e, BlockEvent::Router(RouterEvent::BatchCommitted { .. })))
        .await;

    let value_receiver_on_eth_balance = value_receiver.query().balance().await.unwrap();
    assert_eq!(value_receiver_on_eth_balance, VALUE_SENT);

    let router_balance = env
        .ethereum
        .provider()
        .get_balance(router_address.into())
        .await
        .map(ethexe_ethereum::abi::utils::uint256_to_u128_lossy)
        .unwrap();
    assert_eq!(router_balance, 0);

    stop_nodes([node]).await;
}

/// Mint + Transfer flow on demo_fungible_token via the RPC injected-tx
/// path. Validates promise streaming and on-chain state convergence.
#[tokio::test]
#[ntest::timeout(60_000)]
async fn injected_tx_fungible_token() {
    use ethexe_common::events::mirror::StateChangedEvent;
    use ethexe_compute::ComputeEvent;
    use ethexe_observer::ObserverEvent;

    init_logger();

    let env_config = TestEnvConfig {
        network: EnvNetworkConfig::Enabled,
        ..Default::default()
    };
    let mut env = TestEnv::new(env_config).await.unwrap();

    let pubkey = env.validators[0].public_key;
    let mut node = env
        .new_node(
            NodeConfig::default()
                .service_rpc(8090)
                .validator(env.validators[0]),
        )
        .await;
    node.start_service().await;
    let rpc_client = node
        .rpc_ws_client()
        .await
        .expect("RPC client provide by node");

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

    // 4. Try minting some tokens
    let amount: u128 = 5_000_000_000;
    let mint_action = demo_fungible_token::FTAction::Mint(amount);

    let mint_tx = InjectedTransaction {
        destination: usdt_actor_id,
        payload: mint_action.encode().try_into().unwrap(),
        value: 0,
        reference_block: node.db.globals().latest_prepared_eb_hash,
        salt: vec![1].try_into().unwrap(),
    };

    let rpc_tx = AddressedInjectedTransaction {
        recipient: pubkey.to_address(),
        tx: env
            .signer
            .signed_message(pubkey, mint_tx.clone(), None)
            .unwrap(),
    };

    let mut subscription = rpc_client
        .send_transaction_and_watch(rpc_tx)
        .await
        .expect("successfully send transaction to RPC");

    let expected_event = demo_fungible_token::FTEvent::Transfer {
        from: ActorId::new([0u8; 32]),
        to: pubkey.to_address().into(),
        amount,
    };

    // Listen for inclusion and check the expected payload.
    node.events()
        .find(|event| {
            if let TestingEvent::Compute(ComputeEvent::Promise(promise, _)) = event {
                assert_eq!(promise.reply.payload, expected_event.encode());
                assert_eq!(
                    promise.reply.code,
                    ReplyCode::Success(SuccessReplyReason::Manual)
                );
                assert_eq!(promise.reply.value, 0);

                true
            } else {
                false
            }
        })
        .await;
    tracing::info!("✅ Tokens mint successfully");

    let subscription_receipt = subscription
        .next()
        .await
        .expect("subscription produce value")
        .expect("no errors for correct injected transaction");
    assert_eq!(subscription_receipt.data().tx_hash(), mint_tx.to_hash());
    let subscription_promise = subscription_receipt.data().clone().unwrap_promise();
    assert_eq!(subscription_promise.reply.value, 0);
    assert_eq!(
        subscription_promise.reply.code,
        ReplyCode::Success(SuccessReplyReason::Manual)
    );
    assert_eq!(
        subscription_receipt
            .data()
            .clone()
            .unwrap_promise()
            .reply
            .payload,
        expected_event.encode()
    );

    let db = node.db.clone();
    node.events()
        .find(|event| {
            if let TestingEvent::Observer(ObserverEvent::BlockSynced(synced_block)) = event {
                let Some(block_events) = db.block_events(*synced_block) else {
                    return false;
                };

                for block_event in block_events {
                    if let BlockEvent::Mirror {
                        actor_id,
                        event: MirrorEvent::StateChanged(StateChangedEvent { state_hash }),
                    } = block_event
                        && actor_id == mint_tx.destination
                    {
                        let state = db.program_state(state_hash).expect("state should be exist");
                        assert_eq!(state.balance, 0);
                        assert_eq!(state.injected_queue.cached_queue_size, 0);
                        assert_eq!(state.canonical_queue.cached_queue_size, 0);
                        return true;
                    }
                }
            }

            false
        })
        .await;
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
        payload: transfer_action.encode().try_into().unwrap(),
        value: 0,
        reference_block: node.db.globals().latest_prepared_eb_hash,
        salt: vec![1].try_into().unwrap(),
    };

    let rpc_tx = AddressedInjectedTransaction {
        recipient: pubkey.to_address(),
        tx: env
            .signer
            .signed_message(pubkey, transfer_tx.clone(), None)
            .unwrap(),
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
        .data()
        .clone()
        .unwrap_promise();

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

    stop_nodes([node]).await;
}

/// Same flow as `injected_tx_fungible_token` but the RPC is on a non-validator
/// (Alice) — the injected tx is gossiped through the p2p network to the
/// validator (Bob) and the promise comes back through both nodes.
#[tokio::test]
#[ntest::timeout(60_000)]
async fn injected_tx_fungible_token_over_network() {
    init_logger();

    let env_config = TestEnvConfig {
        network: EnvNetworkConfig::Enabled,
        canonical_quarantine: 0,
        ..Default::default()
    };

    let mut env = TestEnv::new(env_config).await.unwrap();

    let user_pubkey = env.signer.generate().unwrap();

    let mut alice_node = env
        .new_node(NodeConfig::named("Alice").service_rpc(8091))
        .await;
    alice_node.start_service().await;
    let alice_rpc_client = alice_node
        .rpc_ws_client()
        .await
        .expect("RPC client provide by node");

    let bob_pubkey = env.validators[0].public_key;
    let mut bob_node = env
        .new_node(NodeConfig::named("Bob").validator(env.validators[0]))
        .await;
    bob_node.start_service().await;

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

    // 4. Try minting some tokens
    let amount: u128 = 5_000_000_000;
    let mint_action = demo_fungible_token::FTAction::Mint(amount);

    let mint_tx = InjectedTransaction {
        destination: usdt_actor_id,
        payload: mint_action.encode().try_into().unwrap(),
        value: 0,
        reference_block: bob_node.db.globals().latest_prepared_eb_hash,
        salt: vec![1].try_into().unwrap(),
    };

    let rpc_tx = AddressedInjectedTransaction {
        recipient: bob_pubkey.to_address(),
        tx: env
            .signer
            .signed_message(user_pubkey, mint_tx.clone(), None)
            .unwrap(),
    };

    alice_node
        .events()
        .find(|event| {
            matches!(
                event,
                TestingEvent::Network(TestingNetworkEvent::ValidatorIdentityUpdated(_))
            )
        })
        .await;

    let mut subscription = alice_rpc_client
        .send_transaction_and_watch(rpc_tx)
        .await
        .expect("successfully subscribe for transaction promise");

    // wait for the injected transaction received before forcing a block
    bob_node
        .events()
        .find(|event| {
            matches!(
                event,
                TestingEvent::Network(TestingNetworkEvent::InjectedTransaction(_))
            )
        })
        .await;

    // force new block so consensus can produce promise
    env.force_new_block().await;

    let promise = subscription
        .next()
        .await
        .expect("promise from subscription")
        .expect("transaction promise")
        .data()
        .clone()
        .unwrap_promise();

    let expected_event = demo_fungible_token::FTEvent::Transfer {
        from: ActorId::new([0u8; 32]),
        to: user_pubkey.to_address().into(),
        amount,
    };

    let action = demo_fungible_token::FTEvent::decode(&mut &promise.reply.payload[..]).unwrap();
    assert_eq!(action, expected_event);
    assert_eq!(
        promise.reply.code,
        ReplyCode::Success(SuccessReplyReason::Manual)
    );
    assert_eq!(promise.reply.value, 0);

    stop_nodes([alice_node, bob_node]).await;
}

#[tokio::test]
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
        test_info!("📗 Starting validator-{i}");
        let mut validator = env
            .new_node(NodeConfig::named(format!("validator-{i}")).validator(v))
            .await;
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
        test_info!("📗 Stopping validator-{i}");
        v.stop_service().await;
    }

    let ping_wait_for = env.send_message(ping_id, b"PING").await.unwrap();

    let async_code_upload = env.upload_code(demo_async::WASM_BINARY).await.unwrap();

    test_info!("📗 Skipping 20 blocks");
    env.skip_blocks(20).await;

    for (i, v) in validators.iter_mut().enumerate() {
        test_info!("📗 Starting validator-{i} again");
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

#[tokio::test]
#[ntest::timeout(120_000)]
async fn reply_callback() {
    init_logger();

    let mut env = TestEnv::default().await;

    let mut node = env
        .new_node(NodeConfig::default().validator(env.validators[0]))
        .await;
    node.start_service().await;

    let res = env
        .upload_code(demo_reply_callback::WASM_BINARY)
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
    assert_eq!(code, demo_reply_callback::WASM_BINARY);

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
        .send_message(res.program_id, b"")
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();

    assert_eq!(res.code, ReplyCode::Success(SuccessReplyReason::Auto));
    assert_eq!(res.payload, b"");
    assert_eq!(res.value, 0);

    let program_id = res.program_id;

    let provider = env.ethereum.provider();
    let demo_caller =
        ethexe_ethereum::abi::IDemoCaller::deploy(provider.clone(), program_id.into())
            .await
            .expect("deploying DemoCaller failed");

    assert!(!demo_caller.replyOnMethodNameCalled().call().await.unwrap());

    demo_caller
        .methodName(false)
        .send()
        .await
        .unwrap()
        .try_get_receipt()
        .await
        .unwrap();

    // Force the validator to fold the demo_caller's call (and the
    // resulting reply back into the contract) into a finalised MB
    // by sending a no-op message + wait_for_reply.
    let _ = env
        .send_message(program_id, b"")
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();

    assert!(demo_caller.replyOnMethodNameCalled().call().await.unwrap());

    assert!(!demo_caller.onErrorReplyCalled().call().await.unwrap());

    demo_caller
        .methodName(true)
        .send()
        .await
        .unwrap()
        .try_get_receipt()
        .await
        .unwrap();

    let _ = env
        .send_message(program_id, b"")
        .await
        .unwrap()
        .wait_for()
        .await
        .unwrap();

    assert!(demo_caller.onErrorReplyCalled().call().await.unwrap());

    stop_nodes([node]).await;
}

#[tokio::test]
#[ignore = "TODO: #5487 port to MB-driven test harness"]
async fn fast_sync() {}

#[tokio::test]
#[ignore = "TODO: #5488 port to MB-driven test harness"]
async fn re_genesis_with_state_dump() {}

#[tokio::test]
#[ignore = "TODO: #5488 port to MB-driven test harness"]
async fn re_genesis_delayed_message() {}
