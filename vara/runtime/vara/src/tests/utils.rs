// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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

use gear_core::costs::{
    DbCosts, InstantiationCosts, InstrumentationCosts, IoCosts, LazyPagesCosts, PagesCosts,
    ProcessCosts,
};
use pallet_gear::{
    DbWeights, InstantiationWeights, InstructionWeights, InstrumentationWeights, MemoryWeights,
    Schedule, SyscallWeights,
};

const INSTRUCTIONS_SPREAD: u8 = 50;
const SYSCALL_SPREAD: u8 = 10;
const PAGES_SPREAD: u8 = 10;
const DB_SPREAD: u8 = 10;
const ALLOCATIONS_SPREAD: u8 = 10;
const INSTANTIATION_SPREAD: u8 = 10;
const INSTRUMENTATION_SPREAD: u8 = 10;

/// Structure to hold weight expectation
pub(super) struct WeightExpectation {
    weight: u64,
    expected: u64,
    spread: u8,
    name: &'static str,
}

impl WeightExpectation {
    pub(super) fn new(weight: u64, expected: u64, spread: u8, name: &'static str) -> Self {
        Self {
            weight,
            expected,
            spread,
            name,
        }
    }

    pub(super) fn check(&self) -> Result<(), String> {
        let left = self.expected - self.expected * self.spread as u64 / 100;
        let right = self.expected + self.expected * self.spread as u64 / 100;

        if left > self.weight || self.weight > right {
            return Err(format!(
                "[{}] field. Weight: {} ps. Expected: {} ps. {}% spread interval: [{left} ps, {right} ps]",
                self.name, self.weight, self.expected, self.spread
            ));
        }

        Ok(())
    }
}

pub(super) fn check_expectations(expectations: &[WeightExpectation]) -> Result<usize, Vec<String>> {
    let errors = expectations
        .iter()
        .filter_map(|expectation| expectation.check().err())
        .collect::<Vec<String>>();

    if errors.is_empty() {
        Ok(expectations.len())
    } else {
        Err(errors)
    }
}

pub(super) fn expected_instructions_weights_count() -> usize {
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

pub(super) fn expected_syscall_weights_count() -> usize {
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

pub(super) fn expected_pages_costs_count() -> usize {
    let IoCosts {
        lazy_pages: _,
        common:
            PagesCosts {
                load_page_data: _,
                upload_page_data: _,
                mem_grow: _,
                mem_grow_per_page: _,
                parachain_read_heuristic: _,
            },
    } = MemoryWeights::<Runtime>::default().into();

    // total number of lazy pages costs
    5
}

pub(super) fn expected_lazy_pages_costs_count() -> usize {
    let IoCosts {
        common: _,
        lazy_pages:
            LazyPagesCosts {
                signal_read: _,
                signal_write: _,
                signal_write_after_read: _,
                host_func_read: _,
                host_func_write: _,
                host_func_write_after_read: _,
                load_page_storage_data: _,
            },
    } = MemoryWeights::<Runtime>::default().into();

    // total number of lazy pages costs
    7
}

pub(super) fn expected_load_allocations_costs_count() -> usize {
    let ProcessCosts {
        ext: _,
        lazy_pages: _,
        db: _,
        instantiation: _,
        instrumentation: _,
        // Only field below is counted
        load_allocations_per_interval: _,
    } = Schedule::<Runtime>::default().process_costs();

    // total number of schedule costs
    1
}

pub(super) fn expected_instantiation_costs_count() -> usize {
    let InstantiationCosts {
        code_section_per_byte: _,
        data_section_per_byte: _,
        global_section_per_byte: _,
        table_section_per_byte: _,
        element_section_per_byte: _,
        type_section_per_byte: _,
    } = InstantiationWeights::<Runtime>::default().into();

    // total number of instantiation costs
    6
}

pub(super) fn expected_db_costs_count() -> usize {
    let DbCosts {
        read: _,
        read_per_byte: _,
        write: _,
        write_per_byte: _,
    } = DbWeights::<Runtime>::default().into();

    // total number of db costs
    4
}

pub(super) fn expected_code_instrumentation_costs_count() -> usize {
    let InstrumentationCosts {
        base: _,
        per_byte: _,
    } = InstrumentationWeights::<Runtime>::default().into();

    // total number of code instrumentation costs
    2
}

/// Check that the weights of instructions are within the expected range
pub(super) fn check_instructions_weights<T: pallet_gear::Config>(
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
pub(super) fn check_syscall_weights<T: pallet_gear::Config>(
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
pub(super) fn check_lazy_pages_costs(
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
pub(super) fn check_pages_costs(
    page_costs: PagesCosts,
    expected_page_costs: PagesCosts,
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

pub(super) fn check_load_allocations_costs(
    load_allocations_costs: u64,
    expected_load_allocations_costs: u64,
) -> Result<usize, Vec<String>> {
    let expectation = WeightExpectation::new(
        load_allocations_costs,
        expected_load_allocations_costs,
        ALLOCATIONS_SPREAD,
        "load_allocations_per_interval",
    );

    check_expectations(&[expectation])
}

pub(super) fn check_instantiation_costs(
    instantiation_costs: InstantiationCosts,
    expected_instantiation_costs: InstantiationCosts,
) -> Result<usize, Vec<String>> {
    macro_rules! expectation {
        ($inst_name:ident) => {
            WeightExpectation::new(
                instantiation_costs.$inst_name.into(),
                expected_instantiation_costs.$inst_name.into(),
                INSTANTIATION_SPREAD,
                stringify!($inst_name),
            )
        };
    }

    let expectations = vec![
        expectation!(code_section_per_byte),
        expectation!(data_section_per_byte),
        expectation!(global_section_per_byte),
        expectation!(table_section_per_byte),
        expectation!(element_section_per_byte),
        expectation!(type_section_per_byte),
    ];

    check_expectations(&expectations)
}

pub(super) fn check_db_costs(
    db_costs: DbCosts,
    expected_db_costs: DbCosts,
) -> Result<usize, Vec<String>> {
    macro_rules! expectation {
        ($inst_name:ident) => {
            WeightExpectation::new(
                db_costs.$inst_name.into(),
                expected_db_costs.$inst_name.into(),
                DB_SPREAD,
                stringify!($inst_name),
            )
        };
    }

    let expectations = vec![
        expectation!(read),
        expectation!(read_per_byte),
        expectation!(write),
        expectation!(write_per_byte),
    ];

    check_expectations(&expectations)
}

pub(super) fn check_code_instrumentation_costs(
    instrumentation_costs: InstrumentationCosts,
    expected_instrumentation_costs: InstrumentationCosts,
) -> Result<usize, Vec<String>> {
    macro_rules! expectation {
        ($inst_name:ident) => {
            WeightExpectation::new(
                instrumentation_costs.$inst_name.into(),
                expected_instrumentation_costs.$inst_name.into(),
                INSTRUMENTATION_SPREAD,
                stringify!($inst_name),
            )
        };
    }

    let expectations = vec![expectation!(base), expectation!(per_byte)];

    check_expectations(&expectations)
}
