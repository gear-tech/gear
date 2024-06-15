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

use crate::*;
use gear_core::{ids::prelude::CodeIdExt, message::DispatchKind};
use gprimitives::{ActorId, MessageId};
use hypercore_db::{BlockInfo, MemDb};
use hypercore_ethereum::event::{CreateProgram, SendMessage};
use std::collections::BTreeMap;
use utils::*;
use wabt::wat2wasm;

fn init_new_block(processor: &mut Processor, height: u32, timestamp: u64) -> H256 {
    let chain_head = H256::random();
    processor
        .db
        .set_block_info(chain_head, BlockInfo { height, timestamp });
    processor.creator.set_chain_head(chain_head);
    chain_head
}

#[test]
fn process_observer_event() {
    init_logger();

    let db = MemDb::default();
    let mut processor =
        Processor::new(Database::from_one(&db)).expect("failed to create processor");

    let ch0 = init_new_block(&mut processor, 0, 0);

    processor.db.set_block_program_hashes(ch0, BTreeMap::new());

    let code = demo_ping::WASM_BINARY.to_vec();
    let code_id = CodeId::generate(&code);

    let event = Event::UploadCode {
        origin: ActorId::zero(),
        code_id,
        code,
    };

    let outcomes = processor
        .process_observer_event(&event)
        .expect("failed to process observer event");
    log::debug!("\n\nUpload code outcomes: {outcomes:?}\n\n");

    assert_eq!(outcomes, vec![LocalOutcome::CodeApproved(code_id)]);

    let ch1 = init_new_block(&mut processor, 1, 12);

    let create_program_event = BlockEvent::CreateProgram(CreateProgram {
        origin: H256::random().0.into(),
        actor_id: ActorId::from(42),
        code_id,
        init_payload: b"PING".to_vec(),
        gas_limit: 1_000_000_000,
        value: 0,
    });

    let event = Event::Block {
        parent_hash: ch0,
        block_hash: ch1,
        events: vec![create_program_event],
    };

    let outcomes = processor
        .process_observer_event(&event)
        .expect("failed to process observer event");
    log::debug!("\n\nCreate program outcomes: {outcomes:?}\n\n");

    let ch2 = init_new_block(&mut processor, 2, 24);

    let send_message_event = BlockEvent::SendMessage(SendMessage {
        origin: H256::random().0.into(),
        destination: ActorId::from(42),
        payload: b"PING".to_vec(),
        gas_limit: 1_000_000_000,
        value: 0,
    });

    let event = Event::Block {
        parent_hash: ch1,
        block_hash: ch2,
        events: vec![send_message_event],
    };

    let outcomes = processor
        .process_observer_event(&event)
        .expect("failed to process observer event");
    log::debug!("\n\nSend message outcomes: {outcomes:?}\n\n");
}

#[test]
fn handle_new_code_valid() {
    init_logger();

    let db = MemDb::default();
    let mut processor =
        Processor::new(Database::from_one(&db)).expect("failed to create processor");

    init_new_block(&mut processor, 0, 0);

    let (code_id, original_code) = utils::wat_to_wasm(utils::VALID_PROGRAM);
    let original_code_len = original_code.len();

    assert!(processor.db.read_original_code(code_id).is_none());
    assert!(processor
        .db
        .read_instrumented_code(hypercore_runtime::VERSION, code_id)
        .is_none());

    let calculated_id = processor
        .handle_new_code(original_code.clone())
        .expect("failed to call runtime api")
        .expect("code failed verification or instrumentation");

    assert_eq!(calculated_id, code_id);

    assert_eq!(
        processor
            .db
            .read_original_code(code_id)
            .expect("failed to read original code"),
        original_code
    );
    assert!(
        processor
            .db
            .read_instrumented_code(hypercore_runtime::VERSION, code_id)
            .expect("failed to read original code")
            .code()
            .len()
            > original_code_len
    );
}

#[test]
fn handle_new_code_invalid() {
    init_logger();

    let db = MemDb::default();
    let mut processor =
        Processor::new(Database::from_one(&db)).expect("failed to create processor");

    init_new_block(&mut processor, 0, 0);

    let (code_id, original_code) = utils::wat_to_wasm(utils::INVALID_PROGRAM);

    assert!(processor.db.read_original_code(code_id).is_none());
    assert!(processor
        .db
        .read_instrumented_code(hypercore_runtime::VERSION, code_id)
        .is_none());

    assert!(processor
        .handle_new_code(original_code.clone())
        .expect("failed to call runtime api")
        .is_none());

    assert!(processor.db.read_original_code(code_id).is_none());
    assert!(processor
        .db
        .read_instrumented_code(hypercore_runtime::VERSION, code_id)
        .is_none());
}

#[test]
fn host_ping_pong() {
    init_logger();

    let db = MemDb::default();
    let mut processor = Processor::new(Database::from_one(&db)).unwrap();

    init_new_block(&mut processor, 0, 0);

    let program_id = 42.into();

    let code_id = processor
        .handle_new_code(demo_ping::WASM_BINARY)
        .expect("failed to call runtime api")
        .expect("code failed verification or instrumentation");

    let state_hash = processor
        .handle_new_program(program_id, code_id)
        .expect("failed to create new program");

    let state_hash = processor
        .handle_user_message(state_hash, vec![create_message(DispatchKind::Init, "PING")])
        .expect("failed to populate message queue");

    let _init = processor.run_on_host(program_id, state_hash).unwrap();
}

#[test]
fn ping_pong() {
    init_logger();

    let db = MemDb::default();
    let mut processor = Processor::new(Database::from_one(&db)).unwrap();

    init_new_block(&mut processor, 0, 0);

    let user_id = ActorId::from(10);
    let program_id = ProgramId::from(0x10000);

    let code_id = processor
        .handle_new_code(demo_ping::WASM_BINARY)
        .expect("failed to call runtime api")
        .expect("code failed verification or instrumentation");

    let state_hash = processor
        .handle_new_program(program_id, code_id)
        .expect("failed to create new program");

    let state_hash = processor
        .handle_user_message(
            state_hash,
            vec![
                create_message_full(MessageId::from(1), DispatchKind::Init, user_id, "PING"),
                create_message_full(MessageId::from(2), DispatchKind::Handle, user_id, "PING"),
            ],
        )
        .expect("failed to populate message queue");

    let mut programs = BTreeMap::from_iter([(program_id, state_hash)]);

    let (to_users, _) = run::run(8, processor.creator.clone(), &mut programs);

    assert_eq!(to_users.len(), 2);

    let message = &to_users[0];
    assert_eq!(message.destination(), user_id);
    assert_eq!(message.payload_bytes(), b"PONG");

    let message = &to_users[1];
    assert_eq!(message.destination(), user_id);
    assert_eq!(message.payload_bytes(), b"PONG");
}

fn create_message(kind: DispatchKind, payload: impl AsRef<[u8]>) -> UserMessage {
    create_message_full(H256::random().into(), kind, H256::random().into(), payload)
}

fn create_message_full(
    id: MessageId,
    kind: DispatchKind,
    source: ActorId,
    payload: impl AsRef<[u8]>,
) -> UserMessage {
    UserMessage {
        id,
        kind,
        source,
        payload: payload.as_ref().to_vec(),
        gas_limit: 1_000_000_000,
        value: 0,
    }
}

#[test]
fn async_and_ping() {
    init_logger();

    let mut message_nonce: u64 = 0;
    let mut get_next_message_id = || {
        message_nonce += 1;
        MessageId::from(message_nonce)
    };
    let user_id = ActorId::from(10);

    let db = MemDb::default();
    let mut processor = Processor::new(Database::from_one(&db)).unwrap();

    init_new_block(&mut processor, 0, 0);

    let ping_id = ProgramId::from(0x10000000);
    let async_id = ProgramId::from(0x20000000);

    let ping_code_id = processor
        .handle_new_code(demo_ping::WASM_BINARY)
        .expect("failed to call runtime api")
        .expect("code failed verification or instrumentation");

    let upload_code_id = processor
        .handle_new_code(demo_async::WASM_BINARY)
        .expect("failed to call runtime api")
        .expect("code failed verification or instrumentation");

    let ping_state_hash = processor
        .handle_new_program(ping_id, ping_code_id)
        .expect("failed to create new program");
    let ping_state_hash = processor
        .handle_user_message(
            ping_state_hash,
            vec![UserMessage {
                id: get_next_message_id(),
                kind: DispatchKind::Init,
                source: user_id,
                payload: b"PING".to_vec(),
                gas_limit: 10_000_000_000,
                value: 0,
            }],
        )
        .expect("failed to populate message queue");

    let async_state_hash = processor
        .handle_new_program(async_id, upload_code_id)
        .expect("failed to create new program");
    let async_state_hash = processor
        .handle_user_message(
            async_state_hash,
            vec![UserMessage {
                id: get_next_message_id(),
                kind: DispatchKind::Init,
                source: user_id,
                payload: ping_id.encode(),
                gas_limit: 10_000_000_000,
                value: 0,
            }],
        )
        .expect("failed to populate message queue");

    let wait_for_reply_to = get_next_message_id();
    let async_state_hash = processor
        .handle_user_message(
            async_state_hash,
            vec![UserMessage {
                id: wait_for_reply_to,
                kind: DispatchKind::Handle,
                source: user_id,
                payload: demo_async::Command::Common.encode(),
                gas_limit: 10_000_000_000,
                value: 0,
            }],
        )
        .expect("failed to populate message queue");

    let mut programs =
        BTreeMap::from_iter([(ping_id, ping_state_hash), (async_id, async_state_hash)]);

    let (to_users, _) = run::run(8, processor.creator.clone(), &mut programs);

    assert_eq!(to_users.len(), 3);

    let message = &to_users[0];
    assert_eq!(message.destination(), user_id);
    assert_eq!(message.payload_bytes(), b"PONG");

    let message = &to_users[1];
    assert_eq!(message.destination(), user_id);
    assert_eq!(message.payload_bytes(), b"");

    let message = &to_users[2];
    assert_eq!(message.destination(), user_id);
    assert_eq!(
        message.payload_bytes(),
        wait_for_reply_to.into_bytes().as_slice()
    );
}

#[test]
fn many_waits() {
    init_logger();

    let threads_amount = 8;

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

    let db = MemDb::default();
    let mut processor = Processor::new(Database::from_one(&db)).unwrap();

    init_new_block(&mut processor, 0, 0);

    let code_id = processor
        .handle_new_code(code)
        .expect("failed to call runtime api")
        .expect("code failed verification or instrumentation");

    let amount = 10000;
    let mut programs = BTreeMap::new();
    for i in 0..amount {
        let program_id = ProgramId::from(i);

        let state_hash = processor
            .handle_new_program(program_id, code_id)
            .expect("failed to create new program");
        let state_hash = processor
            .handle_user_message(state_hash, vec![create_message(DispatchKind::Init, b"")])
            .expect("failed to populate message queue");

        programs.insert(program_id, state_hash);
    }

    let (to_users, _) = run::run(threads_amount, processor.creator.clone(), &mut programs);
    assert_eq!(to_users.len(), amount as usize);

    for (_pid, state_hash) in programs.iter_mut() {
        let new_state_hash = processor
            .handle_user_message(*state_hash, vec![create_message(DispatchKind::Handle, b"")])
            .expect("failed to populate message queue");
        *state_hash = new_state_hash;
    }

    let (to_users, _) = run::run(threads_amount, processor.creator.clone(), &mut programs);
    assert_eq!(to_users.len(), 0);

    init_new_block(&mut processor, 11, 11);

    let (to_users, _) = run::run(threads_amount, processor.creator.clone(), &mut programs);

    assert_eq!(to_users.len(), amount as usize);

    for message in to_users {
        assert_eq!(message.payload_bytes(), b"Hello, world!");
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
        let _ = env_logger::Builder::from_default_env()
            .format_module_path(false)
            .format_level(true)
            .try_init();
    }

    pub fn wat_to_wasm(wat: &str) -> (CodeId, Vec<u8>) {
        let code = wat2wasm(wat).expect("failed to parse wat to bin");
        let code_id = CodeId::generate(&code);

        (code_id, code)
    }
}
