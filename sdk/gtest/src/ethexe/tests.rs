// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::{DEFAULT_USER_ALICE, UNITS};
use ethexe_common::gear::MessageType;
use ethexe_runtime_common::{
    RuntimeInterface,
    state::{MemStorage, ProgramState, Storage},
};
use gear_core::ids::{ActorId, MessageId, prelude::MessageIdExt};
use parity_scale_codec::Encode;
use std::panic::{AssertUnwindSafe, catch_unwind};

use super::{Program, System, runtime::GTestEthexeRuntime};

const ETHEXE_EXECUTABLE_BALANCE: u128 = 2_000_000_000_000;

#[test]
fn top_level_gtest_stays_vara_when_ethexe_feature_enabled() {
    let system = crate::System::new();
    let program = crate::Program::from_binary_with_id(&system, 113, demo_ping::WASM_BINARY);

    let message_id = program.send_bytes(DEFAULT_USER_ALICE, b"PING");
    let result = system.run_next_block();

    assert!(result.succeed.contains(&message_id));
}

#[test]
fn ethexe_runtime_interface_tracks_state_hash_updates() {
    let storage = MemStorage::default();
    let state_hash = storage.write_program_state(ProgramState::zero());
    let runtime = GTestEthexeRuntime::new(&storage, state_hash);
    let mut updated_state = ProgramState::zero();
    updated_state.executable_balance = 99;
    let updated_state_hash = storage.write_program_state(updated_state);

    runtime.update_state_hash(&updated_state_hash);

    assert_eq!(runtime.state_hash(), updated_state_hash);
}

#[test]
fn ethexe_program_registration_creates_zero_balances() {
    let system = System::new();
    let program = Program::from_binary_with_id(&system, 100, demo_ping::WASM_BINARY);

    assert_eq!(program.balance(), 0);
    assert_eq!(program.executable_balance(), 0);
}

#[test]
fn ethexe_top_up_executable_balance_changes_only_executable_pool() {
    let system = System::new();
    let program = Program::from_binary_with_id(&system, 101, demo_ping::WASM_BINARY);

    system.top_up_executable_balance(program.id(), 55 * UNITS);

    assert_eq!(program.balance(), 0);
    assert_eq!(program.executable_balance(), 55 * UNITS);
}

#[test]
fn ethexe_duplicate_program_creation_does_not_reset_backend_state() {
    let system = System::new();
    let program = Program::from_binary_with_id(&system, 200, demo_ping::WASM_BINARY);
    system.top_up_executable_balance(program.id(), 55 * UNITS);

    let duplicate_creation = catch_unwind(AssertUnwindSafe(|| {
        Program::from_binary_with_id(&system, 200, demo_ping::WASM_BINARY);
    }));

    assert!(duplicate_creation.is_err());
    assert_eq!(program.executable_balance(), 55 * UNITS);
}

#[test]
fn ethexe_top_up_balance_changes_only_reducible_pool() {
    let system = System::new();
    let program = Program::from_binary_with_id(&system, 201, demo_ping::WASM_BINARY);

    system.top_up_balance(program.id(), 77 * UNITS);

    assert_eq!(program.balance(), 77 * UNITS);
    assert_eq!(program.executable_balance(), 0);
}

#[test]
fn ethexe_user_balance_still_uses_accounts() {
    let system = System::new();
    let user = 300;

    system.mint_to(user, 13 * UNITS);

    assert_eq!(system.balance_of(user), 13 * UNITS);
}

#[test]
fn ethexe_canonical_send_queues_init_then_handle() {
    let system = System::new();
    let program = Program::from_binary_with_id(&system, 102, demo_ping::WASM_BINARY);

    let first = program.send_bytes(1, b"PING");
    let second = program.send_bytes(1, b"PING");

    assert_eq!(system.queue_len(), 2);

    let manager = system.0.borrow();
    let storage = &manager.ethexe().storage;
    let state = manager.ethexe().program_state(program.id());
    let queue = state
        .canonical_queue
        .query(storage)
        .expect("canonical queue exists");
    let dispatches: Vec<_> = queue.into_iter().collect();

    assert_eq!(dispatches.len(), 2);
    assert_eq!(dispatches[0].id, first);
    assert!(dispatches[0].kind.is_init());
    assert_eq!(dispatches[0].message_type, MessageType::Canonical);
    assert_eq!(dispatches[0].source, ActorId::from(1));
    assert_eq!(dispatches[0].value, 0);
    let payload = dispatches[0].payload.clone().query(storage).unwrap();
    assert_eq!(&payload[..], b"PING");

    assert_eq!(dispatches[1].id, second);
    assert!(dispatches[1].kind.is_handle());
    assert_eq!(dispatches[1].message_type, MessageType::Canonical);
    assert_eq!(dispatches[1].source, ActorId::from(1));
    assert_eq!(dispatches[1].value, 0);
    let payload = dispatches[1].payload.clone().query(storage).unwrap();
    assert_eq!(&payload[..], b"PING");
}

#[test]
fn ethexe_injected_message_rejects_uninitialized_program() {
    let system = System::new();
    let program = Program::from_binary_with_id(&system, 103, demo_ping::WASM_BINARY);

    let rejected = catch_unwind(AssertUnwindSafe(|| {
        system.inject_message(program.id(), 1, b"PING", 0);
    }));

    assert!(rejected.is_err());
    assert_eq!(system.queue_len(), 0);

    let message_id = program.send_bytes(1, b"PING");
    assert_eq!(
        message_id,
        MessageId::generate_from_user(system.block_height() + 1, ActorId::from(1), 1)
    );
}

#[test]
fn ethexe_send_with_gas_is_rejected() {
    let system = System::new();
    let program = Program::from_binary_with_id(&system, 104, demo_ping::WASM_BINARY);

    let rejected = catch_unwind(AssertUnwindSafe(|| {
        program.send_bytes_with_gas(1, b"PING", 10_000, 0);
    }));

    assert!(rejected.is_err());

    let message_id = program.send_bytes(1, b"PING");
    assert_eq!(
        message_id,
        MessageId::generate_from_user(system.block_height() + 1, ActorId::from(1), 1)
    );
}

#[test]
fn ethexe_ping_canonical_decreases_executable_balance() {
    let system = System::new();
    let program = Program::from_binary_with_id(&system, 105, demo_ping::WASM_BINARY);

    system.top_up_executable_balance(program.id(), 500_000_000_000u128);
    let initial_executable_balance = program.executable_balance();

    let message_id = program.send_bytes(1, b"PING");
    let result = system.run_next_block();

    assert!(result.succeed.contains(&message_id));
    assert!(program.executable_balance() < initial_executable_balance);
}

#[test]
fn ethexe_injected_queue_runs_before_canonical_queue() {
    let system = System::new();
    let program = Program::from_binary_with_id(&system, 106, demo_ping::WASM_BINARY);

    system.top_up_executable_balance(program.id(), ETHEXE_EXECUTABLE_BALANCE);
    initialize_ping_program(&system, &program);

    let canonical = program.send_bytes(1, b"PING");
    let injected = system.inject_message(program.id(), 2, b"PING", 0);
    let result = system.run_next_block();

    assert!(result.succeed.contains(&injected));
    assert!(result.succeed.contains(&canonical));
    assert!(result.gas_burned.contains_key(&injected));
    assert!(result.gas_burned.contains_key(&canonical));

    let reply_order: Vec<_> = result
        .log()
        .iter()
        .filter(|log| log.source() == program.id() && log.payload() == b"PONG")
        .filter_map(|log| log.reply_to())
        .collect();

    assert_eq!(reply_order, [injected, canonical]);
}

#[test]
fn ethexe_first_injected_panic_below_threshold_does_not_charge() {
    let system = System::new();
    let program = Program::from_binary_with_id(&system, 107, demo_panic_payload::WASM_BINARY);

    system.top_up_executable_balance(program.id(), ETHEXE_EXECUTABLE_BALANCE);
    initialize_panic_payload_program(&system, &program);

    let initial_executable_balance = program.executable_balance();
    let injected = system.inject_message(program.id(), 1, Vec::<u8>::new(), 0);
    let result = system.run_next_block();

    assert!(result.failed.contains(&injected));
    assert_eq!(program.executable_balance(), initial_executable_balance);
    assert!(!result.gas_burned.contains_key(&injected));
}

#[test]
fn ethexe_canonical_panic_charges_executable_balance() {
    let system = System::new();
    let program = Program::from_binary_with_id(&system, 108, demo_panic_payload::WASM_BINARY);

    system.top_up_executable_balance(program.id(), ETHEXE_EXECUTABLE_BALANCE);
    initialize_panic_payload_program(&system, &program);

    let initial_executable_balance = program.executable_balance();
    let canonical = program.send_bytes(1, Vec::<u8>::new());
    let result = system.run_next_block();

    assert!(result.failed.contains(&canonical));
    assert!(program.executable_balance() < initial_executable_balance);
    assert!(result.gas_burned.contains_key(&canonical));
}

#[test]
fn ethexe_chunk_charges_max_gas_not_sum() {
    let measure = |program_ids: &[u64]| {
        let system = System::new();
        let first = Program::from_binary_with_id(&system, 109, demo_ping::WASM_BINARY);
        let second = Program::from_binary_with_id(&system, 110, demo_ping::WASM_BINARY);

        system.top_up_executable_balance(first.id(), ETHEXE_EXECUTABLE_BALANCE);
        system.top_up_executable_balance(second.id(), ETHEXE_EXECUTABLE_BALANCE);
        initialize_ping_program(&system, &first);
        initialize_ping_program(&system, &second);

        let mut messages = Vec::new();
        if program_ids.contains(&109) {
            messages.push(first.send_bytes(1, b"PING"));
        }
        if program_ids.contains(&110) {
            messages.push(second.send_bytes(1, b"PING"));
        }

        let result = system.run_next_block();
        for message_id in messages {
            assert!(result.succeed.contains(&message_id));
            assert!(result.gas_burned.contains_key(&message_id));
        }

        result.gas_allowance_spent
    };

    let first_only = measure(&[109]);
    let second_only = measure(&[110]);
    let together = measure(&[109, 110]);

    assert_eq!(together, first_only.max(second_only));
}

#[test]
fn ethexe_run_to_block_uses_ethexe_execution() {
    let system = System::new();
    let program = Program::from_binary_with_id(&system, 111, demo_ping::WASM_BINARY);
    system.top_up_executable_balance(program.id(), 500_000_000_000u128);

    let message_id = program.send_bytes(1, b"PING");
    let results = system.run_to_block(system.block_height() + 1);

    assert_eq!(results.len(), 1);
    assert!(results[0].succeed.contains(&message_id));
    assert_eq!(system.queue_len(), 0);
}

#[test]
fn ethexe_read_state_is_rejected() {
    let system = System::new();
    let program = Program::from_binary_with_id(&system, 112, demo_ping::WASM_BINARY);

    let rejected = catch_unwind(AssertUnwindSafe(|| {
        let _ = program.read_state_bytes(Vec::new());
    }));

    assert!(rejected.is_err());
}

fn initialize_ping_program(system: &System, program: &Program<'_>) {
    let message_id = program.send_bytes(1, b"PING");
    let result = system.run_next_block();

    assert!(result.succeed.contains(&message_id));
}

fn initialize_panic_payload_program(system: &System, program: &Program<'_>) {
    let message_id = program.send_bytes(1, ActorId::zero().encode());
    let result = system.run_next_block();

    assert!(result.succeed.contains(&message_id));
}
