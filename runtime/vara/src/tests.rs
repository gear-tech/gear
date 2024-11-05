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

use frame_support::dispatch::GetDispatchInfo;
use frame_system::limits::WeightsPerClass;
use gear_core::costs::LazyPagesCosts;
use pallet_gear::{InstructionWeights, MemoryWeights, SyscallWeights};
use pallet_staking::WeightInfo as _;
use sp_runtime::AccountId32;

#[cfg(feature = "dev")]
use frame_support::traits::StorageInstance;

const INSTRUCTIONS_SPREAD: u8 = 50;
const SYSCALL_SPREAD: u8 = 10;
const PAGES_SPREAD: u8 = 10;

/// Structure to hold weight expectation
struct WeightExpectation {
    weight: u64,
    expected: u64,
    spread: u8,
    name: &'static str,
}

impl WeightExpectation {
    fn new(weight: u64, expected: u64, spread: u8, name: &'static str) -> Self {
        Self {
            weight,
            expected,
            spread,
            name,
        }
    }

    fn check(&self) -> Result<(), String> {
        let left = self.expected - self.expected * self.spread as u64 / 100;
        let right = self.expected + self.expected * self.spread as u64 / 100;

        if left > self.weight || self.weight > right {
            return Err(format!("Instruction [{}]. Weight is {} ps. Expected weight is {} ps. {}% spread interval: [{left} ps, {right} ps]", self.name, self.weight, self.expected, self.spread));
        }

        Ok(())
    }
}

fn check_expectations(expectations: &[WeightExpectation]) -> Result<usize, Vec<String>> {
    let errors = expectations
        .iter()
        .filter_map(|expectation| {
            if let Err(err) = expectation.check() {
                Some(err)
            } else {
                None
            }
        })
        .collect::<Vec<String>>();

    if errors.is_empty() {
        Ok(expectations.iter().count())
    } else {
        Err(errors)
    }
}

fn expected_instructions_weights_count() -> usize {
    let InstructionWeights {
        i64const: _,
        i64load: _,
        i32load: _,
        i64store: _,
        i32store: _,
        select: _,
        r#if: _,
        br: _,
        br_if: _,
        br_table: _,
        br_table_per_entry: _,
        call: _,
        call_indirect: _,
        call_indirect_per_param: _,
        call_per_local: _,
        local_get: _,
        local_set: _,
        local_tee: _,
        global_get: _,
        global_set: _,
        memory_current: _,
        i64clz: _,
        i32clz: _,
        i64ctz: _,
        i32ctz: _,
        i64popcnt: _,
        i32popcnt: _,
        i64eqz: _,
        i32eqz: _,
        i32extend8s: _,
        i32extend16s: _,
        i64extend8s: _,
        i64extend16s: _,
        i64extend32s: _,
        i64extendsi32: _,
        i64extendui32: _,
        i32wrapi64: _,
        i64eq: _,
        i32eq: _,
        i64ne: _,
        i32ne: _,
        i64lts: _,
        i32lts: _,
        i64ltu: _,
        i32ltu: _,
        i64gts: _,
        i32gts: _,
        i64gtu: _,
        i32gtu: _,
        i64les: _,
        i32les: _,
        i64leu: _,
        i32leu: _,
        i64ges: _,
        i32ges: _,
        i64geu: _,
        i32geu: _,
        i64add: _,
        i32add: _,
        i64sub: _,
        i32sub: _,
        i64mul: _,
        i32mul: _,
        i64divs: _,
        i32divs: _,
        i64divu: _,
        i32divu: _,
        i64rems: _,
        i32rems: _,
        i64remu: _,
        i32remu: _,
        i64and: _,
        i32and: _,
        i64or: _,
        i32or: _,
        i64xor: _,
        i32xor: _,
        i64shl: _,
        i32shl: _,
        i64shrs: _,
        i32shrs: _,
        i64shru: _,
        i32shru: _,
        i64rotl: _,
        i32rotl: _,
        i64rotr: _,
        i32rotr: _,
        version: _,
        _phantom: _,
    } = InstructionWeights::<Runtime>::default();

    // total number of instructions
    87
}

fn expected_syscall_weights_count() -> usize {
    let SyscallWeights {
        alloc: _,
        free: _,
        free_range: _,
        free_range_per_page: _,
        gr_reserve_gas: _,
        gr_unreserve_gas: _,
        gr_system_reserve_gas: _,
        gr_gas_available: _,
        gr_message_id: _,
        gr_program_id: _,
        gr_source: _,
        gr_value: _,
        gr_value_available: _,
        gr_size: _,
        gr_read: _,
        gr_read_per_byte: _,
        gr_env_vars: _,
        gr_block_height: _,
        gr_block_timestamp: _,
        gr_random: _,
        gr_reply_deposit: _,
        gr_send: _,
        gr_send_per_byte: _,
        gr_send_wgas: _,
        gr_send_wgas_per_byte: _,
        gr_send_init: _,
        gr_send_push: _,
        gr_send_push_per_byte: _,
        gr_send_commit: _,
        gr_send_commit_wgas: _,
        gr_reservation_send: _,
        gr_reservation_send_per_byte: _,
        gr_reservation_send_commit: _,
        gr_reply_commit: _,
        gr_reply_commit_wgas: _,
        gr_reservation_reply: _,
        gr_reservation_reply_per_byte: _,
        gr_reservation_reply_commit: _,
        gr_reply_push: _,
        gr_reply: _,
        gr_reply_per_byte: _,
        gr_reply_wgas: _,
        gr_reply_wgas_per_byte: _,
        gr_reply_push_per_byte: _,
        gr_reply_to: _,
        gr_signal_code: _,
        gr_signal_from: _,
        gr_reply_input: _,
        gr_reply_input_wgas: _,
        gr_reply_push_input: _,
        gr_reply_push_input_per_byte: _,
        gr_send_input: _,
        gr_send_input_wgas: _,
        gr_send_push_input: _,
        gr_send_push_input_per_byte: _,
        gr_debug: _,
        gr_debug_per_byte: _,
        gr_reply_code: _,
        gr_exit: _,
        gr_leave: _,
        gr_wait: _,
        gr_wait_for: _,
        gr_wait_up_to: _,
        gr_wake: _,
        gr_create_program: _,
        gr_create_program_payload_per_byte: _,
        gr_create_program_salt_per_byte: _,
        gr_create_program_wgas: _,
        gr_create_program_wgas_payload_per_byte: _,
        gr_create_program_wgas_salt_per_byte: _,
        _phantom: __phantom,
    } = SyscallWeights::<Runtime>::default();

    // total number of syscalls
    70
}

fn expected_pages_costs_count() -> usize {
    let LazyPagesCosts {
        // Fields for lazy pages costs
        signal_read: _,
        signal_write: _,
        signal_write_after_read: _,
        host_func_read: _,
        host_func_write: _,
        host_func_write_after_read: _,
        load_page_storage_data: _,
        // Fields for pages costs
        load_page_data: _,
        upload_page_data: _,
        mem_grow: _,
        mem_grow_per_page: _,
        parachain_read_heuristic: _,
    } = MemoryWeights::<Runtime>::default().into();

    // total number of lazy pages costs
    5
}

fn expected_lazy_pages_costs_count() -> usize {
    let LazyPagesCosts {
        // Fields for lazy pages costs
        signal_read: _,
        signal_write: _,
        signal_write_after_read: _,
        host_func_read: _,
        host_func_write: _,
        host_func_write_after_read: _,
        load_page_storage_data: _,
        // Fields for pages costs
        load_page_data: _,
        upload_page_data: _,
        mem_grow: _,
        mem_grow_per_page: _,
        parachain_read_heuristic: _,
    } = MemoryWeights::<Runtime>::default().into();

    // total number of lazy pages costs
    7
}

/// Check that the weights of instructions are within the expected range
fn check_instructions_weights<T: pallet_gear::Config>(
    weights: InstructionWeights<T>,
    expected: InstructionWeights<T>,
) -> Result<usize, Vec<String>> {
    macro_rules! expectation {
        ($inst_name:ident) => {
            WeightExpectation::new(
                weights.$inst_name.into(),
                expected.$inst_name.into(),
                INSTRUCTIONS_SPREAD,
                stringify!($inst_name),
            )
        };
    }

    let expectations = vec![
        expectation!(i64const),
        expectation!(i64load),
        expectation!(i32load),
        expectation!(i64store),
        expectation!(i32store),
        expectation!(select),
        expectation!(r#if),
        expectation!(br),
        expectation!(br_if),
        expectation!(br_table),
        expectation!(br_table_per_entry),
        expectation!(call),
        expectation!(call_indirect),
        expectation!(call_indirect_per_param),
        expectation!(call_per_local),
        expectation!(local_get),
        expectation!(local_set),
        expectation!(local_tee),
        expectation!(global_get),
        expectation!(global_set),
        expectation!(memory_current),
        expectation!(i64clz),
        expectation!(i32clz),
        expectation!(i64ctz),
        expectation!(i32ctz),
        expectation!(i64popcnt),
        expectation!(i32popcnt),
        expectation!(i64eqz),
        expectation!(i32eqz),
        expectation!(i32extend8s),
        expectation!(i32extend16s),
        expectation!(i64extend8s),
        expectation!(i64extend16s),
        expectation!(i64extend32s),
        expectation!(i64extendsi32),
        expectation!(i64extendui32),
        expectation!(i32wrapi64),
        expectation!(i64eq),
        expectation!(i32eq),
        expectation!(i64ne),
        expectation!(i32ne),
        expectation!(i64lts),
        expectation!(i32lts),
        expectation!(i64ltu),
        expectation!(i32ltu),
        expectation!(i64gts),
        expectation!(i32gts),
        expectation!(i64gtu),
        expectation!(i32gtu),
        expectation!(i64les),
        expectation!(i32les),
        expectation!(i64leu),
        expectation!(i32leu),
        expectation!(i64ges),
        expectation!(i32ges),
        expectation!(i64geu),
        expectation!(i32geu),
        expectation!(i64add),
        expectation!(i32add),
        expectation!(i64sub),
        expectation!(i32sub),
        expectation!(i64mul),
        expectation!(i32mul),
        expectation!(i64divs),
        expectation!(i32divs),
        expectation!(i64divu),
        expectation!(i32divu),
        expectation!(i64rems),
        expectation!(i32rems),
        expectation!(i64remu),
        expectation!(i32remu),
        expectation!(i64and),
        expectation!(i32and),
        expectation!(i64or),
        expectation!(i32or),
        expectation!(i64xor),
        expectation!(i32xor),
        expectation!(i64shl),
        expectation!(i32shl),
        expectation!(i64shrs),
        expectation!(i32shrs),
        expectation!(i64shru),
        expectation!(i32shru),
        expectation!(i64rotl),
        expectation!(i32rotl),
        expectation!(i64rotr),
        expectation!(i32rotr),
    ];

    check_expectations(&expectations)
}

/// Check that the weights of syscalls are within the expected range
fn check_syscall_weights<T: pallet_gear::Config>(
    weights: SyscallWeights<T>,
    expected: SyscallWeights<T>,
) -> Result<usize, Vec<String>> {
    macro_rules! expectation {
        ($inst_name:ident) => {
            WeightExpectation::new(
                weights.$inst_name.ref_time(),
                expected.$inst_name.ref_time(),
                SYSCALL_SPREAD,
                stringify!($inst_name),
            )
        };
    }

    let expectations = vec![
        expectation!(alloc),
        expectation!(free),
        expectation!(free_range),
        expectation!(free_range_per_page),
        expectation!(gr_reserve_gas),
        expectation!(gr_unreserve_gas),
        expectation!(gr_system_reserve_gas),
        expectation!(gr_gas_available),
        expectation!(gr_message_id),
        expectation!(gr_program_id),
        expectation!(gr_source),
        expectation!(gr_value),
        expectation!(gr_value_available),
        expectation!(gr_size),
        expectation!(gr_read),
        expectation!(gr_read_per_byte),
        expectation!(gr_env_vars),
        expectation!(gr_block_height),
        expectation!(gr_block_timestamp),
        expectation!(gr_random),
        expectation!(gr_reply_deposit),
        expectation!(gr_send),
        expectation!(gr_send_per_byte),
        expectation!(gr_send_wgas),
        expectation!(gr_send_wgas_per_byte),
        expectation!(gr_send_init),
        expectation!(gr_send_push),
        expectation!(gr_send_push_per_byte),
        expectation!(gr_send_commit),
        expectation!(gr_send_commit_wgas),
        expectation!(gr_reservation_send),
        expectation!(gr_reservation_send_per_byte),
        expectation!(gr_reservation_send_commit),
        expectation!(gr_reply_commit),
        expectation!(gr_reply_commit_wgas),
        expectation!(gr_reservation_reply),
        expectation!(gr_reservation_reply_per_byte),
        expectation!(gr_reservation_reply_commit),
        expectation!(gr_reply_push),
        expectation!(gr_reply),
        expectation!(gr_reply_per_byte),
        expectation!(gr_reply_wgas),
        expectation!(gr_reply_wgas_per_byte),
        expectation!(gr_reply_push_per_byte),
        expectation!(gr_reply_to),
        expectation!(gr_signal_code),
        expectation!(gr_signal_from),
        expectation!(gr_reply_input),
        expectation!(gr_reply_input_wgas),
        expectation!(gr_reply_push_input),
        expectation!(gr_reply_push_input_per_byte),
        expectation!(gr_send_input),
        expectation!(gr_send_input_wgas),
        expectation!(gr_send_push_input),
        expectation!(gr_send_push_input_per_byte),
        expectation!(gr_debug),
        expectation!(gr_debug_per_byte),
        expectation!(gr_reply_code),
        expectation!(gr_exit),
        expectation!(gr_leave),
        expectation!(gr_wait),
        expectation!(gr_wait_for),
        expectation!(gr_wait_up_to),
        expectation!(gr_wake),
        expectation!(gr_create_program),
        expectation!(gr_create_program_payload_per_byte),
        expectation!(gr_create_program_salt_per_byte),
        expectation!(gr_create_program_wgas),
        expectation!(gr_create_program_wgas_payload_per_byte),
        expectation!(gr_create_program_wgas_salt_per_byte),
    ];

    check_expectations(&expectations)
}

/// Check that the lazy-pages costs are within the expected range
fn check_lazy_pages_costs(
    lazy_pages_costs: LazyPagesCosts,
    expected_lazy_pages_costs: LazyPagesCosts,
) -> Result<usize, Vec<String>> {
    macro_rules! expectation {
        ($inst_name:ident) => {
            WeightExpectation::new(
                lazy_pages_costs.$inst_name.cost_for_one(),
                expected_lazy_pages_costs.$inst_name.cost_for_one(),
                PAGES_SPREAD,
                stringify!($inst_name),
            )
        };
    }

    let expectations = vec![
        expectation!(signal_read),
        expectation!(signal_write),
        expectation!(signal_write_after_read),
        expectation!(host_func_read),
        expectation!(host_func_write),
        expectation!(host_func_write_after_read),
        expectation!(load_page_storage_data),
    ];

    check_expectations(&expectations)
}

/// Check that the pages costs are within the expected range
fn check_pages_costs(
    page_costs: LazyPagesCosts,
    expected_page_costs: LazyPagesCosts,
) -> Result<usize, Vec<String>> {
    macro_rules! expectation {
        ($inst_name:ident) => {
            WeightExpectation::new(
                page_costs.$inst_name.cost_for_one(),
                expected_page_costs.$inst_name.cost_for_one(),
                PAGES_SPREAD,
                stringify!($inst_name),
            )
        };
    }

    let expectations = vec![
        expectation!(load_page_data),
        expectation!(upload_page_data),
        expectation!(mem_grow),
        expectation!(mem_grow_per_page),
        expectation!(parachain_read_heuristic),
    ];

    check_expectations(&expectations)
}

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
fn payout_stakers_fits_in_block() {
    let expected_weight =
        <Runtime as pallet_staking::Config>::WeightInfo::payout_stakers_alive_staked(
            <Runtime as pallet_staking::Config>::MaxExposurePageSize::get(),
        );

    let call: <Runtime as frame_system::Config>::RuntimeCall =
        RuntimeCall::Staking(pallet_staking::Call::payout_stakers {
            validator_stash: AccountId32::new(Default::default()),
            era: Default::default(),
        });

    let dispatch_info = call.get_dispatch_info();

    assert_eq!(dispatch_info.class, DispatchClass::Normal);
    assert_eq!(dispatch_info.weight, expected_weight);

    let block_weights = <Runtime as frame_system::Config>::BlockWeights::get();

    let normal_class_weights: WeightsPerClass =
        block_weights.per_class.get(DispatchClass::Normal).clone();

    let normal_ref_time = normal_class_weights
        .max_extrinsic
        .unwrap_or(Weight::MAX)
        .ref_time();
    let base_weight = normal_class_weights.base_extrinsic.ref_time();

    assert!(normal_ref_time - base_weight > expected_weight.ref_time());
}

#[test]
fn normal_dispatch_length_suits_minimal() {
    const MB: u32 = 1024 * 1024;

    let block_length = <Runtime as frame_system::Config>::BlockLength::get();

    // Normal dispatch class is bigger than 2 MB.
    assert!(*block_length.max.get(DispatchClass::Normal) > 2 * MB);

    // Others are on maximum.
    assert_eq!(*block_length.max.get(DispatchClass::Operational), 5 * MB);
    assert_eq!(*block_length.max.get(DispatchClass::Mandatory), 5 * MB);
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

        i64clz: 400,
        i32clz: 300,
        i64ctz: 400,
        i32ctz: 250,
        i64popcnt: 450,
        i32popcnt: 350,
        i64eqz: 1_300,
        i32eqz: 1_200,
        i32extend8s: 200,
        i32extend16s: 200,
        i64extend8s: 400,
        i64extend16s: 400,
        i64extend32s: 400,
        i64extendsi32: 200,
        i64extendui32: 200,
        i32wrapi64: 200,
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
        i64gtu: 1_900,
        i32gtu: 1_000,
        i64les: 1_900,
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
        i32sub: 500,
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

    let result = check_instructions_weights(weights, expected_weights);

    assert!(result.is_ok(), "{:#?}", result.err().unwrap());
    assert_eq!(result.unwrap(), expected_instructions_weights_count());
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
        gr_system_reserve_gas: 1_200_000.into(),
        gr_gas_available: 1_000_000.into(),
        gr_message_id: 1_000_000.into(),
        gr_program_id: 1_000_000.into(),
        gr_source: 1_000_000.into(),
        gr_value: 1_000_000.into(),
        gr_value_available: 1_000_000.into(),
        gr_size: 1_000_000.into(),
        gr_read: 1_900_000.into(),
        gr_read_per_byte: 200.into(),
        gr_env_vars: 1_200_000.into(),
        gr_block_height: 1_000_000.into(),
        gr_block_timestamp: 1_000_000.into(),
        gr_random: 1_900_000.into(),
        gr_reply_deposit: 4_900_000.into(),
        gr_send: 3_200_000.into(),
        gr_send_per_byte: 500.into(),
        gr_send_wgas: 3_300_000.into(),
        gr_send_wgas_per_byte: 500.into(),
        gr_send_init: 1_200_000.into(),
        gr_send_push: 2_000_000.into(),
        gr_send_push_per_byte: 500.into(),
        gr_send_commit: 2_700_000.into(),
        gr_send_commit_wgas: 2_700_000.into(),
        gr_reservation_send: 3_400_000.into(),
        gr_reservation_send_per_byte: 500.into(),
        gr_reservation_send_commit: 2_900_000.into(),
        gr_reply_commit: 12_000_000.into(),
        gr_reply_commit_wgas: 12_000_000.into(),
        gr_reservation_reply: 8_500_000.into(),
        gr_reservation_reply_per_byte: 675_000.into(),
        gr_reservation_reply_commit: 8_000_000.into(),
        gr_reply_push: 1_700_000.into(),
        gr_reply: 12_500_000.into(),
        gr_reply_per_byte: 650.into(),
        gr_reply_wgas: 12_500_000.into(),
        gr_reply_wgas_per_byte: 650.into(),
        gr_reply_push_per_byte: 640.into(),
        gr_reply_to: 1_000_000.into(),
        gr_signal_code: 1_000_000.into(),
        gr_signal_from: 1_000_000.into(),
        gr_reply_input: 20_000_000.into(),
        gr_reply_input_wgas: 30_000_000.into(),
        gr_reply_push_input: 1_200_000.into(),
        gr_reply_push_input_per_byte: 110.into(),
        gr_send_input: 3_100_000.into(),
        gr_send_input_wgas: 3_100_000.into(),
        gr_send_push_input: 1_500_000.into(),
        gr_send_push_input_per_byte: 150.into(),
        gr_debug: 1_200_000.into(),
        gr_debug_per_byte: 500.into(),
        gr_reply_code: 1_000_000.into(),
        gr_exit: 18_000_000.into(),
        gr_leave: 14_000_000.into(),
        gr_wait: 14_000_000.into(),
        gr_wait_for: 14_000_000.into(),
        gr_wait_up_to: 14_500_000.into(),
        gr_wake: 3_300_000.into(),
        gr_create_program: 4_100_000.into(),
        gr_create_program_payload_per_byte: 120.into(),
        gr_create_program_salt_per_byte: 1_400.into(),
        gr_create_program_wgas: 4_100_000.into(),
        gr_create_program_wgas_payload_per_byte: 120.into(),
        gr_create_program_wgas_salt_per_byte: 1_400.into(),
        _phantom: Default::default(),
    };

    let result = check_syscall_weights(weights, expected);

    assert!(result.is_ok(), "{:#?}", result.err().unwrap());
    assert_eq!(result.unwrap(), expected_syscall_weights_count());
}

#[test]
fn page_costs_heuristic_test() {
    let page_costs: LazyPagesCosts = MemoryWeights::<Runtime>::default().into();

    let expected_page_costs = LazyPagesCosts {
        load_page_data: 9_000_000.into(),
        upload_page_data: 105_000_000.into(),
        mem_grow: 800_000.into(),
        mem_grow_per_page: 0.into(),
        parachain_read_heuristic: 0.into(),
        ..Default::default()
    };

    let result = check_pages_costs(page_costs, expected_page_costs);

    assert!(result.is_ok(), "{:#?}", result.err().unwrap());
    assert_eq!(result.unwrap(), expected_pages_costs_count());
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
        ..Default::default()
    };

    let result = check_lazy_pages_costs(lazy_pages_costs, expected_lazy_pages_costs);

    assert!(result.is_ok(), "{:#?}", result.err().unwrap());
    assert_eq!(result.unwrap(), expected_lazy_pages_costs_count());
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
