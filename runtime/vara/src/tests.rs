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
use weights::pallet_gear::WeightInfo;

const WEIGHT_TO_NS_DELIMETER: f32 = 1_000_000.0;
const INTERVAL_ERROR: f32 = 0.1; // 20% match interval

macro_rules! check_weight_inbounds_interval {
    ($instruction_call:ident, $interval_mid:expr) => {
        let weight_v =
            <() as WeightInfo>::$instruction_call(1) - <() as WeightInfo>::$instruction_call(0);
        let weight_v: f32 = (weight_v.ref_time() as f32) / WEIGHT_TO_NS_DELIMETER;
        let interval_start: f32 = $interval_mid * (1.0 - INTERVAL_ERROR);
        let interval_end: f32 = $interval_mid * (1.0 + INTERVAL_ERROR);

        if !(interval_start <= weight_v && weight_v <= interval_end) {
            let instr_name = stringify!($instruction_call);
            println!("FAILED: Interval mismatch for {instr_name}");
            println!("Weight is {weight_v} ns. Expected interval is [{interval_start}, {interval_end}] ns");
            panic!();
        }
    };
}

#[test]
fn heuristics_test() {
    check_weight_inbounds_interval!(instr_i64load, 30.7);
    check_weight_inbounds_interval!(instr_i64store, 42.6);
    check_weight_inbounds_interval!(instr_select, 7.7);
    check_weight_inbounds_interval!(instr_if, 6.0);
    check_weight_inbounds_interval!(instr_br, 3.3);
    check_weight_inbounds_interval!(instr_br_if, 6.3);
    check_weight_inbounds_interval!(instr_br_table, 9.9);
    check_weight_inbounds_interval!(instr_br_table_per_entry, 0.43);

    check_weight_inbounds_interval!(instr_call, 4.9);
    check_weight_inbounds_interval!(instr_call_const, 5.2);
    check_weight_inbounds_interval!(instr_call_indirect, 22.1);
    check_weight_inbounds_interval!(instr_call_indirect_per_param, 1.9);

    check_weight_inbounds_interval!(instr_local_get, 0.85);
    check_weight_inbounds_interval!(instr_local_set, 2.2);
    check_weight_inbounds_interval!(instr_local_tee, 2.2);
    check_weight_inbounds_interval!(instr_global_get, 2.1);
    check_weight_inbounds_interval!(instr_global_set, 3.1);
    check_weight_inbounds_interval!(instr_memory_current, 14.2);

    check_weight_inbounds_interval!(instr_i64clz, 6.3);
    check_weight_inbounds_interval!(instr_i64ctz, 5.9);
    check_weight_inbounds_interval!(instr_i64popcnt, 1.8);
    check_weight_inbounds_interval!(instr_i64eqz, 3.6);
    check_weight_inbounds_interval!(instr_i64extendsi32, 1.2);
    check_weight_inbounds_interval!(instr_i64extendui32, 0.64);
    check_weight_inbounds_interval!(instr_i32wrapi64, 0.65);
    check_weight_inbounds_interval!(instr_i64eq, 3.7);
    check_weight_inbounds_interval!(instr_i64ne, 3.7);

    check_weight_inbounds_interval!(instr_i64lts, 3.7);
    check_weight_inbounds_interval!(instr_i64ltu, 3.7);
    check_weight_inbounds_interval!(instr_i64gts, 3.7);
    check_weight_inbounds_interval!(instr_i64gtu, 3.7);
    check_weight_inbounds_interval!(instr_i64les, 3.7);
    check_weight_inbounds_interval!(instr_i64leu, 3.7);

    check_weight_inbounds_interval!(instr_i64ges, 3.7);
    check_weight_inbounds_interval!(instr_i64geu, 3.7);
    check_weight_inbounds_interval!(instr_i64add, 3.0);
    check_weight_inbounds_interval!(instr_i64sub, 3.0);
    check_weight_inbounds_interval!(instr_i64mul, 3.6);
    check_weight_inbounds_interval!(instr_i64divs, 4.3);

    check_weight_inbounds_interval!(instr_i64divu, 4.7);
    check_weight_inbounds_interval!(instr_i64rems, 21.6);
    check_weight_inbounds_interval!(instr_i64remu, 4.8);
    check_weight_inbounds_interval!(instr_i64and, 3.0);
    check_weight_inbounds_interval!(instr_i64or, 3.0);
    check_weight_inbounds_interval!(instr_i64xor, 3.0);

    check_weight_inbounds_interval!(instr_i64shl, 2.7);
    check_weight_inbounds_interval!(instr_i64shrs, 2.7);
    check_weight_inbounds_interval!(instr_i64shru, 2.7);
    check_weight_inbounds_interval!(instr_i64rotl, 2.7);
    check_weight_inbounds_interval!(instr_i64rotr, 2.7);
}
