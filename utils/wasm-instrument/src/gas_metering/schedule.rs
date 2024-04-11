// This file is part of Gear.
//
// Copyright (C) 2021-2024 Gear Technologies Inc.
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

#![doc = r" This is auto-generated module that contains cost schedule from"]
#![doc = r" `pallets/gear/src/schedule.rs`."]
#![doc = r""]
#![doc = r" See `./scripts/weight-dump.sh` if you want to update it."]

pub struct Schedule {
    pub limits: Limits,
    pub instruction_weights: InstructionWeights,
    pub syscall_weights: SyscallWeights,
    pub memory_weights: MemoryWeights,
    pub module_instantiation_per_byte: Weight,
    pub db_write_per_byte: Weight,
    pub db_read_per_byte: Weight,
    pub code_instrumentation_cost: Weight,
    pub code_instrumentation_byte_cost: Weight,
}

impl Default for Schedule {
    fn default() -> Self {
        Self {
            limits: Limits::default(),
            instruction_weights: InstructionWeights::default(),
            syscall_weights: SyscallWeights::default(),
            memory_weights: MemoryWeights::default(),
            module_instantiation_per_byte: Weight {
                ref_time: 2957,
                proof_size: 0,
            },
            db_write_per_byte: Weight {
                ref_time: 241,
                proof_size: 0,
            },
            db_read_per_byte: Weight {
                ref_time: 573,
                proof_size: 0,
            },
            code_instrumentation_cost: Weight {
                ref_time: 286601000,
                proof_size: 3682,
            },
            code_instrumentation_byte_cost: Weight {
                ref_time: 60451,
                proof_size: 0,
            },
        }
    }
}

pub struct Limits {
    pub stack_height: Option<u32>,
    pub globals: u32,
    pub locals: u32,
    pub parameters: u32,
    pub memory_pages: u16,
    pub table_size: u32,
    pub br_table_size: u32,
    pub subject_len: u32,
    pub call_depth: u32,
    pub payload_len: u32,
    pub code_len: u32,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            stack_height: Some(36743),
            globals: 256,
            locals: 1024,
            parameters: 128,
            memory_pages: 512,
            table_size: 4096,
            br_table_size: 256,
            subject_len: 32,
            call_depth: 32,
            payload_len: 8388608,
            code_len: 524288,
        }
    }
}

pub struct InstructionWeights {
    pub version: u32,
    pub i64const: u32,
    pub i64load: u32,
    pub i32load: u32,
    pub i64store: u32,
    pub i32store: u32,
    pub select: u32,
    pub r#if: u32,
    pub br: u32,
    pub br_if: u32,
    pub br_table: u32,
    pub br_table_per_entry: u32,
    pub call: u32,
    pub call_indirect: u32,
    pub call_indirect_per_param: u32,
    pub call_per_local: u32,
    pub local_get: u32,
    pub local_set: u32,
    pub local_tee: u32,
    pub global_get: u32,
    pub global_set: u32,
    pub memory_current: u32,
    pub i64clz: u32,
    pub i32clz: u32,
    pub i64ctz: u32,
    pub i32ctz: u32,
    pub i64popcnt: u32,
    pub i32popcnt: u32,
    pub i64eqz: u32,
    pub i32eqz: u32,
    pub i32extend8s: u32,
    pub i32extend16s: u32,
    pub i64extend8s: u32,
    pub i64extend16s: u32,
    pub i64extend32s: u32,
    pub i64extendsi32: u32,
    pub i64extendui32: u32,
    pub i32wrapi64: u32,
    pub i64eq: u32,
    pub i32eq: u32,
    pub i64ne: u32,
    pub i32ne: u32,
    pub i64lts: u32,
    pub i32lts: u32,
    pub i64ltu: u32,
    pub i32ltu: u32,
    pub i64gts: u32,
    pub i32gts: u32,
    pub i64gtu: u32,
    pub i32gtu: u32,
    pub i64les: u32,
    pub i32les: u32,
    pub i64leu: u32,
    pub i32leu: u32,
    pub i64ges: u32,
    pub i32ges: u32,
    pub i64geu: u32,
    pub i32geu: u32,
    pub i64add: u32,
    pub i32add: u32,
    pub i64sub: u32,
    pub i32sub: u32,
    pub i64mul: u32,
    pub i32mul: u32,
    pub i64divs: u32,
    pub i32divs: u32,
    pub i64divu: u32,
    pub i32divu: u32,
    pub i64rems: u32,
    pub i32rems: u32,
    pub i64remu: u32,
    pub i32remu: u32,
    pub i64and: u32,
    pub i32and: u32,
    pub i64or: u32,
    pub i32or: u32,
    pub i64xor: u32,
    pub i32xor: u32,
    pub i64shl: u32,
    pub i32shl: u32,
    pub i64shrs: u32,
    pub i32shrs: u32,
    pub i64shru: u32,
    pub i32shru: u32,
    pub i64rotl: u32,
    pub i32rotl: u32,
    pub i64rotr: u32,
    pub i32rotr: u32,
}

impl Default for InstructionWeights {
    fn default() -> Self {
        Self {
            version: 1300,
            i64const: 155,
            i64load: 9514,
            i32load: 9159,
            i64store: 23660,
            i32store: 14219,
            select: 7295,
            r#if: 6348,
            br: 3181,
            br_if: 5915,
            br_table: 9349,
            br_table_per_entry: 361,
            call: 4536,
            call_indirect: 20157,
            call_indirect_per_param: 2241,
            call_per_local: 0,
            local_get: 1098,
            local_set: 2553,
            local_tee: 2885,
            global_get: 1876,
            global_set: 2809,
            memory_current: 14383,
            i64clz: 6814,
            i32clz: 6297,
            i64ctz: 6250,
            i32ctz: 5262,
            i64popcnt: 1164,
            i32popcnt: 746,
            i64eqz: 3701,
            i32eqz: 2380,
            i32extend8s: 976,
            i32extend16s: 885,
            i64extend8s: 993,
            i64extend16s: 1035,
            i64extend32s: 936,
            i64extendsi32: 866,
            i64extendui32: 556,
            i32wrapi64: 346,
            i64eq: 3593,
            i32eq: 2307,
            i64ne: 3548,
            i32ne: 2328,
            i64lts: 3617,
            i32lts: 2444,
            i64ltu: 3887,
            i32ltu: 2456,
            i64gts: 3952,
            i32gts: 2412,
            i64gtu: 3784,
            i32gtu: 2464,
            i64les: 3607,
            i32les: 2273,
            i64leu: 3516,
            i32leu: 2265,
            i64ges: 3486,
            i32ges: 2154,
            i64geu: 3605,
            i32geu: 2203,
            i64add: 2586,
            i32add: 1250,
            i64sub: 2659,
            i32sub: 1100,
            i64mul: 3539,
            i32mul: 2346,
            i64divs: 3342,
            i32divs: 3681,
            i64divu: 4414,
            i32divu: 4165,
            i64rems: 15453,
            i32rems: 13340,
            i64remu: 4085,
            i32remu: 4189,
            i64and: 2692,
            i32and: 1156,
            i64or: 2580,
            i32or: 1177,
            i64xor: 2660,
            i32xor: 1254,
            i64shl: 2119,
            i32shl: 1053,
            i64shrs: 2239,
            i32shrs: 1174,
            i64shru: 2379,
            i32shru: 1136,
            i64rotl: 2250,
            i32rotl: 1263,
            i64rotr: 2319,
            i32rotr: 1244,
        }
    }
}

pub struct SyscallWeights {
    pub alloc: Weight,
    pub free: Weight,
    pub free_range: Weight,
    pub free_range_per_page: Weight,
    pub gr_reserve_gas: Weight,
    pub gr_unreserve_gas: Weight,
    pub gr_system_reserve_gas: Weight,
    pub gr_gas_available: Weight,
    pub gr_message_id: Weight,
    pub gr_program_id: Weight,
    pub gr_source: Weight,
    pub gr_value: Weight,
    pub gr_value_available: Weight,
    pub gr_size: Weight,
    pub gr_read: Weight,
    pub gr_read_per_byte: Weight,
    pub gr_env_vars: Weight,
    pub gr_block_height: Weight,
    pub gr_block_timestamp: Weight,
    pub gr_random: Weight,
    pub gr_reply_deposit: Weight,
    pub gr_send: Weight,
    pub gr_send_per_byte: Weight,
    pub gr_send_wgas: Weight,
    pub gr_send_wgas_per_byte: Weight,
    pub gr_send_init: Weight,
    pub gr_send_push: Weight,
    pub gr_send_push_per_byte: Weight,
    pub gr_send_commit: Weight,
    pub gr_send_commit_wgas: Weight,
    pub gr_reservation_send: Weight,
    pub gr_reservation_send_per_byte: Weight,
    pub gr_reservation_send_commit: Weight,
    pub gr_reply_commit: Weight,
    pub gr_reply_commit_wgas: Weight,
    pub gr_reservation_reply: Weight,
    pub gr_reservation_reply_per_byte: Weight,
    pub gr_reservation_reply_commit: Weight,
    pub gr_reply_push: Weight,
    pub gr_reply: Weight,
    pub gr_reply_per_byte: Weight,
    pub gr_reply_wgas: Weight,
    pub gr_reply_wgas_per_byte: Weight,
    pub gr_reply_push_per_byte: Weight,
    pub gr_reply_to: Weight,
    pub gr_signal_code: Weight,
    pub gr_signal_from: Weight,
    pub gr_reply_input: Weight,
    pub gr_reply_input_wgas: Weight,
    pub gr_reply_push_input: Weight,
    pub gr_reply_push_input_per_byte: Weight,
    pub gr_send_input: Weight,
    pub gr_send_input_wgas: Weight,
    pub gr_send_push_input: Weight,
    pub gr_send_push_input_per_byte: Weight,
    pub gr_debug: Weight,
    pub gr_debug_per_byte: Weight,
    pub gr_reply_code: Weight,
    pub gr_exit: Weight,
    pub gr_leave: Weight,
    pub gr_wait: Weight,
    pub gr_wait_for: Weight,
    pub gr_wait_up_to: Weight,
    pub gr_wake: Weight,
    pub gr_create_program: Weight,
    pub gr_create_program_payload_per_byte: Weight,
    pub gr_create_program_salt_per_byte: Weight,
    pub gr_create_program_wgas: Weight,
    pub gr_create_program_wgas_payload_per_byte: Weight,
    pub gr_create_program_wgas_salt_per_byte: Weight,
}

impl Default for SyscallWeights {
    fn default() -> Self {
        Self {
            alloc: Weight {
                ref_time: 7167172,
                proof_size: 0,
            },
            free: Weight {
                ref_time: 757895,
                proof_size: 0,
            },
            free_range: Weight {
                ref_time: 900972,
                proof_size: 0,
            },
            free_range_per_page: Weight {
                ref_time: 61620,
                proof_size: 0,
            },
            gr_reserve_gas: Weight {
                ref_time: 2352421,
                proof_size: 0,
            },
            gr_unreserve_gas: Weight {
                ref_time: 2183611,
                proof_size: 0,
            },
            gr_system_reserve_gas: Weight {
                ref_time: 1186635,
                proof_size: 0,
            },
            gr_gas_available: Weight {
                ref_time: 1054312,
                proof_size: 0,
            },
            gr_message_id: Weight {
                ref_time: 1129832,
                proof_size: 0,
            },
            gr_program_id: Weight {
                ref_time: 1059044,
                proof_size: 0,
            },
            gr_source: Weight {
                ref_time: 1046996,
                proof_size: 0,
            },
            gr_value: Weight {
                ref_time: 1095736,
                proof_size: 0,
            },
            gr_value_available: Weight {
                ref_time: 1081158,
                proof_size: 0,
            },
            gr_size: Weight {
                ref_time: 1095335,
                proof_size: 0,
            },
            gr_read: Weight {
                ref_time: 1825905,
                proof_size: 0,
            },
            gr_read_per_byte: Weight {
                ref_time: 165,
                proof_size: 0,
            },
            gr_env_vars: Weight {
                ref_time: 1302076,
                proof_size: 0,
            },
            gr_block_height: Weight {
                ref_time: 1073860,
                proof_size: 0,
            },
            gr_block_timestamp: Weight {
                ref_time: 1091947,
                proof_size: 0,
            },
            gr_random: Weight {
                ref_time: 2094904,
                proof_size: 0,
            },
            gr_reply_deposit: Weight {
                ref_time: 6520451,
                proof_size: 0,
            },
            gr_send: Weight {
                ref_time: 3157153,
                proof_size: 0,
            },
            gr_send_per_byte: Weight {
                ref_time: 271,
                proof_size: 0,
            },
            gr_send_wgas: Weight {
                ref_time: 3206077,
                proof_size: 0,
            },
            gr_send_wgas_per_byte: Weight {
                ref_time: 269,
                proof_size: 0,
            },
            gr_send_init: Weight {
                ref_time: 1179784,
                proof_size: 0,
            },
            gr_send_push: Weight {
                ref_time: 2055072,
                proof_size: 0,
            },
            gr_send_push_per_byte: Weight {
                ref_time: 382,
                proof_size: 0,
            },
            gr_send_commit: Weight {
                ref_time: 2717863,
                proof_size: 0,
            },
            gr_send_commit_wgas: Weight {
                ref_time: 2730733,
                proof_size: 0,
            },
            gr_reservation_send: Weight {
                ref_time: 3439117,
                proof_size: 0,
            },
            gr_reservation_send_per_byte: Weight {
                ref_time: 267,
                proof_size: 0,
            },
            gr_reservation_send_commit: Weight {
                ref_time: 2937858,
                proof_size: 0,
            },
            gr_reply_commit: Weight {
                ref_time: 15979646,
                proof_size: 0,
            },
            gr_reply_commit_wgas: Weight {
                ref_time: 21362724,
                proof_size: 0,
            },
            gr_reservation_reply: Weight {
                ref_time: 9297412,
                proof_size: 0,
            },
            gr_reservation_reply_per_byte: Weight {
                ref_time: 432205,
                proof_size: 0,
            },
            gr_reservation_reply_commit: Weight {
                ref_time: 7187780,
                proof_size: 0,
            },
            gr_reply_push: Weight {
                ref_time: 1838663,
                proof_size: 0,
            },
            gr_reply: Weight {
                ref_time: 18397180,
                proof_size: 0,
            },
            gr_reply_per_byte: Weight {
                ref_time: 417,
                proof_size: 0,
            },
            gr_reply_wgas: Weight {
                ref_time: 17551258,
                proof_size: 0,
            },
            gr_reply_wgas_per_byte: Weight {
                ref_time: 423,
                proof_size: 0,
            },
            gr_reply_push_per_byte: Weight {
                ref_time: 675,
                proof_size: 0,
            },
            gr_reply_to: Weight {
                ref_time: 1081434,
                proof_size: 0,
            },
            gr_signal_code: Weight {
                ref_time: 1057043,
                proof_size: 0,
            },
            gr_signal_from: Weight {
                ref_time: 1088676,
                proof_size: 0,
            },
            gr_reply_input: Weight {
                ref_time: 33576008,
                proof_size: 0,
            },
            gr_reply_input_wgas: Weight {
                ref_time: 0,
                proof_size: 0,
            },
            gr_reply_push_input: Weight {
                ref_time: 1354524,
                proof_size: 0,
            },
            gr_reply_push_input_per_byte: Weight {
                ref_time: 163,
                proof_size: 0,
            },
            gr_send_input: Weight {
                ref_time: 3403443,
                proof_size: 0,
            },
            gr_send_input_wgas: Weight {
                ref_time: 3583443,
                proof_size: 0,
            },
            gr_send_push_input: Weight {
                ref_time: 1631742,
                proof_size: 0,
            },
            gr_send_push_input_per_byte: Weight {
                ref_time: 170,
                proof_size: 0,
            },
            gr_debug: Weight {
                ref_time: 1452831,
                proof_size: 0,
            },
            gr_debug_per_byte: Weight {
                ref_time: 321,
                proof_size: 0,
            },
            gr_reply_code: Weight {
                ref_time: 1056266,
                proof_size: 0,
            },
            gr_exit: Weight {
                ref_time: 197413680,
                proof_size: 0,
            },
            gr_leave: Weight {
                ref_time: 191227910,
                proof_size: 0,
            },
            gr_wait: Weight {
                ref_time: 127692646,
                proof_size: 0,
            },
            gr_wait_for: Weight {
                ref_time: 191877852,
                proof_size: 0,
            },
            gr_wait_up_to: Weight {
                ref_time: 188425504,
                proof_size: 0,
            },
            gr_wake: Weight {
                ref_time: 2048190,
                proof_size: 0,
            },
            gr_create_program: Weight {
                ref_time: 4276748,
                proof_size: 0,
            },
            gr_create_program_payload_per_byte: Weight {
                ref_time: 86,
                proof_size: 0,
            },
            gr_create_program_salt_per_byte: Weight {
                ref_time: 1941,
                proof_size: 0,
            },
            gr_create_program_wgas: Weight {
                ref_time: 4280463,
                proof_size: 0,
            },
            gr_create_program_wgas_payload_per_byte: Weight {
                ref_time: 87,
                proof_size: 0,
            },
            gr_create_program_wgas_salt_per_byte: Weight {
                ref_time: 1925,
                proof_size: 0,
            },
        }
    }
}

pub struct MemoryWeights {
    pub lazy_pages_signal_read: Weight,
    pub lazy_pages_signal_write: Weight,
    pub lazy_pages_signal_write_after_read: Weight,
    pub lazy_pages_host_func_read: Weight,
    pub lazy_pages_host_func_write: Weight,
    pub lazy_pages_host_func_write_after_read: Weight,
    pub load_page_data: Weight,
    pub upload_page_data: Weight,
    pub static_page: Weight,
    pub mem_grow: Weight,
    pub parachain_read_heuristic: Weight,
}

impl Default for MemoryWeights {
    fn default() -> Self {
        Self {
            lazy_pages_signal_read: Weight {
                ref_time: 29086537,
                proof_size: 0,
            },
            lazy_pages_signal_write: Weight {
                ref_time: 36014887,
                proof_size: 0,
            },
            lazy_pages_signal_write_after_read: Weight {
                ref_time: 10581256,
                proof_size: 0,
            },
            lazy_pages_host_func_read: Weight {
                ref_time: 30901094,
                proof_size: 0,
            },
            lazy_pages_host_func_write: Weight {
                ref_time: 37438602,
                proof_size: 0,
            },
            lazy_pages_host_func_write_after_read: Weight {
                ref_time: 9187150,
                proof_size: 0,
            },
            load_page_data: Weight {
                ref_time: 10250009,
                proof_size: 0,
            },
            upload_page_data: Weight {
                ref_time: 103952080,
                proof_size: 0,
            },
            static_page: Weight {
                ref_time: 100,
                proof_size: 0,
            },
            mem_grow: Weight {
                ref_time: 1297452,
                proof_size: 0,
            },
            parachain_read_heuristic: Weight {
                ref_time: 0,
                proof_size: 0,
            },
        }
    }
}

pub struct Weight {
    pub ref_time: u64,
    pub proof_size: u64,
}
