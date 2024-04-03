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
                ref_time: 256,
                proof_size: 0,
            },
            db_write_per_byte: Weight {
                ref_time: 212,
                proof_size: 0,
            },
            db_read_per_byte: Weight {
                ref_time: 639,
                proof_size: 0,
            },
            code_instrumentation_cost: Weight {
                ref_time: 307460000,
                proof_size: 3682,
            },
            code_instrumentation_byte_cost: Weight {
                ref_time: 60169,
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
            version: 1201,
            i64const: 110,
            i64load: 6522,
            i32load: 6712,
            i64store: 15602,
            i32store: 17164,
            select: 5184,
            r#if: 5358,
            br: 3144,
            br_if: 3507,
            br_table: 8554,
            br_table_per_entry: 83,
            call: 4392,
            call_indirect: 15174,
            call_indirect_per_param: 1097,
            call_per_local: 0,
            local_get: 743,
            local_set: 1354,
            local_tee: 1666,
            global_get: 643,
            global_set: 914,
            memory_current: 11582,
            i64clz: 4035,
            i32clz: 4083,
            i64ctz: 3801,
            i32ctz: 3665,
            i64popcnt: 641,
            i32popcnt: 393,
            i64eqz: 1289,
            i32eqz: 1087,
            i32extend8s: 574,
            i32extend16s: 600,
            i64extend8s: 581,
            i64extend16s: 608,
            i64extend32s: 496,
            i64extendsi32: 454,
            i64extendui32: 443,
            i32wrapi64: 166,
            i64eq: 1226,
            i32eq: 736,
            i64ne: 1198,
            i32ne: 767,
            i64lts: 1191,
            i32lts: 732,
            i64ltu: 1203,
            i32ltu: 754,
            i64gts: 1175,
            i32gts: 958,
            i64gtu: 1191,
            i32gtu: 822,
            i64les: 1229,
            i32les: 781,
            i64leu: 1242,
            i32leu: 756,
            i64ges: 1373,
            i32ges: 1302,
            i64geu: 1404,
            i32geu: 883,
            i64add: 1370,
            i32add: 509,
            i64sub: 1017,
            i32sub: 753,
            i64mul: 1350,
            i32mul: 1321,
            i64divs: 3577,
            i32divs: 2961,
            i64divu: 3746,
            i32divu: 2881,
            i64rems: 11069,
            i32rems: 8402,
            i64remu: 3044,
            i32remu: 2194,
            i64and: 1075,
            i32and: 597,
            i64or: 1160,
            i32or: 632,
            i64xor: 1111,
            i32xor: 574,
            i64shl: 1033,
            i32shl: 522,
            i64shrs: 1565,
            i32shrs: 525,
            i64shru: 974,
            i32shru: 594,
            i64rotl: 989,
            i32rotl: 574,
            i64rotr: 1103,
            i32rotr: 485,
        }
    }
}

pub struct SyscallWeights {
    pub alloc: Weight,
    pub alloc_per_page: Weight,
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
                ref_time: 4762466,
                proof_size: 0,
            },
            alloc_per_page: Weight {
                ref_time: 233734,
                proof_size: 0,
            },
            free: Weight {
                ref_time: 620198,
                proof_size: 0,
            },
            free_range: Weight {
                ref_time: 763708,
                proof_size: 0,
            },
            free_range_per_page: Weight {
                ref_time: 62635,
                proof_size: 0,
            },
            gr_reserve_gas: Weight {
                ref_time: 2196938,
                proof_size: 0,
            },
            gr_unreserve_gas: Weight {
                ref_time: 1933513,
                proof_size: 0,
            },
            gr_system_reserve_gas: Weight {
                ref_time: 1044040,
                proof_size: 0,
            },
            gr_gas_available: Weight {
                ref_time: 921103,
                proof_size: 0,
            },
            gr_message_id: Weight {
                ref_time: 922373,
                proof_size: 0,
            },
            gr_program_id: Weight {
                ref_time: 927470,
                proof_size: 0,
            },
            gr_source: Weight {
                ref_time: 930069,
                proof_size: 0,
            },
            gr_value: Weight {
                ref_time: 942744,
                proof_size: 0,
            },
            gr_value_available: Weight {
                ref_time: 941689,
                proof_size: 0,
            },
            gr_size: Weight {
                ref_time: 923731,
                proof_size: 0,
            },
            gr_read: Weight {
                ref_time: 1595231,
                proof_size: 0,
            },
            gr_read_per_byte: Weight {
                ref_time: 172,
                proof_size: 0,
            },
            gr_env_vars: Weight {
                ref_time: 1037692,
                proof_size: 0,
            },
            gr_block_height: Weight {
                ref_time: 921269,
                proof_size: 0,
            },
            gr_block_timestamp: Weight {
                ref_time: 922299,
                proof_size: 0,
            },
            gr_random: Weight {
                ref_time: 1943105,
                proof_size: 0,
            },
            gr_reply_deposit: Weight {
                ref_time: 6231647,
                proof_size: 0,
            },
            gr_send: Weight {
                ref_time: 3136787,
                proof_size: 0,
            },
            gr_send_per_byte: Weight {
                ref_time: 424,
                proof_size: 0,
            },
            gr_send_wgas: Weight {
                ref_time: 3173610,
                proof_size: 0,
            },
            gr_send_wgas_per_byte: Weight {
                ref_time: 417,
                proof_size: 0,
            },
            gr_send_init: Weight {
                ref_time: 1012453,
                proof_size: 0,
            },
            gr_send_push: Weight {
                ref_time: 2028864,
                proof_size: 0,
            },
            gr_send_push_per_byte: Weight {
                ref_time: 423,
                proof_size: 0,
            },
            gr_send_commit: Weight {
                ref_time: 2616461,
                proof_size: 0,
            },
            gr_send_commit_wgas: Weight {
                ref_time: 2650548,
                proof_size: 0,
            },
            gr_reservation_send: Weight {
                ref_time: 3398225,
                proof_size: 0,
            },
            gr_reservation_send_per_byte: Weight {
                ref_time: 424,
                proof_size: 0,
            },
            gr_reservation_send_commit: Weight {
                ref_time: 2845908,
                proof_size: 0,
            },
            gr_reply_commit: Weight {
                ref_time: 58489262,
                proof_size: 0,
            },
            gr_reply_commit_wgas: Weight {
                ref_time: 56646128,
                proof_size: 0,
            },
            gr_reservation_reply: Weight {
                ref_time: 43670382,
                proof_size: 0,
            },
            gr_reservation_reply_per_byte: Weight {
                ref_time: 708083,
                proof_size: 0,
            },
            gr_reservation_reply_commit: Weight {
                ref_time: 49797420,
                proof_size: 0,
            },
            gr_reply_push: Weight {
                ref_time: 1673327,
                proof_size: 0,
            },
            gr_reply: Weight {
                ref_time: 59918734,
                proof_size: 0,
            },
            gr_reply_per_byte: Weight {
                ref_time: 690,
                proof_size: 0,
            },
            gr_reply_wgas: Weight {
                ref_time: 59972126,
                proof_size: 0,
            },
            gr_reply_wgas_per_byte: Weight {
                ref_time: 679,
                proof_size: 0,
            },
            gr_reply_push_per_byte: Weight {
                ref_time: 761,
                proof_size: 0,
            },
            gr_reply_to: Weight {
                ref_time: 950162,
                proof_size: 0,
            },
            gr_signal_code: Weight {
                ref_time: 917607,
                proof_size: 0,
            },
            gr_signal_from: Weight {
                ref_time: 936220,
                proof_size: 0,
            },
            gr_reply_input: Weight {
                ref_time: 76191750,
                proof_size: 0,
            },
            gr_reply_input_wgas: Weight {
                ref_time: 78968252,
                proof_size: 0,
            },
            gr_reply_push_input: Weight {
                ref_time: 1157797,
                proof_size: 0,
            },
            gr_reply_push_input_per_byte: Weight {
                ref_time: 163,
                proof_size: 0,
            },
            gr_send_input: Weight {
                ref_time: 3015378,
                proof_size: 0,
            },
            gr_send_input_wgas: Weight {
                ref_time: 3057616,
                proof_size: 0,
            },
            gr_send_push_input: Weight {
                ref_time: 1551539,
                proof_size: 0,
            },
            gr_send_push_input_per_byte: Weight {
                ref_time: 152,
                proof_size: 0,
            },
            gr_debug: Weight {
                ref_time: 1238259,
                proof_size: 0,
            },
            gr_debug_per_byte: Weight {
                ref_time: 375,
                proof_size: 0,
            },
            gr_reply_code: Weight {
                ref_time: 916177,
                proof_size: 0,
            },
            gr_exit: Weight {
                ref_time: 53349924,
                proof_size: 0,
            },
            gr_leave: Weight {
                ref_time: 13670732,
                proof_size: 0,
            },
            gr_wait: Weight {
                ref_time: 10707758,
                proof_size: 0,
            },
            gr_wait_for: Weight {
                ref_time: 10903200,
                proof_size: 0,
            },
            gr_wait_up_to: Weight {
                ref_time: 14040232,
                proof_size: 0,
            },
            gr_wake: Weight {
                ref_time: 3491328,
                proof_size: 0,
            },
            gr_create_program: Weight {
                ref_time: 4034527,
                proof_size: 0,
            },
            gr_create_program_payload_per_byte: Weight {
                ref_time: 112,
                proof_size: 0,
            },
            gr_create_program_salt_per_byte: Weight {
                ref_time: 1899,
                proof_size: 0,
            },
            gr_create_program_wgas: Weight {
                ref_time: 4107747,
                proof_size: 0,
            },
            gr_create_program_wgas_payload_per_byte: Weight {
                ref_time: 99,
                proof_size: 0,
            },
            gr_create_program_wgas_salt_per_byte: Weight {
                ref_time: 1899,
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
                ref_time: 28655462,
                proof_size: 0,
            },
            lazy_pages_signal_write: Weight {
                ref_time: 35330786,
                proof_size: 0,
            },
            lazy_pages_signal_write_after_read: Weight {
                ref_time: 10669586,
                proof_size: 0,
            },
            lazy_pages_host_func_read: Weight {
                ref_time: 29935057,
                proof_size: 0,
            },
            lazy_pages_host_func_write: Weight {
                ref_time: 34767252,
                proof_size: 0,
            },
            lazy_pages_host_func_write_after_read: Weight {
                ref_time: 11092966,
                proof_size: 0,
            },
            load_page_data: Weight {
                ref_time: 10229789,
                proof_size: 0,
            },
            upload_page_data: Weight {
                ref_time: 103478464,
                proof_size: 0,
            },
            static_page: Weight {
                ref_time: 100,
                proof_size: 0,
            },
            mem_grow: Weight {
                ref_time: 1105988,
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
