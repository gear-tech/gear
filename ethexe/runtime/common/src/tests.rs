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

use super::*;
use alloc::vec;
use core::cell::Cell;
use demo_fungible_token::{FTAction, FTEvent, InitConfig, WASM_BINARY};
use ethexe_common::MaybeHashOf;
use gear_core::{
    code::Code,
    gas_metering::CustomConstantCostRules,
    ids::{ActorId, CodeId, MessageId, prelude::*},
    program::MemoryInfix,
};
use gear_lazy_pages::{LazyPagesStorage, LazyPagesVersion};
use gear_lazy_pages_common::LazyPagesInitContext;
use gear_lazy_pages_native_interface::LazyPagesNative;
use parity_scale_codec::{Decode, Encode};
use std::time::Instant;

#[derive(Debug)]
struct EmptyStorage;

impl LazyPagesStorage for EmptyStorage {
    fn page_exists(&self, _key: &[u8]) -> bool {
        false
    }

    fn load_page(&mut self, _key: &[u8], _buffer: &mut [u8]) -> Option<u32> {
        unreachable!();
    }
}

struct TestRuntimeInterface {
    storage: state::MemStorage,
    block_info: BlockInfo,
    state_hash: Cell<H256>,
}

impl TestRuntimeInterface {
    fn new(storage: state::MemStorage) -> Self {
        Self {
            storage,
            block_info: BlockInfo {
                height: 1,
                timestamp: 0,
            },
            state_hash: Cell::new(H256::zero()),
        }
    }
}

impl RuntimeInterface<state::MemStorage> for TestRuntimeInterface {
    type LazyPages = LazyPagesNative;

    fn block_info(&self) -> BlockInfo {
        self.block_info
    }

    fn init_lazy_pages(&self) {}

    fn random_data(&self) -> (Vec<u8>, u32) {
        (vec![0u8; 32], 0)
    }

    fn storage(&self) -> &state::MemStorage {
        &self.storage
    }

    fn update_state_hash(&self, hash: &H256) {
        self.state_hash.set(*hash);
    }
}

fn init_lazy_pages() {
    const STORAGE_PREFIX: [u8; 32] = *b"ethexe_sequential_queue_test____";

    gear_lazy_pages::init(
        LazyPagesVersion::Version1,
        LazyPagesInitContext::new(STORAGE_PREFIX),
        EmptyStorage,
    )
    .expect("failed to init lazy-pages");
}

fn build_code() -> Code {
    let code_bytes = WASM_BINARY.to_vec();

    Code::try_new(
        code_bytes,
        1,
        |_| CustomConstantCostRules::new(0, 0, 0),
        None,
        None,
        None,
        None,
    )
    .expect("failed to create Code")
}

#[derive(Clone, Copy)]
struct LoadConfig {
    extra_mints: usize,
    name_len: usize,
    symbol_len: usize,
}

impl Default for LoadConfig {
    fn default() -> Self {
        Self {
            extra_mints: 0,
            name_len: "MyToken".len(),
            symbol_len: "MTK".len(),
        }
    }
}

fn build_state_with_config(
    storage: &state::MemStorage,
    user_id: ActorId,
    config: LoadConfig,
) -> state::ProgramState {
    let mut queue = state::MessageQueue::default();

    let init = Dispatch::new(
        storage,
        MessageId::from(1),
        user_id,
        InitConfig {
            name: "N".repeat(config.name_len),
            symbol: "S".repeat(config.symbol_len),
            decimals: 18,
            initial_capacity: None,
        }
        .encode(),
        0,
        true,
        MessageType::Canonical,
        false,
    )
    .expect("failed to build init dispatch");
    queue.queue(init);

    let mut message_id = 2u64;
    let mint = Dispatch::new(
        storage,
        MessageId::from(message_id),
        user_id,
        FTAction::Mint(1_000_000).encode(),
        0,
        false,
        MessageType::Canonical,
        false,
    )
    .expect("failed to build mint dispatch");
    queue.queue(mint);

    for _ in 0..config.extra_mints {
        message_id += 1;
        let mint = Dispatch::new(
            storage,
            MessageId::from(message_id),
            user_id,
            FTAction::Mint(1).encode(),
            0,
            false,
            MessageType::Canonical,
            false,
        )
        .expect("failed to build extra mint dispatch");
        queue.queue(mint);
    }

    message_id += 1;
    let burn = Dispatch::new(
        storage,
        MessageId::from(message_id),
        user_id,
        FTAction::Burn(2_000_000).encode(),
        0,
        false,
        MessageType::Canonical,
        false,
    )
    .expect("failed to build burn dispatch");
    queue.queue(burn);

    message_id += 1;
    let total_supply = Dispatch::new(
        storage,
        MessageId::from(message_id),
        user_id,
        FTAction::TotalSupply.encode(),
        0,
        false,
        MessageType::Canonical,
        false,
    )
    .expect("failed to build total supply dispatch");
    queue.queue(total_supply);

    message_id += 1;
    let balance = Dispatch::new(
        storage,
        MessageId::from(message_id),
        user_id,
        FTAction::BalanceOf(user_id).encode(),
        0,
        false,
        MessageType::Canonical,
        false,
    )
    .expect("failed to build balance dispatch");
    queue.queue(balance);

    let queue_len = queue.len();
    let queue_hash = queue.store(storage);

    let mut state = state::ProgramState::zero();
    state.program = state::Program::Active(state::ActiveProgram {
        allocations_hash: MaybeHashOf::empty(),
        pages_hash: MaybeHashOf::empty(),
        memory_infix: MemoryInfix::new(0),
        initialized: false,
    });
    state.canonical_queue = state::MessageQueueHashWithSize {
        hash: queue_hash,
        cached_queue_size: queue_len as u8,
    };
    state.executable_balance = 1_500_000_000_000;

    state
}

fn extract_events(journals: &ProgramJournals) -> Vec<FTEvent> {
    let mut events = Vec::new();

    for (journal, _, _) in journals {
        for note in journal {
            let JournalNote::SendDispatch { dispatch, .. } = note else {
                continue;
            };

            let mut bytes = dispatch.payload_bytes();
            if let Ok(event) = FTEvent::decode(&mut bytes) {
                events.push(event);
            }
        }
    }

    events
}

fn run_queue() -> (ProgramJournals, u64) {
    run_queue_with_config(LoadConfig::default())
}

fn run_queue_with_config(config: LoadConfig) -> (ProgramJournals, u64) {
    init_lazy_pages();

    let code = build_code();
    let program_id = ActorId::generate_from_user(CodeId::generate(code.original_code()), b"");
    let user_id = ActorId::from(10);

    let storage = state::MemStorage::default();
    let program_state = build_state_with_config(&storage, user_id, config);
    let runtime = TestRuntimeInterface::new(storage);

    let (journals, gas_spent) = process_queue::<_, _>(
        program_id,
        program_state,
        MessageType::Canonical,
        Some(code.instrumented_code().clone()),
        Some(code.metadata().clone()),
        &runtime,
        10_000_000_000_000,
    );

    (journals, gas_spent)
}

fn extract_supply(journals: &ProgramJournals) -> Option<u128> {
    extract_events(journals)
        .into_iter()
        .find_map(|event| match event {
            FTEvent::TotalSupply(value) => Some(value),
            _ => None,
        })
}

fn extract_balance(journals: &ProgramJournals) -> Option<u128> {
    extract_events(journals)
        .into_iter()
        .find_map(|event| match event {
            FTEvent::Balance(value) => Some(value),
            _ => None,
        })
}

#[test]
fn sequential_queue_ok() {
    let (journals, _gas_spent) = run_queue();

    assert_eq!(extract_supply(&journals), Some(1_000_000));
    assert_eq!(extract_balance(&journals), Some(1_000_000));
}

#[test]
fn sequential_queue_recovers_after_failed_dispatch() {
    // This test verifies that sequential execution properly recovers state
    // after a failed dispatch (Burn fails because it tries to burn more than minted).
    // The subsequent TotalSupply and BalanceOf queries should return correct values.
    let (journals, gas_spent) = run_queue();

    assert_eq!(extract_supply(&journals), Some(1_000_000));
    assert_eq!(extract_balance(&journals), Some(1_000_000));
    assert!(gas_spent > 0);
}

#[test]
fn sequential_queue_benchmark() {
    const ITERATIONS: usize = 50;

    let config = LoadConfig {
        extra_mints: 2000,
        name_len: 2048,
        symbol_len: 2048,
    };

    let mut gas_total = 0u64;
    let mut elapsed = std::time::Duration::default();

    for _ in 0..ITERATIONS {
        let start = Instant::now();
        let (_journals, gas_spent) = run_queue_with_config(config);
        elapsed += start.elapsed();
        gas_total += gas_spent;
    }

    assert!(gas_total > 0);

    let avg_gas = gas_total / ITERATIONS as u64;
    let avg_elapsed = elapsed / ITERATIONS as u32;

    eprintln!("sequential: avg_gas={avg_gas}, avg_elapsed={avg_elapsed:?}");
}
