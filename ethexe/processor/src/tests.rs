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
use ethexe_common::{
    BlockHeader, HashOf,
    db::*,
    events::{BlockRequestEvent, MirrorRequestEvent, RouterRequestEvent},
};
use ethexe_runtime_common::ScheduleRestorer;
use gear_core::ids::prelude::CodeIdExt;
use gprimitives::{ActorId, MessageId};
use parity_scale_codec::Encode;
use utils::*;

fn init_genesis_block(processor: &mut Processor) -> H256 {
    let genesis_block_hash = init_new_block(processor, Default::default());

    // Set zero hash announce for genesis block (genesis announce hash)
    let genesis_announce_hash = HashOf::zero();

    processor
        .db
        .set_announce_program_states(genesis_announce_hash, Default::default());
    processor
        .db
        .set_announce_schedule(genesis_announce_hash, Default::default());

    genesis_block_hash
}

fn init_new_block(processor: &mut Processor, header: BlockHeader) -> H256 {
    let chain_head = H256::random();
    processor.db.set_block_header(chain_head, header);
    processor.creator.set_chain_head(chain_head);
    chain_head
}

#[track_caller]
fn init_new_block_from_parent(processor: &mut Processor, parent_hash: H256) -> H256 {
    let parent_block_header = processor.db.block_header(parent_hash).unwrap_or_default();
    let height = parent_block_header.height + 1;
    let timestamp = parent_block_header.timestamp + 12;

    init_new_block(
        processor,
        BlockHeader {
            height,
            timestamp,
            parent_hash,
        },
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn process_observer_event() {
    init_logger();

    let mut processor = Processor::new(Database::memory()).expect("failed to create processor");

    let genesis = init_genesis_block(&mut processor);
    let block1 = init_new_block_from_parent(&mut processor, genesis);

    let code = demo_ping::WASM_BINARY.to_vec();
    let code_id = CodeId::generate(&code);
    let code_and_id = CodeAndIdUnchecked { code, code_id };

    let valid = processor
        .process_upload_code(code_and_id)
        .expect("failed to upload code");
    assert!(valid);

    let block1_announce = Announce::with_default_gas(block1, HashOf::zero());
    let block1_announce_hash = block1_announce.to_hash();

    // Process and save results
    let BlockProcessingResult {
        states, schedule, ..
    } = processor
        .process_announce(block1_announce, vec![])
        .await
        .unwrap();
    processor
        .db
        .set_announce_program_states(block1_announce_hash, states);
    processor
        .db
        .set_announce_schedule(block1_announce_hash, schedule);

    let block2 = init_new_block_from_parent(&mut processor, block1);

    let actor_id = ActorId::from(42);

    let create_program_events = vec![
        BlockRequestEvent::Router(RouterRequestEvent::ProgramCreated { actor_id, code_id }),
        BlockRequestEvent::mirror(
            actor_id,
            MirrorRequestEvent::ExecutableBalanceTopUpRequested {
                value: 10_000_000_000,
            },
        ),
        BlockRequestEvent::mirror(
            actor_id,
            MirrorRequestEvent::MessageQueueingRequested {
                id: H256::random().0.into(),
                source: H256::random().0.into(),
                payload: b"PING".to_vec(),
                value: 0,
                call_reply: false,
            },
        ),
    ];

    let block2_announce = Announce::with_default_gas(block2, block1_announce_hash);
    let block2_announce_hash = block2_announce.to_hash();

    // Process block2 announce and save results
    let BlockProcessingResult {
        states, schedule, ..
    } = processor
        .process_announce(block2_announce, create_program_events)
        .await
        .expect("failed to process create program");
    processor
        .db
        .set_announce_program_states(block2_announce_hash, states);
    processor
        .db
        .set_announce_schedule(block2_announce_hash, schedule);

    let block3 = init_new_block_from_parent(&mut processor, block2);

    let send_message_event = BlockRequestEvent::mirror(
        actor_id,
        MirrorRequestEvent::MessageQueueingRequested {
            id: H256::random().0.into(),
            source: H256::random().0.into(),
            payload: b"PING".to_vec(),
            value: 0,
            call_reply: false,
        },
    );

    let block3_announce = Announce::with_default_gas(block3, block2_announce_hash);

    // Process block3 announce
    processor
        .process_announce(block3_announce, vec![send_message_event])
        .await
        .expect("failed to process send message");
}

#[test]
fn handle_new_code_valid() {
    init_logger();

    let mut processor = Processor::new(Database::memory()).expect("failed to create processor");

    init_genesis_block(&mut processor);

    let (code_id, original_code) = utils::wat_to_wasm(utils::VALID_PROGRAM);
    let original_code_len = original_code.len();

    assert!(processor.db.original_code(code_id).is_none());
    assert!(
        processor
            .db
            .instrumented_code(ethexe_runtime_common::VERSION, code_id)
            .is_none()
    );

    assert!(processor.db.code_metadata(code_id).is_none());

    let calculated_id = processor
        .handle_new_code(&original_code)
        .expect("failed to call runtime api")
        .expect("code failed verification or instrumentation");

    assert_eq!(calculated_id, code_id);

    assert_eq!(
        processor
            .db
            .original_code(code_id)
            .expect("failed to read original code"),
        original_code
    );

    assert!(
        processor
            .db
            .instrumented_code(ethexe_runtime_common::VERSION, code_id)
            .expect("failed to read instrumented code")
            .bytes()
            .len()
            > original_code_len
    );

    assert_eq!(
        processor
            .db
            .code_metadata(code_id)
            .expect("failed to read code metadata")
            .original_code_len(),
        original_code_len as u32
    );
}

#[test]
fn handle_new_code_invalid() {
    init_logger();

    let mut processor = Processor::new(Database::memory()).expect("failed to create processor");

    init_genesis_block(&mut processor);

    let (code_id, original_code) = utils::wat_to_wasm(utils::INVALID_PROGRAM);

    assert!(processor.db.original_code(code_id).is_none());
    assert!(
        processor
            .db
            .instrumented_code(ethexe_runtime_common::VERSION, code_id)
            .is_none()
    );

    assert!(processor.db.code_metadata(code_id).is_none());

    assert!(
        processor
            .handle_new_code(&original_code)
            .expect("failed to call runtime api")
            .is_none()
    );

    assert!(processor.db.original_code(code_id).is_none());
    assert!(
        processor
            .db
            .instrumented_code(ethexe_runtime_common::VERSION, code_id)
            .is_none()
    );

    assert!(processor.db.code_metadata(code_id).is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn ping_pong() {
    init_logger();

    let mut processor = Processor::new(Database::memory()).unwrap();

    let genesis = init_genesis_block(&mut processor);
    let block = init_new_block_from_parent(&mut processor, genesis);
    let block_announce = Announce::with_default_gas(block, HashOf::zero());

    let user_id = ActorId::from(10);
    let actor_id = ActorId::from(0x10000);

    let code_id = processor
        .handle_new_code(demo_ping::WASM_BINARY)
        .expect("failed to call runtime api")
        .expect("code failed verification or instrumentation");

    let mut handler = processor.handler(block_announce).unwrap();

    handler
        .handle_router_event(RouterRequestEvent::ProgramCreated { actor_id, code_id })
        .expect("failed to create new program");

    handler
        .handle_mirror_event(
            actor_id,
            MirrorRequestEvent::ExecutableBalanceTopUpRequested {
                value: 10_000_000_000,
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

    processor.process_queue(&mut handler).await;

    let to_users = handler.transitions.current_messages();

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
    let user_id = ActorId::from(10);

    let mut processor = Processor::new(Database::memory()).unwrap();

    let genesis = init_genesis_block(&mut processor);
    let block = init_new_block_from_parent(&mut processor, genesis);
    let block_announce = Announce::with_default_gas(block, HashOf::zero());

    let ping_id = ActorId::from(0x10000000);
    let async_id = ActorId::from(0x20000000);

    let ping_code_id = processor
        .handle_new_code(demo_ping::WASM_BINARY)
        .expect("failed to call runtime api")
        .expect("code failed verification or instrumentation");

    let upload_code_id = processor
        .handle_new_code(demo_async::WASM_BINARY)
        .expect("failed to call runtime api")
        .expect("code failed verification or instrumentation");

    let mut handler = processor.handler(block_announce).unwrap();

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
                value: 10_000_000_000,
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
                value: 10_000_000_000,
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

    processor.process_queue(&mut handler).await;

    let to_users = handler.transitions.current_messages();

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

    let wat = r#"
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
                        (call $wait_for (i32.const 10))
                    )
                    (else
                        (call $reply (i32.const 0) (i32.const 13) (i32.const 0x400) (i32.const 0x600))
                    )
                )
            )
            (data (i32.const 0) "Hello, world!")
        )
    "#;

    let (_, code) = wat_to_wasm(wat);

    let mut processor = Processor::new(Database::memory()).unwrap();

    let genesis = init_genesis_block(&mut processor);
    let block1 = init_new_block_from_parent(&mut processor, genesis);
    let block1_announce = Announce::with_default_gas(block1, HashOf::zero());
    let block1_announce_hash = block1_announce.to_hash();

    let code_id = processor
        .handle_new_code(code)
        .expect("failed to call runtime api")
        .expect("code failed verification or instrumentation");

    let mut handler = processor.handler(block1_announce).unwrap();

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
                    value: 10_000_000_000,
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

    handler.run_schedule();
    processor.process_queue(&mut handler).await;

    assert_eq!(
        handler.transitions.current_messages().len(),
        amount as usize
    );

    for pid in handler.transitions.known_programs() {
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

    processor.process_queue(&mut handler).await;

    // unchanged
    assert_eq!(
        handler.transitions.current_messages().len(),
        amount as usize
    );

    let (_outcomes, states, schedule) = handler.transitions.finalize();
    processor
        .db
        .set_announce_program_states(block1_announce_hash, states);
    processor
        .db
        .set_announce_schedule(block1_announce_hash, schedule);

    let mut block = block1;
    let mut block_announce_hash = block1_announce_hash;
    for _ in 0..9 {
        block = init_new_block_from_parent(&mut processor, block);
        let block_announce = Announce::with_default_gas(block, block_announce_hash);
        let parent_announce_hash = block_announce_hash;
        block_announce_hash = block_announce.to_hash();

        let states = processor
            .db
            .announce_program_states(parent_announce_hash)
            .unwrap();
        processor
            .db
            .set_announce_program_states(block_announce_hash, states);
        let schedule = processor
            .db
            .announce_schedule(parent_announce_hash)
            .unwrap();
        processor
            .db
            .set_announce_schedule(block_announce_hash, schedule);
    }

    let block12 = init_new_block_from_parent(&mut processor, block);
    let block12_announce = Announce::with_default_gas(block12, block_announce_hash);

    let states = processor
        .db
        .announce_program_states(block_announce_hash)
        .unwrap();
    let schedule = processor.db.announce_schedule(block_announce_hash).unwrap();

    // Reproducibility test.
    let restored_schedule = ScheduleRestorer::from_storage(&processor.db, &states, 0)
        .unwrap()
        .restore();
    // This could fail in case of handling more scheduled ops: please, update test than.
    assert_eq!(schedule, restored_schedule);

    let mut handler = processor.handler(block12_announce).unwrap();
    handler.run_schedule();
    processor.process_queue(&mut handler).await;

    assert_eq!(
        handler.transitions.current_messages().len(),
        amount as usize
    );

    for (_pid, message) in handler.transitions.current_messages() {
        assert_eq!(message.payload, b"Hello, world!");
    }
}

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
}
