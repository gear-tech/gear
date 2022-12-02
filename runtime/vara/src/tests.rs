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

const INTERVAL_ERROR_DIVIDER: u32 = 5; // 40% match interval

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

    check_weight_inbounds_interval!(instruction_weights, i64const, 283);
    check_weight_inbounds_interval!(instruction_weights, i64load, 30_700);
    check_weight_inbounds_interval!(instruction_weights, i64store, 42_600);
    check_weight_inbounds_interval!(instruction_weights, select, 7_100);
    check_weight_inbounds_interval!(instruction_weights, r#if, 6_000);
    check_weight_inbounds_interval!(instruction_weights, br, 3_300);
    check_weight_inbounds_interval!(instruction_weights, br_if, 6_000);
    check_weight_inbounds_interval!(instruction_weights, br_table, 9_900);
    check_weight_inbounds_interval!(instruction_weights, br_table_per_entry, 435);

    check_weight_inbounds_interval!(instruction_weights, call, 4_900);
    check_weight_inbounds_interval!(instruction_weights, call_indirect, 22_100);
    check_weight_inbounds_interval!(instruction_weights, call_indirect_per_param, 1_700);

    check_weight_inbounds_interval!(instruction_weights, local_get, 856);
    check_weight_inbounds_interval!(instruction_weights, local_set, 1_940);
    check_weight_inbounds_interval!(instruction_weights, local_tee, 2_000);
    check_weight_inbounds_interval!(instruction_weights, global_get, 2_100);
    check_weight_inbounds_interval!(instruction_weights, global_set, 2_900);
    check_weight_inbounds_interval!(instruction_weights, memory_current, 14_200);

    check_weight_inbounds_interval!(instruction_weights, i64clz, 6_100);
    check_weight_inbounds_interval!(instruction_weights, i64ctz, 5_700);
    check_weight_inbounds_interval!(instruction_weights, i64popcnt, 1_600);
    check_weight_inbounds_interval!(instruction_weights, i64eqz, 3_400);
    check_weight_inbounds_interval!(instruction_weights, i64extendsi32, 1_200);
    check_weight_inbounds_interval!(instruction_weights, i64extendui32, 650);
    check_weight_inbounds_interval!(instruction_weights, i32wrapi64, 375);
    check_weight_inbounds_interval!(instruction_weights, i64eq, 3_200);
    check_weight_inbounds_interval!(instruction_weights, i64ne, 3_200);

    check_weight_inbounds_interval!(instruction_weights, i64lts, 3_200);
    check_weight_inbounds_interval!(instruction_weights, i64ltu, 3_200);
    check_weight_inbounds_interval!(instruction_weights, i64gts, 3_200);
    check_weight_inbounds_interval!(instruction_weights, i64gtu, 3_200);
    check_weight_inbounds_interval!(instruction_weights, i64les, 3_200);
    check_weight_inbounds_interval!(instruction_weights, i64leu, 3_200);

    check_weight_inbounds_interval!(instruction_weights, i64ges, 3_200);
    check_weight_inbounds_interval!(instruction_weights, i64geu, 3_200);
    check_weight_inbounds_interval!(instruction_weights, i64add, 2_500);
    check_weight_inbounds_interval!(instruction_weights, i64sub, 2_500);
    check_weight_inbounds_interval!(instruction_weights, i64mul, 3_100);
    check_weight_inbounds_interval!(instruction_weights, i64divs, 3_800);

    check_weight_inbounds_interval!(instruction_weights, i64divu, 4_200);
    check_weight_inbounds_interval!(instruction_weights, i64rems, 21_100);
    check_weight_inbounds_interval!(instruction_weights, i64remu, 4_300);
    check_weight_inbounds_interval!(instruction_weights, i64and, 2_500);
    check_weight_inbounds_interval!(instruction_weights, i64or, 2_500);
    check_weight_inbounds_interval!(instruction_weights, i64xor, 2_500);

    check_weight_inbounds_interval!(instruction_weights, i64shl, 2_200);
    check_weight_inbounds_interval!(instruction_weights, i64shrs, 2_200);
    check_weight_inbounds_interval!(instruction_weights, i64shru, 2_200);
    check_weight_inbounds_interval!(instruction_weights, i64rotl, 2_200);
    check_weight_inbounds_interval!(instruction_weights, i64rotr, 2_200);
}
