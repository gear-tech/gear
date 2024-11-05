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
use ethexe_common::{
    mirror::RequestEvent as MirrorEvent, router::RequestEvent as RouterEvent, BlockRequestEvent,
};
use ethexe_db::{BlockHeader, BlockMetaStorage, CodesStorage, MemDb, ScheduledTask};
use ethexe_runtime_common::state::{ComplexStorage, Dispatch};
use gear_core::{
    ids::{prelude::CodeIdExt, ProgramId},
    message::DispatchKind,
};
use gprimitives::{ActorId, MessageId};
use parity_scale_codec::Encode;
use std::collections::{BTreeMap, BTreeSet};
use utils::*;
use wabt::wat2wasm;

fn init_new_block(processor: &mut Processor, meta: BlockHeader) -> H256 {
    let chain_head = H256::random();
    processor.db.set_block_header(chain_head, meta);
    processor
        .db
        .set_block_start_program_states(chain_head, Default::default());
    processor
        .db
        .set_block_start_schedule(chain_head, Default::default());
    processor.creator.set_chain_head(chain_head);
    chain_head
}

#[track_caller]
fn init_new_block_from_parent(processor: &mut Processor, parent_hash: H256) -> H256 {
    let parent_block_header = processor.db.block_header(parent_hash).unwrap_or_default();
    let height = parent_block_header.height + 1;
    let timestamp = parent_block_header.timestamp + 12;
    let chain_head = init_new_block(
        processor,
        BlockHeader {
            height,
            timestamp,
            parent_hash,
        },
    );

    let parent_out_program_hashes = processor
        .db
        .block_end_program_states(parent_hash)
        .unwrap_or_else(|| {
            if parent_hash.is_zero() {
                Default::default()
            } else {
                panic!("process block events before new block; start states not found")
            }
        });
    processor
        .db
        .set_block_start_program_states(chain_head, parent_out_program_hashes);

    let parent_out_schedule = processor
        .db
        .block_end_schedule(parent_hash)
        .unwrap_or_else(|| {
            if parent_hash.is_zero() {
                Default::default()
            } else {
                panic!("process block events before new block; start schedule not found")
            }
        });
    processor
        .db
        .set_block_start_schedule(chain_head, parent_out_schedule);

    chain_head
}

#[test]
fn process_observer_event() {
    init_logger();

    let db = MemDb::default();
    let mut processor = Processor::new(Database::from_one(&db, Default::default()))
        .expect("failed to create processor");

    let ch0 = init_new_block_from_parent(&mut processor, Default::default());

    let code = demo_ping::WASM_BINARY.to_vec();
    let code_id = CodeId::generate(&code);

    let outcomes = processor
        .process_upload_code(code_id, &code)
        .expect("failed to upload code");
    log::debug!("\n\nUpload code outcomes: {outcomes:?}\n\n");
    assert_eq!(
        outcomes,
        vec![LocalOutcome::CodeValidated {
            id: code_id,
            valid: true
        }]
    );

    let _ = processor.process_block_events(ch0, vec![]).unwrap();
    let ch1 = init_new_block_from_parent(&mut processor, ch0);

    let actor_id = ActorId::from(42);

    let create_program_events = vec![
        BlockRequestEvent::Router(RouterEvent::ProgramCreated { actor_id, code_id }),
        BlockRequestEvent::mirror(
            actor_id,
            MirrorEvent::ExecutableBalanceTopUpRequested {
                value: 10_000_000_000,
            },
        ),
        BlockRequestEvent::mirror(
            actor_id,
            MirrorEvent::MessageQueueingRequested {
                id: H256::random().0.into(),
                source: H256::random().0.into(),
                payload: b"PING".to_vec(),
                value: 0,
            },
        ),
    ];

    let outcomes = processor
        .process_block_events(ch1, create_program_events)
        .expect("failed to process create program");

    log::debug!("\n\nCreate program outcomes: {outcomes:?}\n\n");

    let ch2 = init_new_block_from_parent(&mut processor, ch1);

    let send_message_event = BlockRequestEvent::mirror(
        actor_id,
        MirrorEvent::MessageQueueingRequested {
            id: H256::random().0.into(),
            source: H256::random().0.into(),
            payload: b"PING".to_vec(),
            value: 0,
        },
    );

    let outcomes = processor
        .process_block_events(ch2, vec![send_message_event])
        .expect("failed to process send message");

    log::debug!("\n\nSend message outcomes: {outcomes:?}\n\n");
}

#[test]
fn handle_new_code_valid() {
    init_logger();

    let db = MemDb::default();
    let mut processor = Processor::new(Database::from_one(&db, Default::default()))
        .expect("failed to create processor");

    init_new_block(&mut processor, Default::default());

    let (code_id, original_code) = utils::wat_to_wasm(utils::VALID_PROGRAM);
    let original_code_len = original_code.len();

    assert!(processor.db.original_code(code_id).is_none());
    assert!(processor
        .db
        .instrumented_code(ethexe_runtime::VERSION, code_id)
        .is_none());

    assert!(processor.db.code_metadata(code_id).is_none());

    let calculated_id = processor
        .handle_new_code(original_code.clone())
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
            .instrumented_code(ethexe_runtime::VERSION, code_id)
            .expect("failed to read original code")
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

    let db = MemDb::default();
    let mut processor = Processor::new(Database::from_one(&db, Default::default()))
        .expect("failed to create processor");

    init_new_block(&mut processor, Default::default());

    let (code_id, original_code) = utils::wat_to_wasm(utils::INVALID_PROGRAM);

    assert!(processor.db.original_code(code_id).is_none());
    assert!(processor
        .db
        .instrumented_code(ethexe_runtime::VERSION, code_id)
        .is_none());

    assert!(processor.db.code_metadata(code_id).is_none());

    assert!(processor
        .handle_new_code(original_code.clone())
        .expect("failed to call runtime api")
        .is_none());

    assert!(processor.db.original_code(code_id).is_none());
    assert!(processor
        .db
        .instrumented_code(ethexe_runtime::VERSION, code_id)
        .is_none());

    assert!(processor.db.code_metadata(code_id).is_none());
}

#[test]
fn ping_pong() {
    init_logger();

    let db = MemDb::default();
    let mut processor = Processor::new(Database::from_one(&db, Default::default())).unwrap();

    init_new_block(&mut processor, Default::default());

    let user_id = ActorId::from(10);
    let program_id = ProgramId::from(0x10000);

    let code_id = processor
        .handle_new_code(demo_ping::WASM_BINARY)
        .expect("failed to call runtime api")
        .expect("code failed verification or instrumentation");

    processor
        .handle_new_program(program_id, code_id)
        .expect("failed to create new program");

    let state_hash = processor
        .handle_executable_balance_top_up(H256::zero(), 10_000_000_000)
        .expect("failed to top up balance");

    let messages = vec![
        create_message_full(
            &mut processor,
            MessageId::from(1),
            DispatchKind::Init,
            user_id,
            "PING",
        ),
        create_message_full(
            &mut processor,
            MessageId::from(2),
            DispatchKind::Handle,
            user_id,
            "PING",
        ),
    ];
    let state_hash = processor
        .handle_messages_queueing(state_hash, messages)
        .expect("failed to populate message queue");

    let states = BTreeMap::from_iter([(program_id, state_hash)]);
    let mut in_block_transitions =
        InBlockTransitions::new(BlockHeader::dummy(0), states, Default::default());

    run::run(
        8,
        processor.db.clone(),
        processor.creator.clone(),
        &mut in_block_transitions,
    );

    let to_users = in_block_transitions.current_messages();

    assert_eq!(to_users.len(), 2);

    let message = &to_users[0].1;
    assert_eq!(message.destination, user_id);
    assert_eq!(message.payload, b"PONG");

    let message = &to_users[1].1;
    assert_eq!(message.destination, user_id);
    assert_eq!(message.payload, b"PONG");
}

#[allow(unused)]
fn create_message(
    processor: &mut Processor,
    kind: DispatchKind,
    payload: impl AsRef<[u8]>,
) -> Dispatch {
    create_message_full(
        processor,
        H256::random().into(),
        kind,
        H256::random().into(),
        payload,
    )
}

fn create_message_full(
    processor: &mut Processor,
    id: MessageId,
    kind: DispatchKind,
    source: ActorId,
    payload: impl AsRef<[u8]>,
) -> Dispatch {
    let payload = payload.as_ref().to_vec();
    let payload_hash = processor.db.store_payload(payload).unwrap();

    Dispatch {
        id,
        kind,
        source,
        payload_hash,
        value: 0,
        details: None,
        context: None,
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
    let mut processor = Processor::new(Database::from_one(&db, Default::default())).unwrap();

    init_new_block(&mut processor, Default::default());

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

    processor
        .handle_new_program(ping_id, ping_code_id)
        .expect("failed to create new program");

    let state_hash = processor
        .handle_executable_balance_top_up(H256::zero(), 10_000_000_000)
        .expect("failed to top up balance");

    let message = create_message_full(
        &mut processor,
        get_next_message_id(),
        DispatchKind::Init,
        user_id,
        "PING",
    );
    let ping_state_hash = processor
        .handle_message_queueing(state_hash, message)
        .expect("failed to populate message queue");

    processor
        .handle_new_program(async_id, upload_code_id)
        .expect("failed to create new program");

    let message = create_message_full(
        &mut processor,
        get_next_message_id(),
        DispatchKind::Init,
        user_id,
        ping_id.encode(),
    );
    let async_state_hash = processor
        .handle_message_queueing(state_hash, message)
        .expect("failed to populate message queue");

    let wait_for_reply_to = get_next_message_id();
    let message = create_message_full(
        &mut processor,
        wait_for_reply_to,
        DispatchKind::Handle,
        user_id,
        demo_async::Command::Common.encode(),
    );
    let async_state_hash = processor
        .handle_message_queueing(async_state_hash, message)
        .expect("failed to populate message queue");

    let states = BTreeMap::from_iter([(ping_id, ping_state_hash), (async_id, async_state_hash)]);
    let mut in_block_transitions =
        InBlockTransitions::new(BlockHeader::dummy(0), states, Default::default());

    run::run(
        8,
        processor.db.clone(),
        processor.creator.clone(),
        &mut in_block_transitions,
    );

    let to_users = in_block_transitions.current_messages();

    assert_eq!(to_users.len(), 3);

    let message = &to_users[0].1;
    assert_eq!(message.destination, user_id);
    assert_eq!(message.payload, b"PONG");

    let message = &to_users[1].1;
    assert_eq!(message.destination, user_id);
    assert_eq!(message.payload, b"");

    let message = &to_users[2].1;
    assert_eq!(message.destination, user_id);
    assert_eq!(message.payload, wait_for_reply_to.into_bytes().as_slice());
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
    let mut processor = Processor::new(Database::from_one(&db, Default::default())).unwrap();

    let ch0 = init_new_block(
        &mut processor,
        BlockHeader {
            height: 1,
            timestamp: 1,
            parent_hash: Default::default(),
        },
    );

    let code_id = processor
        .handle_new_code(code)
        .expect("failed to call runtime api")
        .expect("code failed verification or instrumentation");

    let amount = 10000;
    let mut states = BTreeMap::new();
    for i in 0..amount {
        let program_id = ProgramId::from(i);

        processor
            .handle_new_program(program_id, code_id)
            .expect("failed to create new program");

        let state_hash = processor
            .handle_executable_balance_top_up(H256::zero(), 10_000_000_000)
            .expect("failed to top up balance");

        let message = create_message(&mut processor, DispatchKind::Init, b"");
        let state_hash = processor
            .handle_message_queueing(state_hash, message)
            .expect("failed to populate message queue");

        states.insert(program_id, state_hash);
    }

    let mut in_block_transitions =
        InBlockTransitions::new(BlockHeader::dummy(1), states, Default::default());

    processor.run_schedule(&mut in_block_transitions);
    run::run(
        threads_amount,
        processor.db.clone(),
        processor.creator.clone(),
        &mut in_block_transitions,
    );
    assert_eq!(
        in_block_transitions.current_messages().len(),
        amount as usize
    );

    let mut changes = BTreeMap::new();

    for (pid, state_hash) in in_block_transitions.states_iter().clone() {
        let message = create_message(&mut processor, DispatchKind::Handle, b"");
        let new_state_hash = processor
            .handle_message_queueing(*state_hash, message)
            .expect("failed to populate message queue");
        changes.insert(*pid, new_state_hash);
    }

    for (pid, state_hash) in changes {
        in_block_transitions.modify_state(pid, state_hash).unwrap();
    }

    processor.run_schedule(&mut in_block_transitions);
    run::run(
        threads_amount,
        processor.db.clone(),
        processor.creator.clone(),
        &mut in_block_transitions,
    );
    // unchanged
    assert_eq!(
        in_block_transitions.current_messages().len(),
        amount as usize
    );

    let (_outcomes, states, schedule) = in_block_transitions.finalize();
    processor.db.set_block_end_program_states(ch0, states);
    processor.db.set_block_end_schedule(ch0, schedule);

    let mut parent = ch0;
    for _ in 0..10 {
        parent = init_new_block_from_parent(&mut processor, parent);
        let states = processor.db.block_start_program_states(parent).unwrap();
        processor.db.set_block_end_program_states(parent, states);
        let schedule = processor.db.block_start_schedule(parent).unwrap();
        processor.db.set_block_end_schedule(parent, schedule);
    }

    let ch11 = init_new_block_from_parent(&mut processor, parent);

    let states = processor.db.block_start_program_states(ch11).unwrap();
    let schedule = processor.db.block_start_schedule(ch11).unwrap();

    // Reproducibility test.
    {
        let mut expected_schedule = BTreeMap::<_, BTreeSet<_>>::new();

        for (pid, state_hash) in &states {
            let state = processor.db.read_state(*state_hash).unwrap();
            let waitlist_hash = state.waitlist_hash.with_hash(|h| h).unwrap();
            let waitlist = processor.db.read_waitlist(waitlist_hash).unwrap();

            for (mid, (dispatch, expiry)) in waitlist {
                assert_eq!(mid, dispatch.id);
                expected_schedule
                    .entry(expiry)
                    .or_default()
                    .insert(ScheduledTask::WakeMessage(*pid, mid));
            }
        }

        // This could fail in case of handling more scheduled ops: please, update test than.
        assert_eq!(schedule, expected_schedule);
    }

    let mut in_block_transitions =
        InBlockTransitions::new(BlockHeader::dummy(11), states, schedule);

    processor.run_schedule(&mut in_block_transitions);
    run::run(
        threads_amount,
        processor.db.clone(),
        processor.creator.clone(),
        &mut in_block_transitions,
    );

    assert_eq!(
        in_block_transitions.current_messages().len(),
        amount as usize
    );

    for (_pid, message) in in_block_transitions.current_messages() {
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
