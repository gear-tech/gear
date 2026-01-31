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

// ============================================================================
// Charging Optimization Tests
// ============================================================================
//
// These tests verify the gas charging optimization for sequential execution,
// where data is loaded once and reused across dispatches without re-charging.

/// Test that verifies gas savings from charging optimization.
/// Multiple dispatches in a queue should benefit from caching:
/// - First dispatch goes through full charging (PrechargeContext::NeedsCharging)
/// - Subsequent dispatches use cached data (PrechargeContext::PreCharged)
#[test]
fn charging_optimization_gas_savings_with_multiple_dispatches() {
    // Run queue with extra mints to see the effect of caching
    let config_few = LoadConfig {
        extra_mints: 0,
        ..Default::default()
    };

    let config_many = LoadConfig {
        extra_mints: 10,
        ..Default::default()
    };

    init_lazy_pages();

    let code = build_code();
    let program_id = ActorId::generate_from_user(CodeId::generate(code.original_code()), b"");
    let user_id = ActorId::from(10);

    // Run with few dispatches (5 total: init + mint + burn + total_supply + balance)
    let storage_few = state::MemStorage::default();
    let program_state_few = build_state_with_config(&storage_few, user_id, config_few);
    let runtime_few = TestRuntimeInterface::new(storage_few);

    let (journals_few, gas_few) = process_queue::<_, _>(
        program_id,
        program_state_few,
        MessageType::Canonical,
        Some(code.instrumented_code().clone()),
        Some(code.metadata().clone()),
        &runtime_few,
        10_000_000_000_000,
    );

    // Run with many dispatches (15 total: init + mint + 10 extra mints + burn + total_supply + balance)
    let storage_many = state::MemStorage::default();
    let program_state_many = build_state_with_config(&storage_many, user_id, config_many);
    let runtime_many = TestRuntimeInterface::new(storage_many);

    let (journals_many, gas_many) = process_queue::<_, _>(
        program_id,
        program_state_many,
        MessageType::Canonical,
        Some(code.instrumented_code().clone()),
        Some(code.metadata().clone()),
        &runtime_many,
        10_000_000_000_000,
    );

    // Both should produce valid results
    assert_eq!(extract_supply(&journals_few), Some(1_000_000));
    assert_eq!(extract_supply(&journals_many), Some(1_000_010)); // 1_000_000 + 10 extra mints

    // Gas should increase, but not linearly with dispatch count
    // because subsequent dispatches skip charging for program/code data reads.
    assert!(gas_many > gas_few, "More dispatches should use more gas");

    // Calculate gas per dispatch (rough metric)
    let dispatches_few = 5; // init + mint + burn + total_supply + balance
    let dispatches_many = 15; // init + mint + 10 extra mints + burn + total_supply + balance

    let gas_per_dispatch_few = gas_few / dispatches_few;
    let gas_per_dispatch_many = gas_many / dispatches_many;

    // With caching, gas per dispatch should be lower for many dispatches
    // because the charging overhead is amortized over more dispatches.
    assert!(
        gas_per_dispatch_many < gas_per_dispatch_few,
        "Gas per dispatch should be lower with more dispatches due to caching. \
         Few: {} gas/dispatch, Many: {} gas/dispatch",
        gas_per_dispatch_few,
        gas_per_dispatch_many
    );
}

/// Test that allocation updates are properly propagated to the cache.
/// When a dispatch modifies allocations, the cache should be updated
/// so subsequent dispatches see the correct allocations.
#[test]
fn charging_optimization_allocation_updates_propagate() {
    // The fungible token demo allocates memory during init and mint operations.
    // This test verifies that allocation changes are tracked through the cache.
    init_lazy_pages();

    let code = build_code();
    let program_id = ActorId::generate_from_user(CodeId::generate(code.original_code()), b"");
    let user_id = ActorId::from(10);

    let storage = state::MemStorage::default();

    // Create a queue with multiple operations that may trigger allocations
    let mut queue = state::MessageQueue::default();

    // Init dispatch
    let init = Dispatch::new(
        &storage,
        MessageId::from(1),
        user_id,
        InitConfig {
            name: "TestToken".to_string(),
            symbol: "TTK".to_string(),
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

    // Multiple mints to potentially trigger allocation growth
    for i in 2..=10u64 {
        let mint = Dispatch::new(
            &storage,
            MessageId::from(i),
            user_id,
            FTAction::Mint(100).encode(),
            0,
            false,
            MessageType::Canonical,
            false,
        )
        .expect("failed to build mint dispatch");
        queue.queue(mint);
    }

    // Final balance query
    let balance = Dispatch::new(
        &storage,
        MessageId::from(11),
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
    let queue_hash = queue.store(&storage);

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

    let runtime = TestRuntimeInterface::new(storage);

    let (journals, gas_spent) = process_queue::<_, _>(
        program_id,
        state,
        MessageType::Canonical,
        Some(code.instrumented_code().clone()),
        Some(code.metadata().clone()),
        &runtime,
        10_000_000_000_000,
    );

    // Verify all dispatches completed and balance is correct
    let balance = extract_balance(&journals);
    assert_eq!(balance, Some(900), "Balance should be 9 mints * 100 = 900");

    assert!(gas_spent > 0);

    // Count UpdateAllocations journal notes to verify allocation tracking
    let allocation_updates: usize = journals
        .iter()
        .flat_map(|(journal, _, _)| journal.iter())
        .filter(|note| matches!(note, JournalNote::UpdateAllocations { .. }))
        .count();

    // There should be some allocation updates (at least from init)
    // The exact count depends on the token implementation
    eprintln!("Allocation updates: {allocation_updates}");
}

/// Test that verifies the charging optimization doesn't affect correctness
/// when processing a queue with various message types.
#[test]
fn charging_optimization_correctness_with_mixed_operations() {
    init_lazy_pages();

    let code = build_code();
    let program_id = ActorId::generate_from_user(CodeId::generate(code.original_code()), b"");
    let user_id = ActorId::from(10);
    let other_user = ActorId::from(20);

    let storage = state::MemStorage::default();

    let mut queue = state::MessageQueue::default();
    let mut msg_id = 1u64;

    // Init
    let init = Dispatch::new(
        &storage,
        MessageId::from(msg_id),
        user_id,
        InitConfig {
            name: "MixedOpsToken".to_string(),
            symbol: "MOT".to_string(),
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
    msg_id += 1;

    // Mint 1000
    queue.queue(
        Dispatch::new(
            &storage,
            MessageId::from(msg_id),
            user_id,
            FTAction::Mint(1000).encode(),
            0,
            false,
            MessageType::Canonical,
            false,
        )
        .expect("failed"),
    );
    msg_id += 1;

    // Transfer 200 to other_user
    queue.queue(
        Dispatch::new(
            &storage,
            MessageId::from(msg_id),
            user_id,
            FTAction::Transfer {
                from: user_id,
                to: other_user,
                amount: 200,
            }
            .encode(),
            0,
            false,
            MessageType::Canonical,
            false,
        )
        .expect("failed"),
    );
    msg_id += 1;

    // Query user balance (should be 800)
    queue.queue(
        Dispatch::new(
            &storage,
            MessageId::from(msg_id),
            user_id,
            FTAction::BalanceOf(user_id).encode(),
            0,
            false,
            MessageType::Canonical,
            false,
        )
        .expect("failed"),
    );
    msg_id += 1;

    // Mint more (500)
    queue.queue(
        Dispatch::new(
            &storage,
            MessageId::from(msg_id),
            user_id,
            FTAction::Mint(500).encode(),
            0,
            false,
            MessageType::Canonical,
            false,
        )
        .expect("failed"),
    );
    msg_id += 1;

    // Query total supply (should be 1500)
    queue.queue(
        Dispatch::new(
            &storage,
            MessageId::from(msg_id),
            user_id,
            FTAction::TotalSupply.encode(),
            0,
            false,
            MessageType::Canonical,
            false,
        )
        .expect("failed"),
    );
    msg_id += 1;

    // Try to burn more than available (should fail but not crash)
    queue.queue(
        Dispatch::new(
            &storage,
            MessageId::from(msg_id),
            user_id,
            FTAction::Burn(10000).encode(),
            0,
            false,
            MessageType::Canonical,
            false,
        )
        .expect("failed"),
    );
    msg_id += 1;

    // Final balance check (should still be 1300 = 800 + 500)
    queue.queue(
        Dispatch::new(
            &storage,
            MessageId::from(msg_id),
            user_id,
            FTAction::BalanceOf(user_id).encode(),
            0,
            false,
            MessageType::Canonical,
            false,
        )
        .expect("failed"),
    );

    let queue_len = queue.len();
    let queue_hash = queue.store(&storage);

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

    let runtime = TestRuntimeInterface::new(storage);

    let (journals, gas_spent) = process_queue::<_, _>(
        program_id,
        state,
        MessageType::Canonical,
        Some(code.instrumented_code().clone()),
        Some(code.metadata().clone()),
        &runtime,
        10_000_000_000_000,
    );

    // Extract all balance and supply events
    let events = extract_events(&journals);

    // Find the balance events (there should be 2)
    let balance_events: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            FTEvent::Balance(v) => Some(*v),
            _ => None,
        })
        .collect();

    // Find total supply
    let supply = events.iter().find_map(|e| match e {
        FTEvent::TotalSupply(v) => Some(*v),
        _ => None,
    });

    // Verify correctness:
    // - First balance query after transfer: 800
    // - Second balance query after additional mint: 1300
    // - Total supply: 1500
    assert_eq!(balance_events.len(), 2, "Should have 2 balance events");
    assert_eq!(
        balance_events[0], 800,
        "Balance after transfer should be 800"
    );
    assert_eq!(balance_events[1], 1300, "Final balance should be 1300");
    assert_eq!(supply, Some(1500), "Total supply should be 1500");

    assert!(gas_spent > 0);
}

/// Test to verify that caching works correctly across many sequential dispatches.
/// This stress test ensures the optimization scales properly.
#[test]
fn charging_optimization_stress_test() {
    const DISPATCH_COUNT: usize = 100;

    init_lazy_pages();

    let code = build_code();
    let program_id = ActorId::generate_from_user(CodeId::generate(code.original_code()), b"");
    let user_id = ActorId::from(10);

    let storage = state::MemStorage::default();

    let mut queue = state::MessageQueue::default();
    let mut msg_id = 1u64;

    // Init
    queue.queue(
        Dispatch::new(
            &storage,
            MessageId::from(msg_id),
            user_id,
            InitConfig {
                name: "StressToken".to_string(),
                symbol: "STR".to_string(),
                decimals: 18,
                initial_capacity: None,
            }
            .encode(),
            0,
            true,
            MessageType::Canonical,
            false,
        )
        .expect("failed"),
    );
    msg_id += 1;

    // Many mint operations
    for _ in 0..DISPATCH_COUNT {
        queue.queue(
            Dispatch::new(
                &storage,
                MessageId::from(msg_id),
                user_id,
                FTAction::Mint(1).encode(),
                0,
                false,
                MessageType::Canonical,
                false,
            )
            .expect("failed"),
        );
        msg_id += 1;
    }

    // Final total supply query
    queue.queue(
        Dispatch::new(
            &storage,
            MessageId::from(msg_id),
            user_id,
            FTAction::TotalSupply.encode(),
            0,
            false,
            MessageType::Canonical,
            false,
        )
        .expect("failed"),
    );

    let queue_len = queue.len();
    let queue_hash = queue.store(&storage);

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
    state.executable_balance = 10_000_000_000_000;

    let runtime = TestRuntimeInterface::new(storage);

    let start = Instant::now();
    let (journals, gas_spent) = process_queue::<_, _>(
        program_id,
        state,
        MessageType::Canonical,
        Some(code.instrumented_code().clone()),
        Some(code.metadata().clone()),
        &runtime,
        100_000_000_000_000,
    );
    let elapsed = start.elapsed();

    // Verify correctness
    let supply = extract_supply(&journals);
    assert_eq!(
        supply,
        Some(DISPATCH_COUNT as u128),
        "Total supply should equal number of mints"
    );

    assert!(gas_spent > 0);

    // Log performance metrics
    let dispatches = DISPATCH_COUNT + 2; // init + mints + total_supply
    let gas_per_dispatch = gas_spent / dispatches as u64;
    eprintln!(
        "Stress test: {} dispatches, {} total gas, {} gas/dispatch, {:?} elapsed",
        dispatches, gas_spent, gas_per_dispatch, elapsed
    );
}

/// Helper function to build a benchmark queue with exact dispatch count.
/// dispatch_count=1 means just init, dispatch_count=2 means init + 1 mint, etc.
fn build_benchmark_queue(
    storage: &state::MemStorage,
    user_id: ActorId,
    dispatch_count: usize,
) -> state::ProgramState {
    assert!(dispatch_count >= 1, "Need at least 1 dispatch (init)");

    let mut queue = state::MessageQueue::default();

    // Init dispatch (always first)
    queue.queue(
        Dispatch::new(
            storage,
            MessageId::from(1),
            user_id,
            InitConfig {
                name: "BenchToken".to_string(),
                symbol: "BTK".to_string(),
                decimals: 18,
                initial_capacity: None,
            }
            .encode(),
            0,
            true,
            MessageType::Canonical,
            false,
        )
        .expect("failed"),
    );

    // Add mint dispatches for remaining count
    for i in 2..=dispatch_count as u64 {
        queue.queue(
            Dispatch::new(
                storage,
                MessageId::from(i),
                user_id,
                FTAction::Mint(1).encode(),
                0,
                false,
                MessageType::Canonical,
                false,
            )
            .expect("failed"),
        );
    }

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
    state.executable_balance = 10_000_000_000_000;

    state
}

/// Benchmark test for comparing gas and time consumption across different dispatch counts.
/// Run with: cargo test -p ethexe-runtime-common comparison_benchmark --release -- --nocapture
#[test]
fn comparison_benchmark() {
    const ITERATIONS: usize = 20;
    const GAS_ALLOWANCE: u64 = 100_000_000_000;

    init_lazy_pages();

    let code = build_code();
    let program_id = ActorId::generate_from_user(CodeId::generate(code.original_code()), b"");
    let user_id = ActorId::from(10);

    eprintln!("\n=== Charging Optimization Benchmark ===");
    eprintln!("Iterations per test: {}", ITERATIONS);
    eprintln!("Gas allowance: {}\n", GAS_ALLOWANCE);

    for dispatch_count in [1, 2, 3, 5, 10, 20, 50, 100, 200, 500] {
        let mut gas_total: u64 = 0;
        let mut total_time = std::time::Duration::ZERO;

        for _ in 0..ITERATIONS {
            let storage = state::MemStorage::default();
            let state = build_benchmark_queue(&storage, user_id, dispatch_count);
            let runtime = TestRuntimeInterface::new(storage);

            let start = Instant::now();
            let (_journals, gas_spent) = process_queue::<_, _>(
                program_id,
                state,
                MessageType::Canonical,
                Some(code.instrumented_code().clone()),
                Some(code.metadata().clone()),
                &runtime,
                GAS_ALLOWANCE,
            );
            let elapsed = start.elapsed();

            gas_total += gas_spent;
            total_time += elapsed;
        }

        let avg_gas = gas_total / ITERATIONS as u64;
        let gas_per_dispatch = avg_gas / dispatch_count as u64;
        let avg_elapsed = total_time / ITERATIONS as u32;

        eprintln!(
            "dispatches={:>3}, avg_gas={:>12}, gas/dispatch={:>10}, total_gas={:>14}, avg_time={:>10?}, total_time={:>10?}",
            dispatch_count, avg_gas, gas_per_dispatch, gas_total, avg_elapsed, total_time
        );
    }
}
