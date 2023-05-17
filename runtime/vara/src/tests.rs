// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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
use gear_backend_common::lazy_pages::LazyPagesWeights;
use gear_core_processor::configs::PageCosts;
use pallet_gear::{InstructionWeights, MemoryWeights};

// TODO: move differences check logic to runtime-common #2664.

#[track_caller]
fn assert_spreading(weight: u64, expected: u64, spread: u8) {
    let left = expected - expected * spread as u64 / 100;
    let right = expected + expected * spread as u64 / 100;

    assert!(
        left <= weight && weight <= right,
        "Weight is {weight} ps. Expected weight is {expected} ps. {spread}% spread interval: [{left} ps, {right} ps]"
    );
}

#[track_caller]
fn assert_instruction_weight(weight: u32, expected: u32) {
    assert_spreading(weight.into(), expected.into(), 50);
}

#[track_caller]
fn assert_pages_weight(weight: u64, expected: u64) {
    assert_spreading(weight, expected, 10);
}

#[test]
fn instruction_weights_heuristics_test() {
    let weights = InstructionWeights::<Runtime>::default();

    check_instruction_weight(weights.i64const, 150);
    check_instruction_weight(weights.i64load, 7_000);
    check_instruction_weight(weights.i32load, 7_000);
    check_instruction_weight(weights.i64store, 29_000);
    check_instruction_weight(weights.i32store, 20_000);
    check_instruction_weight(weights.select, 7_100);
    check_instruction_weight(weights.r#if, 8_000);
    check_instruction_weight(weights.br, 3_300);
    check_instruction_weight(weights.br_if, 6_000);
    check_instruction_weight(weights.br_table, 10_900);
    check_instruction_weight(weights.br_table_per_entry, 300);

    check_instruction_weight(weights.call, 4_900);
    check_instruction_weight(weights.call_per_local, 0);
    check_instruction_weight(weights.call_indirect, 22_100);
    check_instruction_weight(weights.call_indirect_per_param, 2_000);

    check_instruction_weight(weights.local_get, 600);
    check_instruction_weight(weights.local_set, 1_900);
    check_instruction_weight(weights.local_tee, 1_500);
    check_instruction_weight(weights.global_get, 2_000);
    check_instruction_weight(weights.global_set, 3_000);
    check_instruction_weight(weights.memory_current, 14_200);

    check_instruction_weight(weights.i64clz, 6_100);
    check_instruction_weight(weights.i32clz, 6_100);
    check_instruction_weight(weights.i64ctz, 6_700);
    check_instruction_weight(weights.i32ctz, 6_700);
    check_instruction_weight(weights.i64popcnt, 1_000);
    check_instruction_weight(weights.i32popcnt, 800);
    check_instruction_weight(weights.i64eqz, 4_000);
    check_instruction_weight(weights.i32eqz, 2_400);
    check_instruction_weight(weights.i64extendsi32, 800);
    check_instruction_weight(weights.i64extendui32, 400);
    check_instruction_weight(weights.i32wrapi64, 200);
    check_instruction_weight(weights.i64eq, 4_200);
    check_instruction_weight(weights.i32eq, 2_200);
    check_instruction_weight(weights.i64ne, 4_200);
    check_instruction_weight(weights.i32ne, 2_200);

    check_instruction_weight(weights.i64lts, 4_000);
    check_instruction_weight(weights.i32lts, 2_000);
    check_instruction_weight(weights.i64ltu, 4_000);
    check_instruction_weight(weights.i32ltu, 2_000);
    check_instruction_weight(weights.i64gts, 4_000);
    check_instruction_weight(weights.i32gts, 2_000);
    check_instruction_weight(weights.i64gtu, 4_000);
    check_instruction_weight(weights.i32gtu, 2_000);
    check_instruction_weight(weights.i64les, 4_000);
    check_instruction_weight(weights.i32les, 2_000);
    check_instruction_weight(weights.i64leu, 4_000);
    check_instruction_weight(weights.i32leu, 2_000);

    check_instruction_weight(weights.i64ges, 4_000);
    check_instruction_weight(weights.i32ges, 2_000);
    check_instruction_weight(weights.i64geu, 4_000);
    check_instruction_weight(weights.i32geu, 2_000);
    check_instruction_weight(weights.i64add, 2_500);
    check_instruction_weight(weights.i32add, 1_000);
    check_instruction_weight(weights.i64sub, 3_000);
    check_instruction_weight(weights.i32sub, 1_000);
    check_instruction_weight(weights.i64mul, 4_000);
    check_instruction_weight(weights.i32mul, 2_300);
    check_instruction_weight(weights.i64divs, 4_800);
    check_instruction_weight(weights.i32divs, 3_800);

    check_instruction_weight(weights.i64divu, 5_200);
    check_instruction_weight(weights.i32divu, 4_200);
    check_instruction_weight(weights.i64rems, 21_100);
    check_instruction_weight(weights.i32rems, 15_100);
    check_instruction_weight(weights.i64remu, 4_300);
    check_instruction_weight(weights.i32remu, 4_300);
    check_instruction_weight(weights.i64and, 3_000);
    check_instruction_weight(weights.i32and, 1_000);
    check_instruction_weight(weights.i64or, 3_000);
    check_instruction_weight(weights.i32or, 1_000);
    check_instruction_weight(weights.i64xor, 3_000);
    check_instruction_weight(weights.i32xor, 1_000);

    check_instruction_weight(weights.i64shl, 2_500);
    check_instruction_weight(weights.i32shl, 1_000);
    check_instruction_weight(weights.i64shrs, 2_500);
    check_instruction_weight(weights.i32shrs, 1_000);
    check_instruction_weight(weights.i64shru, 2_500);
    check_instruction_weight(weights.i32shru, 1_000);
    check_instruction_weight(weights.i64rotl, 2_000);
    check_instruction_weight(weights.i32rotl, 1_000);
    check_instruction_weight(weights.i64rotr, 2_500);
    check_instruction_weight(weights.i32rotr, 1_000);
}

#[test]
fn page_costs_heuristic_test() {
    let page_costs: PageCosts = MemoryWeights::<Runtime>::default().into();
    let expected = PageCosts {
        lazy_pages_signal_read: 28_000_000.into(),
        lazy_pages_signal_write: 33_000_000.into(),
        lazy_pages_signal_write_after_read: 9_500_000.into(),
        lazy_pages_host_func_read: 29_000_000.into(),
        lazy_pages_host_func_write: 33_000_000.into(),
        lazy_pages_host_func_write_after_read: 8_700_000.into(),
        load_page_data: 8_700_000.into(),
        upload_page_data: 104_000_000.into(),
        static_page: 100.into(),
        mem_grow: 100.into(),
        parachain_load_heuristic: 0.into(),
    };
    check_pages_weight(
        page_costs.lazy_pages_signal_read.one(),
        expected.lazy_pages_signal_read.one(),
    );
    check_pages_weight(
        page_costs.lazy_pages_signal_write.one(),
        expected.lazy_pages_signal_write.one(),
    );
    check_pages_weight(
        page_costs.lazy_pages_signal_write_after_read.one(),
        expected.lazy_pages_signal_write_after_read.one(),
    );
    check_pages_weight(
        page_costs.lazy_pages_host_func_read.one(),
        expected.lazy_pages_host_func_read.one(),
    );
    check_pages_weight(
        page_costs.lazy_pages_host_func_write.one(),
        expected.lazy_pages_host_func_write.one(),
    );
    check_pages_weight(
        page_costs.lazy_pages_host_func_write_after_read.one(),
        expected.lazy_pages_host_func_write_after_read.one(),
    );
    check_pages_weight(
        page_costs.load_page_data.one(),
        expected.load_page_data.one(),
    );
    check_pages_weight(
        page_costs.upload_page_data.one(),
        expected.upload_page_data.one(),
    );
    check_pages_weight(page_costs.static_page.one(), expected.static_page.one());
    check_pages_weight(page_costs.mem_grow.one(), expected.mem_grow.one());
    check_pages_weight(
        page_costs.parachain_load_heuristic.one(),
        expected.parachain_load_heuristic.one(),
    );

    let lazy_pages_weights: LazyPagesWeights = page_costs.lazy_pages_weights();
    let expected = LazyPagesWeights {
        signal_read: 28_000_000.into(),
        signal_write: 138_000_000.into(),
        signal_write_after_read: 112_000_000.into(),
        host_func_read: 29_000_000.into(),
        host_func_write: 137_000_000.into(),
        host_func_write_after_read: 112_000_000.into(),
        load_page_storage_data: 8_700_000.into(),
    };
    check_pages_weight(
        lazy_pages_weights.signal_read.one(),
        expected.signal_read.one(),
    );
    check_pages_weight(
        lazy_pages_weights.signal_write.one(),
        expected.signal_write.one(),
    );
    check_pages_weight(
        lazy_pages_weights.signal_write_after_read.one(),
        expected.signal_write_after_read.one(),
    );
    check_pages_weight(
        lazy_pages_weights.host_func_read.one(),
        expected.host_func_read.one(),
    );
    check_pages_weight(
        lazy_pages_weights.host_func_write.one(),
        expected.host_func_write.one(),
    );
    check_pages_weight(
        lazy_pages_weights.host_func_write_after_read.one(),
        expected.host_func_write_after_read.one(),
    );
    check_pages_weight(
        lazy_pages_weights.load_page_storage_data.one(),
        expected.load_page_storage_data.one(),
    );
}
