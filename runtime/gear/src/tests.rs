// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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
use runtime_common::weights::{check_instructions_weights, check_pages_weights};

#[test]
fn instruction_weights_heuristics_test() {
    let weights = InstructionWeights::<Runtime>::default();

    let expected_weights = InstructionWeights {
        version: 0,
        _phantom: core::marker::PhantomData,

        i64const: 150,
        i64load: 7_000,
        i32load: 7_000,
        i64store: 29_000,
        i32store: 20_000,
        select: 7_100,
        r#if: 8_000,
        br: 3_300,
        br_if: 6_000,
        br_table: 10_900,
        br_table_per_entry: 300,

        call: 4_900,
        call_per_local: 0,
        call_indirect: 22_100,

        local_get: 600,
        local_set: 1_900,
        local_tee: 1_500,
        global_get: 2_000,
        global_set: 3_000,
        memory_current: 14_200,

        i64clz: 6_100,
        i32clz: 6_100,
        i64ctz: 6_700,
        i32ctz: 6_700,
        i64popcnt: 1_000,
        i32popcnt: 800,
        i64eqz: 4_000,
        i32eqz: 2_400,
        i32extend8s: 800,
        i32extend16s: 800,
        i64extend8s: 800,
        i64extend16s: 800,
        i64extend32s: 800,
        i64extendsi32: 800,
        i64extendui32: 400,
        i32wrapi64: 200,
        i64eq: 4_200,
        i32eq: 2_200,
        i64ne: 4_200,
        i32ne: 2_200,

        i64lts: 4_000,
        i32lts: 2_000,
        i64ltu: 4_000,
        i32ltu: 2_000,
        i64gts: 4_000,
        i32gts: 2_000,
        i64gtu: 4_000,
        i32gtu: 2_000,
        i64les: 4_000,
        i32les: 2_000,
        i64leu: 4_000,
        i32leu: 2_000,

        i64ges: 4_000,
        i32ges: 2_000,
        i64geu: 4_000,
        i32geu: 2_000,
        i64add: 2_500,
        i32add: 1_000,
        i64sub: 3_000,
        i32sub: 1_000,
        i64mul: 4_000,
        i32mul: 2_300,
        i64divs: 4_800,
        i32divs: 3_800,

        i64divu: 5_200,
        i32divu: 4_200,
        i64rems: 21_100,
        i32rems: 15_100,
        i64remu: 4_300,
        i32remu: 4_300,
        i64and: 3_000,
        i32and: 1_000,
        i64or: 3_000,
        i32or: 1_000,
        i64xor: 3_000,
        i32xor: 1_000,

        i64shl: 2_500,
        i32shl: 1_000,
        i64shrs: 2_500,
        i32shrs: 1_000,
        i64shru: 2_500,
        i32shru: 1_000,
        i64rotl: 2_000,
        i32rotl: 1_000,
        i64rotr: 2_500,
        i32rotr: 1_000,
    };

    check_instructions_weights(weights, expected_weights);
}

#[test]
fn page_costs_heuristic_test() {
    let page_costs: PageCosts = MemoryWeights::<Runtime>::default().into();
    let lazy_pages_weights: LazyPagesWeights = page_costs.lazy_pages_weights();

    let expected_pages_costs = PageCosts {
        lazy_pages_signal_read: 28_000_000.into(),
        lazy_pages_signal_write: 33_000_000.into(),
        lazy_pages_signal_write_after_read: 8_624_904.into(),
        lazy_pages_host_func_read: 29_000_000.into(),
        lazy_pages_host_func_write: 33_000_000.into(),
        lazy_pages_host_func_write_after_read: 7_531_864.into(),
        load_page_data: 8_700_000.into(),
        upload_page_data: 104_000_000.into(),
        static_page: 100.into(),
        mem_grow: 276_000.into(),
        parachain_load_heuristic: 0.into(),
    };

    let expected_lazy_pages_weights = LazyPagesWeights {
        signal_read: 28_000_000.into(),
        signal_write: 138_000_000.into(),
        signal_write_after_read: 112_000_000.into(),
        host_func_read: 29_000_000.into(),
        host_func_write: 137_000_000.into(),
        host_func_write_after_read: 112_000_000.into(),
        load_page_storage_data: 8_700_000.into(),
    };

    check_pages_weights(
        page_costs,
        expected_pages_costs,
        lazy_pages_weights,
        expected_lazy_pages_weights,
    );
}
