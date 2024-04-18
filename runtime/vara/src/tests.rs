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
use gear_lazy_pages_common::LazyPagesCosts;
use pallet_gear::{InstructionWeights, MemoryWeights};
use runtime_common::weights::{check_instructions_weights, check_lazy_pages_costs};

#[test]
fn instruction_weights_heuristics_test() {
    let weights = InstructionWeights::<Runtime>::default();

    let expected_weights = InstructionWeights {
        version: 0,
        _phantom: core::marker::PhantomData,

        i64const: 160,
        i64load: 11_575,
        i32load: 8_000,
        i64store: 29_000,
        i32store: 20_000,
        select: 7_100,
        r#if: 8_000,
        br: 3_300,
        br_if: 6_000,
        br_table: 10_900,
        br_table_per_entry: 100,

        call: 4_900,
        call_per_local: 0,
        call_indirect: 22_100,
        call_indirect_per_param: 2_000,

        local_get: 900,
        local_set: 1_900,
        local_tee: 2_500,
        global_get: 700,
        global_set: 1_000,
        memory_current: 14_200,

        i64clz: 6_100,
        i32clz: 6_100,
        i64ctz: 6_700,
        i32ctz: 6_700,
        i64popcnt: 1_000,
        i32popcnt: 350,
        i64eqz: 1_300,
        i32eqz: 1_200,
        i32extend8s: 800,
        i32extend16s: 800,
        i64extend8s: 800,
        i64extend16s: 800,
        i64extend32s: 800,
        i64extendsi32: 800,
        i64extendui32: 400,
        i32wrapi64: 300,
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
        i32add: 1_000,
        i64sub: 1_300,
        i32sub: 1_000,
        i64mul: 2_000,
        i32mul: 2_000,
        i64divs: 3_500,
        i32divs: 3_500,

        i64divu: 3_500,
        i32divu: 3_500,
        i64rems: 10_000,
        i32rems: 10_000,
        i64remu: 3_500,
        i32remu: 3_500,
        i64and: 1_000,
        i32and: 1_000,
        i64or: 1_000,
        i32or: 1_000,
        i64xor: 1_000,
        i32xor: 1_000,

        i64shl: 1_000,
        i32shl: 800,
        i64shrs: 1_000,
        i32shrs: 800,
        i64shru: 1_000,
        i32shru: 800,
        i64rotl: 1_500,
        i32rotl: 800,
        i64rotr: 1_000,
        i32rotr: 800,
    };

    check_instructions_weights(weights, expected_weights);
}

#[test]
fn page_costs_heuristic_test() {
    let lazy_pages_costs: LazyPagesCosts = MemoryWeights::<Runtime>::default().into();

    let expected_lazy_pages_costs = LazyPagesCosts {
        signal_read: 28_000_000.into(),
        signal_write: 138_000_000.into(),
        signal_write_after_read: 112_000_000.into(),
        host_func_read: 29_000_000.into(),
        host_func_write: 137_000_000.into(),
        host_func_write_after_read: 112_000_000.into(),
        load_page_storage_data: 10_700_000.into(),
    };

    check_lazy_pages_costs(lazy_pages_costs, expected_lazy_pages_costs);
}
