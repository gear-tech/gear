// This file is part of Gear.

// Copyright (C) 2021-2024 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use super::*;
use crate::Runtime;
use frame_support::traits::StorageInstance;
use gear_lazy_pages_common::LazyPagesCosts;
use pallet_gear::{InstructionWeights, MemoryWeights, SyscallWeights};
use runtime_common::weights::{
    check_instructions_weights, check_lazy_pages_costs, check_pages_costs, check_syscall_weights,
    PagesCosts,
};

#[cfg(feature = "dev")]
#[test]
fn bridge_storages_have_correct_prefixes() {
    // # SAFETY: Do not change storage prefixes without total bridge re-deploy.
    const PALLET_PREFIX: &str = "GearEthBridge";

    assert_eq!(
        pallet_gear_eth_bridge::AuthoritySetHashPrefix::<Runtime>::pallet_prefix(),
        PALLET_PREFIX
    );
    assert_eq!(
        pallet_gear_eth_bridge::QueueMerkleRootPrefix::<Runtime>::pallet_prefix(),
        PALLET_PREFIX
    );

    assert_eq!(
        pallet_gear_eth_bridge::AuthoritySetHashPrefix::<Runtime>::STORAGE_PREFIX,
        "AuthoritySetHash"
    );
    assert_eq!(
        pallet_gear_eth_bridge::QueueMerkleRootPrefix::<Runtime>::STORAGE_PREFIX,
        "QueueMerkleRoot"
    );
}

#[cfg(feature = "dev")]
#[test]
fn bridge_session_timer_is_correct() {
    assert_eq!(
        <Runtime as pallet_gear_eth_bridge::Config>::SessionsPerEra::get(),
        <Runtime as pallet_staking::Config>::SessionsPerEra::get()
    );

    // # SAFETY: Do not change staking's SessionsPerEra parameter without
    // making sure of correct integration with already running network.
    //
    // Change of the param will require migrating `pallet-gear-eth-bridge`'s
    // `ClearTimer` (or any actual time- or epoch- dependent entity) and
    // corresponding constant or even total bridge re-deploy.
    assert_eq!(
        <Runtime as pallet_staking::Config>::SessionsPerEra::get(),
        6
    );
}

#[test]
fn instruction_weights_heuristics_test() {
    let weights = InstructionWeights::<Runtime>::default();

    let expected_weights = InstructionWeights {
        version: 0,
        _phantom: core::marker::PhantomData,

        i64const: 160,
        i64load: 5_800,
        i32load: 8_000,
        i64store: 10_000,
        i32store: 20_000,
        select: 7_100,
        r#if: 8_000,
        br: 3_300,
        br_if: 6_000,
        br_table: 10_900,
        br_table_per_entry: 150,

        call: 4_900,
        call_per_local: 0,
        call_indirect: 22_100,
        call_indirect_per_param: 1_000,

        local_get: 900,
        local_set: 1_900,
        local_tee: 2_500,
        global_get: 700,
        global_set: 1_000,
        memory_current: 14_200,

        // TODO (breathx): ALARM
        i64clz: 400,
        i32clz: 300,
        i64ctz: 400,
        i32ctz: 250,
        i64popcnt: 450,
        i32popcnt: 350,
        i64eqz: 1_300,
        i32eqz: 1_200,
        i32extend8s: 400,
        i32extend16s: 400,
        i64extend8s: 400,
        i64extend16s: 400,
        i64extend32s: 400,
        i64extendsi32: 350,
        i64extendui32: 400,
        // TODO (breathx): ALARM
        i32wrapi64: 10,
        i64eq: 1_800,
        i32eq: 1_100,
        i64ne: 1_700,
        i32ne: 1_000,

        i64lts: 1_200,
        i32lts: 1_000,
        i64ltu: 1_200,
        i32ltu: 1_000,
        i64gts: 1_200,
        i32gts: 1_000,
        i64gtu: 1_200,
        i32gtu: 1_000,
        i64les: 1_200,
        i32les: 1_000,
        i64leu: 1_200,
        i32leu: 1_000,

        i64ges: 1_300,
        i32ges: 1_000,
        i64geu: 1_300,
        i32geu: 1_000,
        i64add: 1_300,
        i32add: 500,
        i64sub: 1_300,
        i32sub: 250,
        i64mul: 2_000,
        i32mul: 1_000,
        i64divs: 3_500,
        i32divs: 3_500,

        i64divu: 3_500,
        i32divu: 3_500,
        i64rems: 18_000,
        i32rems: 15_000,
        i64remu: 3_500,
        i32remu: 3_500,
        i64and: 1_000,
        i32and: 500,
        i64or: 1_000,
        i32or: 500,
        i64xor: 1_000,
        i32xor: 500,

        i64shl: 1_000,
        i32shl: 400,
        i64shrs: 1_000,
        i32shrs: 250,
        i64shru: 1_000,
        i32shru: 400,
        i64rotl: 750,
        i32rotl: 400,
        i64rotr: 1_000,
        i32rotr: 300,
    };

    check_instructions_weights(weights, expected_weights);
}

#[test]
fn syscall_weights_test() {
    let weights = SyscallWeights::<Runtime>::default();

    let expected = SyscallWeights {
        alloc: 1_500_000.into(),
        free: 900_000.into(),
        free_range: 900_000.into(),
        free_range_per_page: 40_000.into(),
        gr_reserve_gas: 2_300_000.into(),
        gr_unreserve_gas: 2_200_000.into(),
        gr_system_reserve_gas: 1_000_000.into(),
        gr_gas_available: 932_300.into(),
        gr_message_id: 926_500.into(),
        gr_program_id: 930_100.into(),
        gr_source: 930_700.into(),
        gr_value: 945_700.into(),
        gr_value_available: 967_900.into(),
        gr_size: 926_900.into(),
        gr_read: 1_700_000.into(),
        gr_read_per_byte: 200.into(),
        gr_env_vars: 1_000_000.into(),
        gr_block_height: 925_900.into(),
        gr_block_timestamp: 933_000.into(),
        gr_random: 1_900_000.into(),
        gr_reply_deposit: 4_900_000.into(),
        gr_send: 3_200_000.into(),
        gr_send_per_byte: 500.into(),
        gr_send_wgas: 3_300_000.into(),
        gr_send_wgas_per_byte: 500.into(),
        gr_send_init: 1_000_000.into(),
        gr_send_push: 2_000_000.into(),
        gr_send_push_per_byte: 500.into(),
        gr_send_commit: 2_700_000.into(),
        gr_send_commit_wgas: 2_700_000.into(),
        gr_reservation_send: 3_400_000.into(),
        gr_reservation_send_per_byte: 500.into(),
        gr_reservation_send_commit: 2_900_000.into(),
        gr_reply_commit: 12_000_000.into(),
        gr_reply_commit_wgas: 12_100_000.into(),
        gr_reservation_reply: 8_300_000.into(),
        gr_reservation_reply_per_byte: 675_000.into(),
        gr_reservation_reply_commit: 7_800_000.into(),
        gr_reply_push: 1_700_000.into(),
        gr_reply: 13_600_000.into(),
        gr_reply_per_byte: 650.into(),
        gr_reply_wgas: 11_900_000.into(),
        gr_reply_wgas_per_byte: 650.into(),
        gr_reply_push_per_byte: 640.into(),
        gr_reply_to: 950_200.into(),
        gr_signal_code: 962_500.into(),
        gr_signal_from: 941_500.into(),
        gr_reply_input: 13_300_000.into(),
        gr_reply_input_wgas: 10_600_000.into(),
        gr_reply_push_input: 1_200_000.into(),
        gr_reply_push_input_per_byte: 146.into(),
        gr_send_input: 3_100_000.into(),
        gr_send_input_wgas: 3_100_000.into(),
        gr_send_push_input: 1_500_000.into(),
        gr_send_push_input_per_byte: 165.into(),
        gr_debug: 1_200_000.into(),
        gr_debug_per_byte: 450.into(),
        gr_reply_code: 919_800.into(),
        // TODO (breathx): ALARM
        gr_exit: 96_500_000.into(),
        gr_leave: 130_300_000.into(),
        gr_wait: 112_500_000.into(),
        gr_wait_for: 92_000_000.into(),
        gr_wait_up_to: 127_000_000.into(),
        gr_wake: 3_000_000.into(),
        gr_create_program: 4_100_000.into(),
        gr_create_program_payload_per_byte: 120.into(),
        gr_create_program_salt_per_byte: 1_400.into(),
        gr_create_program_wgas: 4_100_000.into(),
        gr_create_program_wgas_payload_per_byte: 120.into(),
        gr_create_program_wgas_salt_per_byte: 1_400.into(),
        _phantom: Default::default(),
    };

    check_syscall_weights(weights, expected);
}

#[test]
fn page_costs_heuristic_test() {
    let page_costs: PagesCosts = MemoryWeights::<Runtime>::default().into();

    let expected_page_costs = PagesCosts {
        load_page_data: 10_000_000.into(),
        upload_page_data: 105_000_000.into(),
        mem_grow: 800_000.into(),
        mem_grow_per_page: 0.into(),
        parachain_read_heuristic: 0.into(),
    };

    check_pages_costs(page_costs, expected_page_costs);
}

#[test]
fn lazy_page_costs_heuristic_test() {
    let lazy_pages_costs: LazyPagesCosts = MemoryWeights::<Runtime>::default().into();

    let expected_lazy_pages_costs = LazyPagesCosts {
        signal_read: 28_000_000.into(),
        signal_write: 138_000_000.into(),
        signal_write_after_read: 112_000_000.into(),
        host_func_read: 29_000_000.into(),
        host_func_write: 137_000_000.into(),
        host_func_write_after_read: 112_000_000.into(),
        load_page_storage_data: 9_000_000.into(),
    };

    check_lazy_pages_costs(lazy_pages_costs, expected_lazy_pages_costs);
}

/// Check that it is not possible to write/change memory pages too cheaply,
/// because this may cause runtime heap memory overflow.
#[test]
fn write_is_not_too_cheap() {
    let costs: LazyPagesCosts = MemoryWeights::<Runtime>::default().into();
    let cheapest_write = u64::MAX
        .min(costs.signal_write.cost_for_one())
        .min(costs.signal_read.cost_for_one() + costs.signal_write_after_read.cost_for_one())
        .min(costs.host_func_write.cost_for_one())
        .min(costs.host_func_read.cost_for_one() + costs.host_func_write_after_read.cost_for_one());

    let block_max_gas = 3 * 10 ^ 12; // 3 seconds
    let runtime_heap_size_in_wasm_pages = 0x4000; // 1GB
    assert!((block_max_gas / cheapest_write) < runtime_heap_size_in_wasm_pages);
}
