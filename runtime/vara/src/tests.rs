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
use pallet_gear::InstructionWeights;

const INTERVAL_ERROR_DIVIDER: u32 = 3; // 66% match interval

macro_rules! check_weight_inbounds_interval {
    ($weights:expr, $instruction:ident, $interval_mid: expr) => {
        let weight_v: u32 = $weights.$instruction;
        let interval_start: u32 = $interval_mid - $interval_mid / INTERVAL_ERROR_DIVIDER;
        let interval_end: u32 = $interval_mid + $interval_mid / INTERVAL_ERROR_DIVIDER;

        if !(interval_start <= weight_v && weight_v <= interval_end) {
            let instr_name = stringify!($instruction);
            println!("FAILED: Interval mismatch for {instr_name}");
            println!("Weight is {weight_v} ps. Expected interval is [{interval_start}, {interval_end}] ps");
            panic!();
        }
    };
}

#[test]
fn heuristics_test() {
    let instruction_weights = InstructionWeights::<crate::Runtime>::default();

    check_weight_inbounds_interval!(instruction_weights, i64const, 150);
    check_weight_inbounds_interval!(instruction_weights, i64load, 7_000);
    check_weight_inbounds_interval!(instruction_weights, i32load, 7_000);
    check_weight_inbounds_interval!(instruction_weights, i64store, 29_000);
    check_weight_inbounds_interval!(instruction_weights, i32store, 29_000);
    check_weight_inbounds_interval!(instruction_weights, select, 7_100);
    check_weight_inbounds_interval!(instruction_weights, r#if, 8_000);
    check_weight_inbounds_interval!(instruction_weights, br, 3_300);
    check_weight_inbounds_interval!(instruction_weights, br_if, 6_000);
    check_weight_inbounds_interval!(instruction_weights, br_table, 10_900);
    check_weight_inbounds_interval!(instruction_weights, br_table_per_entry, 435);

    check_weight_inbounds_interval!(instruction_weights, call, 4_900);
    check_weight_inbounds_interval!(instruction_weights, call_per_local, 0);
    check_weight_inbounds_interval!(instruction_weights, call_indirect, 22_100);
    check_weight_inbounds_interval!(instruction_weights, call_indirect_per_param, 2_000);

    check_weight_inbounds_interval!(instruction_weights, local_get, 600);
    check_weight_inbounds_interval!(instruction_weights, local_set, 1_900);
    check_weight_inbounds_interval!(instruction_weights, local_tee, 1_500);
    check_weight_inbounds_interval!(instruction_weights, global_get, 2_000);
    check_weight_inbounds_interval!(instruction_weights, global_set, 3_000);
    check_weight_inbounds_interval!(instruction_weights, memory_current, 14_200);

    check_weight_inbounds_interval!(instruction_weights, i64clz, 6_100);
    check_weight_inbounds_interval!(instruction_weights, i32clz, 6_100);
    check_weight_inbounds_interval!(instruction_weights, i64ctz, 6_700);
    check_weight_inbounds_interval!(instruction_weights, i32ctz, 6_700);
    check_weight_inbounds_interval!(instruction_weights, i64popcnt, 1_000);
    check_weight_inbounds_interval!(instruction_weights, i32popcnt, 800);
    check_weight_inbounds_interval!(instruction_weights, i64eqz, 4_000);
    check_weight_inbounds_interval!(instruction_weights, i32eqz, 2_400);
    check_weight_inbounds_interval!(instruction_weights, i64extendsi32, 800);
    check_weight_inbounds_interval!(instruction_weights, i64extendui32, 500);
    check_weight_inbounds_interval!(instruction_weights, i32wrapi64, 200);
    check_weight_inbounds_interval!(instruction_weights, i64eq, 4_200);
    check_weight_inbounds_interval!(instruction_weights, i32eq, 2_200);
    check_weight_inbounds_interval!(instruction_weights, i64ne, 4_200);
    check_weight_inbounds_interval!(instruction_weights, i32ne, 2_200);

    check_weight_inbounds_interval!(instruction_weights, i64lts, 4_000);
    check_weight_inbounds_interval!(instruction_weights, i32lts, 2_000);
    check_weight_inbounds_interval!(instruction_weights, i64ltu, 4_000);
    check_weight_inbounds_interval!(instruction_weights, i32ltu, 2_000);
    check_weight_inbounds_interval!(instruction_weights, i64gts, 4_000);
    check_weight_inbounds_interval!(instruction_weights, i32gts, 2_000);
    check_weight_inbounds_interval!(instruction_weights, i64gtu, 4_000);
    check_weight_inbounds_interval!(instruction_weights, i32gtu, 2_000);
    check_weight_inbounds_interval!(instruction_weights, i64les, 4_000);
    check_weight_inbounds_interval!(instruction_weights, i32les, 2_000);
    check_weight_inbounds_interval!(instruction_weights, i64leu, 4_000);
    check_weight_inbounds_interval!(instruction_weights, i32leu, 2_000);

    check_weight_inbounds_interval!(instruction_weights, i64ges, 4_000);
    check_weight_inbounds_interval!(instruction_weights, i32ges, 2_000);
    check_weight_inbounds_interval!(instruction_weights, i64geu, 4_000);
    check_weight_inbounds_interval!(instruction_weights, i32geu, 2_000);
    check_weight_inbounds_interval!(instruction_weights, i64add, 2_500);
    check_weight_inbounds_interval!(instruction_weights, i32add, 1_000);
    check_weight_inbounds_interval!(instruction_weights, i64sub, 3_000);
    check_weight_inbounds_interval!(instruction_weights, i32sub, 1_000);
    check_weight_inbounds_interval!(instruction_weights, i64mul, 4_000);
    check_weight_inbounds_interval!(instruction_weights, i32mul, 2_300);
    check_weight_inbounds_interval!(instruction_weights, i64divs, 4_800);
    check_weight_inbounds_interval!(instruction_weights, i32divs, 3_800);

    check_weight_inbounds_interval!(instruction_weights, i64divu, 5_200);
    check_weight_inbounds_interval!(instruction_weights, i32divu, 4_200);
    check_weight_inbounds_interval!(instruction_weights, i64rems, 21_100);
    check_weight_inbounds_interval!(instruction_weights, i32rems, 15_100);
    check_weight_inbounds_interval!(instruction_weights, i64remu, 4_400);
    check_weight_inbounds_interval!(instruction_weights, i32remu, 4_300);
    check_weight_inbounds_interval!(instruction_weights, i64and, 3_000);
    check_weight_inbounds_interval!(instruction_weights, i32and, 1_000);
    check_weight_inbounds_interval!(instruction_weights, i64or, 3_000);
    check_weight_inbounds_interval!(instruction_weights, i32or, 1_000);
    check_weight_inbounds_interval!(instruction_weights, i64xor, 3_000);
    check_weight_inbounds_interval!(instruction_weights, i32xor, 1_000);

    check_weight_inbounds_interval!(instruction_weights, i64shl, 2_500);
    check_weight_inbounds_interval!(instruction_weights, i32shl, 1_000);
    check_weight_inbounds_interval!(instruction_weights, i64shrs, 2_500);
    check_weight_inbounds_interval!(instruction_weights, i32shrs, 1_000);
    check_weight_inbounds_interval!(instruction_weights, i64shru, 2_500);
    check_weight_inbounds_interval!(instruction_weights, i32shru, 1_000);
    check_weight_inbounds_interval!(instruction_weights, i64rotl, 2_000);
    check_weight_inbounds_interval!(instruction_weights, i32rotl, 1_000);
    check_weight_inbounds_interval!(instruction_weights, i64rotr, 2_500);
    check_weight_inbounds_interval!(instruction_weights, i32rotr, 1_000);
}
