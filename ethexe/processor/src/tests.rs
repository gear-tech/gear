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

use crate::*;
use anyhow::{Result, anyhow};
use ethexe_common::{
    DEFAULT_BLOCK_GAS_LIMIT, SimpleBlockData,
    db::*,
    events::{BlockRequestEvent, MirrorRequestEvent, RouterRequestEvent},
    mock::*,
};
use ethexe_runtime_common::{RUNTIME_ID, state::MessageQueue};
use gear_core::ids::prelude::CodeIdExt;
use gprimitives::{ActorId, MessageId};
use parity_scale_codec::Encode;
use utils::*;

mod utils {
    use super::*;

    pub const VALID_PROGRAM: &str = r#"
        (module
        (import "env" "memory" (memory 1))
        (import "env" "gr_reply" (func $reply (param i32 i32 i32 i32)))
        (export "init" (func $init))
        (func $init
            (call $reply (i32.const 0) (i32.const 32) (i32.const 222) (i32.const 333))
        )
    )"#;

    pub const INVALID_PROGRAM: &str = r#"
        (module
        (import "env" "world" (func $world))
        (export "hello" (func $hello))
        (func $hello
            (call $world)
        )
    )"#;

    pub fn init_logger() {
        let _ = tracing_subscriber::fmt::try_init();
    }

    pub fn wat_to_wasm(wat: &str) -> (CodeId, Vec<u8>) {
        let code = wat::parse_str(wat).expect("failed to parse wat to bin");
        let code_id = CodeId::generate(&code);

        (code_id, code)
    }

    pub fn upload_code(processor: &mut Processor, code: &[u8]) -> CodeId {
        let code_id = CodeId::generate(code);

        let ValidCodeInfo {
            code,
            instrumented_code,
            code_metadata,
        } = processor
            .process_code(CodeAndIdUnchecked {
                code: code.to_vec(),
                code_id,
            })
            .expect("failed to process code")
            .valid
            .expect("code is invalid");

        let db = &processor.db;
        db.set_original_code(&code);
        db.set_instrumented_code(RUNTIME_ID, code_id, instrumented_code);
        db.set_code_metadata(code_id, code_metadata);
        db.set_code_valid(code_id, true);

        code_id
    }

    pub fn setup_test_env_and_load_codes<const N: usize>(
        codes: [&[u8]; N],
    ) -> (Processor, BlockChain, [CodeId; N]) {
        let db = Database::memory();
        let mut processor = Processor::new(db.clone()).unwrap();
        let chain = BlockChain::mock(20).setup(&db);

        let mut code_ids = Vec::new();
        for code in codes {
            code_ids.push(upload_code(&mut processor, code));
        }

        (processor, chain, code_ids.try_into().unwrap())
    }

    pub fn setup_handler(db: Database, block: SimpleBlockData) -> ProcessingHandler {
        let transitions = InBlockTransitions::new(
            block.header.height,
            Default::default(),
            Default::default(),
            Default::default(),
        );

        ProcessingHandler::new(db, transitions)
    }

    pub fn injected(
        destination: ActorId,
        payload: impl AsRef<[u8]>,
        value: u128,
    ) -> InjectedTransaction {
        InjectedTransaction {
            destination,
            payload: payload.as_ref().to_vec().into(),
            value,
            reference_block: H256::random(),
            salt: H256::random().0.to_vec().into(),
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn ping_init() {
    init_logger();

    let (mut processor, chain, [code_id]) = setup_test_env_and_load_codes([demo_ping::WASM_BINARY]);

    // Empty processing for block1
    let executable = ExecutableData {
        block: chain.blocks[1].to_simple(),
        ..Default::default()
    };
    let FinalizedBlockTransitions {
        states,
        schedule,
        program_creations,
        ..
    } = processor.process_programs(executable).await.unwrap();
    program_creations
        .into_iter()
        .for_each(|(pid, cid)| processor.db.set_program_code_id(pid, cid));

    let actor_id = ActorId::from(42);

    let create_program_events = vec![
        BlockRequestEvent::Router(RouterRequestEvent::ProgramCreated { actor_id, code_id }),
        BlockRequestEvent::Mirror {
            actor_id,
            event: MirrorRequestEvent::ExecutableBalanceTopUpRequested {
                value: 10_000_000_000,
            },
        },
        BlockRequestEvent::Mirror {
            actor_id,
            event: MirrorRequestEvent::MessageQueueingRequested {
                id: H256::random().0.into(),
                source: H256::random().0.into(),
                payload: b"PING".to_vec(),
                value: 0,
                call_reply: false,
            },
        },
    ];

    // Process for block2
    let executable = ExecutableData {
        block: chain.blocks[2].to_simple(),
        program_states: states,
        schedule,
        events: create_program_events,
        ..Default::default()
    };
    let FinalizedBlockTransitions {
        states,
        schedule,
        program_creations,
        ..
    } = processor
        .process_programs(executable)
        .await
        .expect("failed to process create program");
    program_creations
        .into_iter()
        .for_each(|(pid, cid)| processor.db.set_program_code_id(pid, cid));

    let send_message_event = BlockRequestEvent::Mirror {
        actor_id,
        event: MirrorRequestEvent::MessageQueueingRequested {
            id: H256::random().0.into(),
            source: H256::random().0.into(),
            payload: b"PING".to_vec(),
            value: 0,
            call_reply: false,
        },
    };

    // Process for block3
    let executable = ExecutableData {
        block: chain.blocks[3].to_simple(),
        program_states: states,
        schedule,
        events: vec![send_message_event],
        ..Default::default()
    };
    processor
        .process_programs(executable)
        .await
        .expect("failed to process send message");
}

#[test]
fn handle_new_code_valid() {
    init_logger();

    let mut processor = Processor::new(Database::memory()).expect("failed to create processor");

    let (code_id, code) = utils::wat_to_wasm(utils::VALID_PROGRAM);

    let (calculated_id, info) = processor
        .process_code(CodeAndIdUnchecked {
            code: code.clone(),
            code_id,
        })
        .map(|res| (res.code_id, res.valid.expect("code must be valid")))
        .unwrap();

    assert_eq!(calculated_id, code_id);

    assert!(info.instrumented_code.bytes().len() > info.code.len());
    assert_eq!(info.code, code);
    assert_eq!(
        info.code_metadata.original_code_len(),
        info.code.len() as u32,
    );
}

#[test]
fn handle_new_code_invalid() {
    init_logger();

    let mut processor = Processor::new(Database::memory()).expect("failed to create processor");

    let (code_id, code) = utils::wat_to_wasm(utils::INVALID_PROGRAM);

    assert!(
        processor
            .process_code(CodeAndIdUnchecked { code, code_id })
            .expect("failed to call runtime api")
            .valid
            .is_none()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn ping_pong() {
    init_logger();

    let (mut processor, chain, [code_id, ..]) =
        setup_test_env_and_load_codes([demo_ping::WASM_BINARY, demo_async::WASM_BINARY]);

    let user_id = ActorId::from(10);
    let actor_id = ActorId::from(0x10000);

    let mut handler = setup_handler(processor.db.clone(), chain.blocks[1].to_simple());

    handler
        .handle_router_event(RouterRequestEvent::ProgramCreated { actor_id, code_id })
        .expect("failed to create new program");

    handler
        .handle_mirror_event(
            actor_id,
            MirrorRequestEvent::ExecutableBalanceTopUpRequested {
                value: 150_000_000_000,
            },
        )
        .expect("failed to top up balance");

    handler
        .handle_mirror_event(
            actor_id,
            MirrorRequestEvent::MessageQueueingRequested {
                id: MessageId::from(1),
                source: user_id,
                payload: b"PING".to_vec(),
                value: 0,
                call_reply: false,
            },
        )
        .expect("failed to send message");

    handler
        .handle_mirror_event(
            actor_id,
            MirrorRequestEvent::MessageQueueingRequested {
                id: MessageId::from(2),
                source: user_id,
                payload: b"PING".to_vec(),
                value: 0,
                call_reply: false,
            },
        )
        .expect("failed to send message");

    let to_users = processor
        .process_queues(
            handler.into_transitions(),
            chain.blocks[1].to_simple(),
            DEFAULT_BLOCK_GAS_LIMIT,
        )
        .await
        .current_messages();

    assert_eq!(to_users.len(), 2);

    let message = &to_users[0].1;
    assert_eq!(message.destination, user_id);
    assert_eq!(message.payload, b"PONG");

    let message = &to_users[1].1;
    assert_eq!(message.destination, user_id);
    assert_eq!(message.payload, b"PONG");
}

#[tokio::test(flavor = "multi_thread")]
async fn async_and_ping() {
    init_logger();

    let mut message_nonce: u64 = 0;
    let mut get_next_message_id = || {
        message_nonce += 1;
        MessageId::from(message_nonce)
    };

    let (mut processor, chain, [ping_code_id, upload_code_id, ..]) =
        setup_test_env_and_load_codes([demo_ping::WASM_BINARY, demo_async::WASM_BINARY]);

    let mut handler = setup_handler(processor.db.clone(), chain.blocks[1].to_simple());

    let user_id = ActorId::from(10);
    let ping_id = ActorId::from(0x10000000);
    let async_id = ActorId::from(0x20000000);

    handler
        .handle_router_event(RouterRequestEvent::ProgramCreated {
            actor_id: ping_id,
            code_id: ping_code_id,
        })
        .expect("failed to create new program");

    handler
        .handle_mirror_event(
            ping_id,
            MirrorRequestEvent::ExecutableBalanceTopUpRequested {
                value: 350_000_000_000,
            },
        )
        .expect("failed to top up balance");

    handler
        .handle_mirror_event(
            ping_id,
            MirrorRequestEvent::MessageQueueingRequested {
                id: get_next_message_id(),
                source: user_id,
                payload: b"PING".to_vec(),
                value: 0,
                call_reply: false,
            },
        )
        .expect("failed to send message");

    handler
        .handle_router_event(RouterRequestEvent::ProgramCreated {
            actor_id: async_id,
            code_id: upload_code_id,
        })
        .expect("failed to create new program");

    handler
        .handle_mirror_event(
            async_id,
            MirrorRequestEvent::ExecutableBalanceTopUpRequested {
                value: 1_500_000_000_000,
            },
        )
        .expect("failed to top up balance");

    handler
        .handle_mirror_event(
            async_id,
            MirrorRequestEvent::MessageQueueingRequested {
                id: get_next_message_id(),
                source: user_id,
                payload: ping_id.encode(),
                value: 0,
                call_reply: false,
            },
        )
        .expect("failed to send message");

    let wait_for_reply_to = get_next_message_id();

    handler
        .handle_mirror_event(
            async_id,
            MirrorRequestEvent::MessageQueueingRequested {
                id: wait_for_reply_to,
                source: user_id,
                payload: demo_async::Command::Common.encode(),
                value: 0,
                call_reply: false,
            },
        )
        .expect("failed to send message");

    let transitions = processor
        .process_queues(
            handler.into_transitions(),
            chain.blocks[1].to_simple(),
            DEFAULT_BLOCK_GAS_LIMIT,
        )
        .await;

    let to_users = transitions.current_messages();

    assert_eq!(to_users.len(), 3);

    let message = &to_users[0].1;
    assert_eq!(message.destination, user_id);
    assert_eq!(message.payload, b"PONG");

    let message = &to_users[1].1;
    assert_eq!(message.destination, user_id);
    assert_eq!(message.payload, b"");

    let message = &to_users[2].1;
    assert_eq!(message.destination, user_id);
    assert_eq!(message.payload, wait_for_reply_to.into_bytes());
}

#[tokio::test(flavor = "multi_thread")]
async fn many_waits() {
    init_logger();

    let blocks_to_wait = 10;
    let wat = format!(
        r#"
        (module
            (import "env" "memory" (memory 1))
            (import "env" "gr_reply" (func $reply (param i32 i32 i32 i32)))
            (import "env" "gr_wait_for" (func $wait_for (param i32)))
            (export "handle" (func $handle))
            (func $handle
                (if
                    (i32.eqz (i32.load (i32.const 0x200)))
                    (then
                        (i32.store (i32.const 0x200) (i32.const 1))
                        (call $wait_for (i32.const {blocks_to_wait}))
                    )
                    (else
                        (call $reply (i32.const 0) (i32.const 13) (i32.const 0x400) (i32.const 0x600))
                    )
                )
            )
            (data (i32.const 0) "Hello, world!")
        )
        "#
    );

    let (_, code) = wat_to_wasm(wat.as_str());

    let (mut processor, chain, [code_id]) = setup_test_env_and_load_codes([code.as_slice()]);

    let mut handler = setup_handler(processor.db.clone(), chain.blocks[1].to_simple());

    let amount = 10000;
    for i in 0..amount {
        let program_id = ActorId::from(i);

        handler
            .handle_router_event(RouterRequestEvent::ProgramCreated {
                actor_id: program_id,
                code_id,
            })
            .expect("failed to create new program");

        handler
            .handle_mirror_event(
                program_id,
                MirrorRequestEvent::ExecutableBalanceTopUpRequested {
                    value: 150_000_000_000,
                },
            )
            .expect("failed to top up balance");

        handler
            .handle_mirror_event(
                program_id,
                MirrorRequestEvent::MessageQueueingRequested {
                    id: H256::random().0.into(),
                    source: H256::random().0.into(),
                    payload: Default::default(),
                    value: 0,
                    call_reply: false,
                },
            )
            .expect("failed to send message");
    }

    handler.transitions = processor.process_tasks(handler.transitions);
    handler.transitions = processor
        .process_queues(
            handler.transitions,
            chain.blocks[1].to_simple(),
            DEFAULT_BLOCK_GAS_LIMIT,
        )
        .await;
    assert_eq!(
        handler.transitions.current_messages().len(),
        amount as usize
    );

    // Try to send messages to programs once more. Messages must be executed completely.
    let known_programs = handler.transitions.known_programs();
    for pid in known_programs {
        handler
            .handle_mirror_event(
                pid,
                MirrorRequestEvent::MessageQueueingRequested {
                    id: H256::random().0.into(),
                    source: H256::random().0.into(),
                    payload: Default::default(),
                    value: 0,
                    call_reply: false,
                },
            )
            .expect("failed to send message");
    }
    handler.transitions = processor
        .process_queues(
            handler.transitions,
            chain.blocks[1].to_simple(),
            DEFAULT_BLOCK_GAS_LIMIT,
        )
        .await;
    assert_eq!(
        handler.transitions.current_messages().len(),
        amount as usize
    );

    let FinalizedBlockTransitions {
        states,
        schedule,
        program_creations,
        ..
    } = handler.transitions.finalize();
    program_creations
        .into_iter()
        .for_each(|(pid, cid)| processor.db.set_program_code_id(pid, cid));

    // Check all messages wake up and reply with "Hello, world!"
    let wake_block = chain.blocks[1 + blocks_to_wait].to_simple();
    let transitions = InBlockTransitions::new(
        wake_block.header.height,
        states,
        schedule,
        Default::default(),
    );
    let transitions = processor.process_tasks(transitions);
    let transitions = processor
        .process_queues(transitions, wake_block, DEFAULT_BLOCK_GAS_LIMIT)
        .await;

    assert_eq!(transitions.current_messages().len(), amount as usize);

    for (_pid, message) in transitions.current_messages() {
        assert_eq!(message.payload, b"Hello, world!");
    }
}

// Tests that when overlay execution is performed, it doesn't change the original state.
#[tokio::test(flavor = "multi_thread")]
async fn overlay_execution() {
    init_logger();

    // Define message id generator.
    let mut message_nonce: u64 = 0;
    let mut get_next_message_id = || {
        message_nonce += 1;
        MessageId::from(message_nonce)
    };

    // Define function to get message queue from state hash.
    let get_mq_from_state_hash =
        |state_hash: H256, processor: &Processor| -> Result<MessageQueue> {
            let state = processor
                .db
                .program_state(state_hash)
                .ok_or(anyhow!("failed to read pid state"))?;

            state.canonical_queue.query(&processor.db)
        };

    let (mut processor, chain, [ping_code_id, async_code_id]) =
        setup_test_env_and_load_codes([demo_ping::WASM_BINARY, demo_async::WASM_BINARY]);

    // -----------------------------------------------------------------------------
    // --------------------------- Create programs ---------------------------------
    // -----------------------------------------------------------------------------
    let user_id = ActorId::from(10);
    let ping_id = ActorId::from(0x10000000);
    let async_id = ActorId::from(0x20000000);

    let events = vec![
        // Create ping program, top up balance and send init message.
        BlockRequestEvent::Router(RouterRequestEvent::ProgramCreated {
            actor_id: ping_id,
            code_id: ping_code_id,
        }),
        BlockRequestEvent::Mirror {
            actor_id: ping_id,
            event: MirrorRequestEvent::ExecutableBalanceTopUpRequested {
                value: 400_000_000_000,
            },
        },
        BlockRequestEvent::Mirror {
            actor_id: ping_id,
            event: MirrorRequestEvent::MessageQueueingRequested {
                id: get_next_message_id(),
                source: user_id,
                payload: b"PING".to_vec(),
                value: 0,
                call_reply: false,
            },
        },
        // Create async program, top up balance and send init message.
        BlockRequestEvent::Router(RouterRequestEvent::ProgramCreated {
            actor_id: async_id,
            code_id: async_code_id,
        }),
        BlockRequestEvent::Mirror {
            actor_id: async_id,
            event: MirrorRequestEvent::ExecutableBalanceTopUpRequested {
                value: 1_500_000_000_000,
            },
        },
        BlockRequestEvent::Mirror {
            actor_id: async_id,
            event: MirrorRequestEvent::MessageQueueingRequested {
                id: get_next_message_id(),
                source: user_id,
                payload: ping_id.encode(),
                value: 0,
                call_reply: false,
            },
        },
    ];

    let block1 = chain.blocks[1].to_simple();

    let executable_data = ExecutableData {
        block: block1,
        events,
        gas_allowance: Some(DEFAULT_BLOCK_GAS_LIMIT),
        ..Default::default()
    };

    // Process events
    let FinalizedBlockTransitions {
        states,
        schedule,
        program_creations,
        ..
    } = processor
        .process_programs(executable_data)
        .await
        .expect("failed to process events");
    program_creations.into_iter().for_each(|(pid, cid)| {
        processor.db.set_program_code_id(pid, cid);
    });

    // Check that program have empty queues
    let ping_mq = states.get(&ping_id).expect("ping state wasn't found");
    let async_mq = states.get(&async_id).expect("async state wasn't found");
    assert_eq!(ping_mq.canonical_queue_size, 0);
    assert_eq!(async_mq.canonical_queue_size, 0);
    assert_eq!(ping_mq.injected_queue_size, 0);
    assert_eq!(async_mq.injected_queue_size, 0);

    // -----------------------------------------------------------------------------
    // ------------------ Create a block with non-empty queues ---------------------
    // -----------------------------------------------------------------------------
    // This block won't be processed, but there will be messages saved into corresponding queues.
    // This is needed to test a case when RPC calculate reply for handle procedure is called when
    // programs already have some state.

    let block2 = chain.blocks[2].to_simple();
    let mut handler = ProcessingHandler::new(
        processor.db.clone(),
        InBlockTransitions::new(block2.header.height, states, schedule, Default::default()),
    );

    // Manually add messages to programs queues
    let ping_mid1 = get_next_message_id();
    handler
        .handle_mirror_event(
            ping_id,
            MirrorRequestEvent::MessageQueueingRequested {
                id: ping_mid1,
                source: user_id,
                payload: b"PING".to_vec(),
                value: 0,
                call_reply: false,
            },
        )
        .expect("failed to send message");

    let ping_mid2 = get_next_message_id();
    handler
        .handle_mirror_event(
            ping_id,
            MirrorRequestEvent::MessageQueueingRequested {
                id: ping_mid2,
                source: user_id,
                payload: b"PING".to_vec(),
                value: 0,
                call_reply: false,
            },
        )
        .expect("failed to send message");

    let async_mid1 = get_next_message_id();
    handler
        .handle_mirror_event(
            async_id,
            MirrorRequestEvent::MessageQueueingRequested {
                id: async_mid1,
                source: user_id,
                payload: demo_async::Command::Common.encode().encode(),
                value: 0,
                call_reply: false,
            },
        )
        .expect("failed to send message");

    let async_mid2 = get_next_message_id();
    handler
        .handle_mirror_event(
            async_id,
            MirrorRequestEvent::MessageQueueingRequested {
                id: async_mid2,
                source: user_id,
                payload: demo_async::Command::Common.encode().encode(),
                value: 0,
                call_reply: false,
            },
        )
        .expect("failed to send message");

    let async_mid3 = get_next_message_id();
    handler
        .handle_mirror_event(
            async_id,
            MirrorRequestEvent::MessageQueueingRequested {
                id: async_mid3,
                source: user_id,
                payload: demo_async::Command::Common.encode().encode(),
                value: 0,
                call_reply: false,
            },
        )
        .expect("failed to send message");

    // Handler ops wrote to the storage states of particular programs,
    // but block programs states are not updated yet. That the reason state hash
    // can't be obtained from the db.
    let ping_state_hash = handler
        .transitions
        .state_of(&ping_id)
        .expect("failed to get ping state");
    let ping_mq = get_mq_from_state_hash(ping_state_hash.hash, &processor)
        .expect("failed to get ping message queue");
    assert_eq!(ping_mq.len(), 2);

    let async_state_hash = handler
        .transitions
        .state_of(&async_id)
        .expect("failed to get async state");
    let async_mq = get_mq_from_state_hash(async_state_hash.hash, &processor)
        .expect("failed to get async message queue");
    assert_eq!(async_mq.len(), 3);

    // Finalize (from the ethexe-processor point of view) the block
    let FinalizedBlockTransitions { states, .. } = handler.transitions.finalize();

    // Same checks as above, but with obtaining states from db
    let ping_mq = states.get(&ping_id).expect("ping state wasn't found");
    let async_mq = states.get(&async_id).expect("async state wasn't found");
    assert_eq!(ping_mq.canonical_queue_size, 2);
    assert_eq!(async_mq.canonical_queue_size, 3);
    assert_eq!(ping_mq.injected_queue_size, 0);
    assert_eq!(async_mq.injected_queue_size, 0);

    // -----------------------------------------------------------------------------
    // ------------------------ Run in overlay a message ---------------------------
    // -----------------------------------------------------------------------------
    let block3 = chain.blocks[3].to_simple();

    // Send message using overlay on the block3.
    let mut overlaid_processor = processor.clone().overlaid();
    let executable = ExecutableDataForReply {
        block: block3,
        program_states: states,
        source: user_id,
        program_id: async_id,
        payload: demo_async::Command::Common.encode(),
        value: 0,
        gas_allowance: DEFAULT_BLOCK_GAS_LIMIT,
    };
    let reply_info = overlaid_processor
        .execute_for_reply(executable)
        .await
        .unwrap();
    assert_eq!(reply_info.payload, MessageId::zero().encode());
}

#[tokio::test(flavor = "multi_thread")]
async fn injected_ping_pong() {
    init_logger();

    let (mut processor, chain, [code_id]) = setup_test_env_and_load_codes([demo_ping::WASM_BINARY]);

    let user_1 = ActorId::from(10);
    let user_2 = ActorId::from(20);
    let actor_id = ActorId::from(0x10000);

    let mut handler = setup_handler(processor.db.clone(), chain.blocks[1].to_simple());

    handler
        .handle_router_event(RouterRequestEvent::ProgramCreated { actor_id, code_id })
        .expect("failed to create new program");

    handler
        .handle_mirror_event(
            actor_id,
            MirrorRequestEvent::ExecutableBalanceTopUpRequested {
                value: 200_000_000_000,
            },
        )
        .expect("failed to top up balance");

    handler
        .handle_mirror_event(
            actor_id,
            MirrorRequestEvent::MessageQueueingRequested {
                id: MessageId::from(1),
                source: user_1,
                payload: b"INIT".to_vec(),
                value: 0,
                call_reply: false,
            },
        )
        .expect("failed to send message");

    handler.transitions = processor
        .process_queues(
            handler.transitions,
            chain.blocks[1].to_simple(),
            DEFAULT_BLOCK_GAS_LIMIT,
        )
        .await;

    handler
        .handle_mirror_event(
            actor_id,
            MirrorRequestEvent::MessageQueueingRequested {
                id: MessageId::from(2),
                source: user_1,
                payload: b"PING".to_vec(),
                value: 0,
                call_reply: false,
            },
        )
        .expect("failed to send message");

    handler
        .handle_injected_transaction(user_2, injected(actor_id, b"PING", 0))
        .expect("failed to send message");

    handler.transitions = processor
        .process_queues(
            handler.transitions,
            chain.blocks[1].to_simple(),
            DEFAULT_BLOCK_GAS_LIMIT,
        )
        .await;

    let to_users = handler.transitions.current_messages();

    assert_eq!(to_users.len(), 3);
    let message = &to_users[0].1;
    assert_eq!(message.destination, user_1);
    assert_eq!(message.payload, b"");

    let message = &to_users[1].1;
    assert_eq!(message.destination, user_2);
    assert_eq!(message.payload, b"PONG");

    let message = &to_users[2].1;
    assert_eq!(message.destination, user_1);
    assert_eq!(message.payload, b"PONG");
}

#[cfg(debug_assertions)] // FIXME: test fails in release mode
#[tokio::test(flavor = "multi_thread")]
async fn injected_prioritized_over_canonical() {
    const MSG_NUM: usize = 100;
    const GAS_ALLOWANCE: u64 = 600_000_000;

    init_logger();

    let (mut processor, chain, [code_id]) = setup_test_env_and_load_codes([demo_ping::WASM_BINARY]);

    let canonical_user = ActorId::from(10);
    let injected_user = ActorId::from(20);
    let actor_id = ActorId::from(0x10000);

    let mut handler = setup_handler(processor.db.clone(), chain.blocks[1].to_simple());

    handler
        .handle_router_event(RouterRequestEvent::ProgramCreated { actor_id, code_id })
        .expect("failed to create new program");

    handler
        .handle_mirror_event(
            actor_id,
            MirrorRequestEvent::ExecutableBalanceTopUpRequested {
                value: 500_000_000_000_000,
            },
        )
        .expect("failed to top up balance");

    handler
        .handle_mirror_event(
            actor_id,
            MirrorRequestEvent::MessageQueueingRequested {
                id: H256::random().0.into(),
                source: canonical_user,
                payload: b"INIT".to_vec(),
                value: 0,
                call_reply: false,
            },
        )
        .expect("failed to send message");

    handler.transitions = processor
        .process_queues(
            handler.transitions,
            chain.blocks[1].to_simple(),
            GAS_ALLOWANCE,
        )
        .await;

    // Send canonical messages
    for _ in 0..MSG_NUM {
        handler
            .handle_mirror_event(
                actor_id,
                MirrorRequestEvent::MessageQueueingRequested {
                    id: H256::random().0.into(),
                    source: canonical_user,
                    payload: b"PING".to_vec(),
                    value: 0,
                    call_reply: false,
                },
            )
            .expect("failed to send message");
    }

    // Send injected messages
    for _ in 0..MSG_NUM {
        handler
            .handle_injected_transaction(injected_user, injected(actor_id, b"PING", 0))
            .expect("failed to send message");
    }

    let transitions = processor
        .process_queues(
            handler.transitions,
            chain.blocks[1].to_simple(),
            GAS_ALLOWANCE,
        )
        .await;

    // +_+_+ strange that pass without skipping init
    // Verify that injected messages were processed first
    let mut is_canonical_found = false;
    for (_, message) in transitions.current_messages() {
        if message.destination == canonical_user {
            is_canonical_found = true;
        } else if is_canonical_found && message.destination == injected_user {
            panic!("Canonical message processed before injected one");
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn executable_balance_charged() {
    init_logger();

    let (mut processor, chain, [code_id]) = setup_test_env_and_load_codes([demo_ping::WASM_BINARY]);
    let mut handler = setup_handler(processor.db.clone(), chain.blocks[1].to_simple());

    let user_id = ActorId::from(10);
    let actor_id = ActorId::from(0x10000);

    handler
        .handle_router_event(RouterRequestEvent::ProgramCreated { actor_id, code_id })
        .expect("failed to create new program");

    handler
        .handle_mirror_event(
            actor_id,
            MirrorRequestEvent::ExecutableBalanceTopUpRequested {
                value: 80_000_000_000,
            },
        )
        .expect("failed to top up balance");

    let exec_balance_before = handler.program_state(actor_id).executable_balance;
    assert_eq!(exec_balance_before, 80_000_000_000);

    handler
        .handle_mirror_event(
            actor_id,
            MirrorRequestEvent::MessageQueueingRequested {
                id: MessageId::from(1),
                source: user_id,
                payload: b"PING".to_vec(),
                value: 0,
                call_reply: false,
            },
        )
        .expect("failed to send message");

    handler.transitions = processor
        .process_queues(
            handler.transitions,
            chain.blocks[1].to_simple(),
            DEFAULT_BLOCK_GAS_LIMIT,
        )
        .await;

    let to_users = handler.transitions.current_messages();

    assert_eq!(to_users.len(), 1);

    let message = &to_users[0].1;
    assert_eq!(message.destination, user_id);
    assert_eq!(message.payload, b"PONG");

    // Check that executable balance decreased
    let exec_balance_after = handler.program_state(actor_id).executable_balance;
    assert!(exec_balance_after < exec_balance_before);

    handler
        .handle_injected_transaction(user_id, injected(actor_id, b"", 0))
        .expect("failed to send message");

    let to_users = handler.transitions.current_messages();

    assert_eq!(to_users.len(), 1);

    let message = &to_users[0].1;
    assert_eq!(message.destination, user_id);
    assert_eq!(message.payload, b"PONG");

    // Check that executable balance decreased on injected message as well
    let exec_balance_after = handler.program_state(actor_id).executable_balance;
    assert!(exec_balance_after < exec_balance_before);
}

#[tokio::test(flavor = "multi_thread")]
async fn executable_balance_injected_panic_not_charged() {
    // Testing special case when injected message causes panic in the program.
    // In this case executable balance should not be charged if gas burned during
    // panicked message execution is less than the threshold (see `INJECTED_MESSAGE_PANIC_GAS_CHARGE_THRESHOLD`).

    const INITIAL_EXECUTABLE_BALANCE: u128 = 150_000_000_000;

    init_logger();

    let (mut processor, chain, [code_id]) =
        setup_test_env_and_load_codes([demo_panic_payload::WASM_BINARY]);

    let user_id = ActorId::from(10);
    let actor_id = ActorId::from(0x10000);

    let mut handler = setup_handler(processor.db.clone(), chain.blocks[1].to_simple());

    handler
        .handle_router_event(RouterRequestEvent::ProgramCreated { actor_id, code_id })
        .expect("failed to create new program");

    handler
        .handle_mirror_event(
            actor_id,
            MirrorRequestEvent::ExecutableBalanceTopUpRequested {
                value: INITIAL_EXECUTABLE_BALANCE,
            },
        )
        .expect("failed to top up balance");

    let exec_balance_before = handler.program_state(actor_id).executable_balance;
    assert_eq!(exec_balance_before, INITIAL_EXECUTABLE_BALANCE);

    // Init message should not panic
    handler
        .handle_mirror_event(
            actor_id,
            MirrorRequestEvent::MessageQueueingRequested {
                id: H256::random().0.into(),
                source: user_id,
                payload: ActorId::zero().encode(),
                value: 0,
                call_reply: false,
            },
        )
        .unwrap();
    handler.transitions = processor
        .process_queues(
            handler.transitions,
            chain.blocks[1].to_simple(),
            DEFAULT_BLOCK_GAS_LIMIT,
        )
        .await;
    let init_balance = handler.program_state(actor_id).executable_balance;

    // We know for sure handling this message is cost less than the threshold.
    // This message will cause panic in the program.
    handler
        .handle_injected_transaction(user_id, injected(actor_id, b"", 0))
        .unwrap();
    handler.transitions = processor
        .process_queues(
            handler.transitions,
            chain.blocks[1].to_simple(),
            DEFAULT_BLOCK_GAS_LIMIT,
        )
        .await;

    let to_users = handler.transitions.current_messages();
    assert_eq!(to_users.len(), 2);

    let message = &to_users[1].1;
    assert_eq!(message.destination, user_id);
    // Check that panic indeed happened
    assert_eq!(&message.payload[..3], b"\xE0\x80\x80");

    // Check that executable balance is unchanged
    let exec_balance_after = handler.program_state(actor_id).executable_balance;
    assert_eq!(exec_balance_after, init_balance);

    // Send canonical message to make sure executable balance is charged in panic case.
    handler
        .handle_mirror_event(
            actor_id,
            MirrorRequestEvent::MessageQueueingRequested {
                id: MessageId::from(3),
                source: user_id,
                payload: vec![],
                value: 0,
                call_reply: false,
            },
        )
        .expect("failed to send message");
    let transitions = processor
        .process_queues(
            handler.into_transitions(),
            chain.blocks[1].to_simple(),
            DEFAULT_BLOCK_GAS_LIMIT,
        )
        .await;

    let to_users = transitions.current_messages();
    assert_eq!(to_users.len(), 3);

    let message = &to_users[2].1;
    assert_eq!(message.destination, user_id);
    // Check that panic indeed happened
    assert_eq!(&message.payload[..3], b"\xE0\x80\x80");

    // Check that executable balance decreased on canonical message
    let exec_balance_after = ProcessingHandler::new(processor.db.clone(), transitions)
        .program_state(actor_id)
        .executable_balance;
    assert!(exec_balance_after < init_balance);
}

#[tokio::test(flavor = "multi_thread")]
async fn insufficient_executable_balance_still_charged() {
    // Just enough balance to charge for instrumentation and instantiation but not enough to process the message.
    const INSUFFICIENT_EXECUTABLE_BALANCE: u128 = 30_000_000_000;

    init_logger();

    let (mut processor, chain, [code_id]) = setup_test_env_and_load_codes([demo_ping::WASM_BINARY]);
    let mut handler = setup_handler(processor.db.clone(), chain.blocks[1].to_simple());

    let user_id = ActorId::from(10);
    let actor_id = ActorId::from(0x10000);

    handler
        .handle_router_event(RouterRequestEvent::ProgramCreated { actor_id, code_id })
        .expect("failed to create new program");

    handler
        .handle_mirror_event(
            actor_id,
            MirrorRequestEvent::ExecutableBalanceTopUpRequested {
                value: INSUFFICIENT_EXECUTABLE_BALANCE,
            },
        )
        .expect("failed to top up balance");

    // Should fail due to insufficient balance (ran out of gas)
    handler
        .handle_mirror_event(
            actor_id,
            MirrorRequestEvent::MessageQueueingRequested {
                id: MessageId::from(1),
                source: user_id,
                payload: b"PING".to_vec(),
                value: 0,
                call_reply: false,
            },
        )
        .expect("failed to send message");

    handler.transitions = processor
        .process_queues(
            handler.transitions,
            chain.blocks[1].to_simple(),
            DEFAULT_BLOCK_GAS_LIMIT,
        )
        .await;

    let to_users = handler.transitions.current_messages();
    assert_eq!(to_users.len(), 1);

    // Check that message processing failed due to insufficient balance (ran out of gas)
    let message = &to_users[0].1;
    assert_eq!(message.destination, user_id);
    assert!(message.reply_details.unwrap().to_reply_code().is_error());

    // Check that executable balance decreased
    let exec_balance_after = handler.program_state(actor_id).executable_balance;
    assert!(exec_balance_after < INSUFFICIENT_EXECUTABLE_BALANCE);
}
