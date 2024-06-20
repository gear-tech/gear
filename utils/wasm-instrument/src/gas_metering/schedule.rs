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
                ref_time: 192,
                proof_size: 0,
            },
            db_write_per_byte: Weight {
                ref_time: 237,
                proof_size: 0,
            },
            db_read_per_byte: Weight {
                ref_time: 584,
                proof_size: 0,
            },
            code_instrumentation_cost: Weight {
                ref_time: 230751537,
                proof_size: 3682,
            },
            code_instrumentation_byte_cost: Weight {
                ref_time: 59895,
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
    pub table_number: u32,
    pub br_table_size: u32,
    pub subject_len: u32,
    pub call_depth: u32,
    pub payload_len: u32,
    pub code_len: u32,
    pub data_segments_amount: u32,
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
            table_number: 100,
            br_table_size: 256,
            subject_len: 32,
            call_depth: 32,
            payload_len: 8388608,
            code_len: 524288,
            data_segments_amount: 1024,
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
            version: 1400,
            i64const: 153,
            i64load: 3111,
            i32load: 4850,
            i64store: 18248,
            i32store: 18485,
            select: 4079,
            r#if: 4247,
            br: 3392,
            br_if: 3784,
            br_table: 7513,
            br_table_per_entry: 93,
            call: 4778,
            call_indirect: 16129,
            call_indirect_per_param: 927,
            call_per_local: 0,
            local_get: 783,
            local_set: 1469,
            local_tee: 1389,
            global_get: 778,
            global_set: 1034,
            memory_current: 12053,
            i64clz: 4296,
            i32clz: 4152,
            i64ctz: 4115,
            i32ctz: 3967,
            i64popcnt: 576,
            i32popcnt: 428,
            i64eqz: 1469,
            i32eqz: 976,
            i32extend8s: 611,
            i32extend16s: 534,
            i64extend8s: 533,
            i64extend16s: 516,
            i64extend32s: 480,
            i64extendsi32: 548,
            i64extendui32: 310,
            i32wrapi64: 142,
            i64eq: 1280,
            i32eq: 868,
            i64ne: 1325,
            i32ne: 896,
            i64lts: 1338,
            i32lts: 872,
            i64ltu: 1328,
            i32ltu: 872,
            i64gts: 1297,
            i32gts: 808,
            i64gtu: 1283,
            i32gtu: 814,
            i64les: 1284,
            i32les: 922,
            i64leu: 1343,
            i32leu: 815,
            i64ges: 1331,
            i32ges: 792,
            i64geu: 1312,
            i32geu: 887,
            i64add: 895,
            i32add: 476,
            i64sub: 891,
            i32sub: 475,
            i64mul: 1200,
            i32mul: 844,
            i64divs: 2986,
            i32divs: 2209,
            i64divu: 3097,
            i32divu: 2129,
            i64rems: 9929,
            i32rems: 7007,
            i64remu: 3059,
            i32remu: 2102,
            i64and: 924,
            i32and: 385,
            i64or: 851,
            i32or: 422,
            i64xor: 887,
            i32xor: 434,
            i64shl: 778,
            i32shl: 342,
            i64shrs: 732,
            i32shrs: 424,
            i64shru: 741,
            i32shru: 355,
            i64rotl: 789,
            i32rotl: 363,
            i64rotr: 702,
            i32rotr: 433,
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
                ref_time: 1520740,
                proof_size: 0,
            },
            free: Weight {
                ref_time: 844812,
                proof_size: 0,
            },
            free_range: Weight {
                ref_time: 874247,
                proof_size: 0,
            },
            free_range_per_page: Weight {
                ref_time: 37426,
                proof_size: 0,
            },
            gr_reserve_gas: Weight {
                ref_time: 2084430,
                proof_size: 0,
            },
            gr_unreserve_gas: Weight {
                ref_time: 1940658,
                proof_size: 0,
            },
            gr_system_reserve_gas: Weight {
                ref_time: 1007193,
                proof_size: 0,
            },
            gr_gas_available: Weight {
                ref_time: 993448,
                proof_size: 0,
            },
            gr_message_id: Weight {
                ref_time: 931480,
                proof_size: 0,
            },
            gr_program_id: Weight {
                ref_time: 899030,
                proof_size: 0,
            },
            gr_source: Weight {
                ref_time: 905461,
                proof_size: 0,
            },
            gr_value: Weight {
                ref_time: 920227,
                proof_size: 0,
            },
            gr_value_available: Weight {
                ref_time: 990711,
                proof_size: 0,
            },
            gr_size: Weight {
                ref_time: 889491,
                proof_size: 0,
            },
            gr_read: Weight {
                ref_time: 1609296,
                proof_size: 0,
            },
            gr_read_per_byte: Weight {
                ref_time: 201,
                proof_size: 0,
            },
            gr_env_vars: Weight {
                ref_time: 1032560,
                proof_size: 0,
            },
            gr_block_height: Weight {
                ref_time: 902492,
                proof_size: 0,
            },
            gr_block_timestamp: Weight {
                ref_time: 897276,
                proof_size: 0,
            },
            gr_random: Weight {
                ref_time: 1815295,
                proof_size: 0,
            },
            gr_reply_deposit: Weight {
                ref_time: 6198717,
                proof_size: 0,
            },
            gr_send: Weight {
                ref_time: 2948853,
                proof_size: 0,
            },
            gr_send_per_byte: Weight {
                ref_time: 502,
                proof_size: 0,
            },
            gr_send_wgas: Weight {
                ref_time: 2985312,
                proof_size: 0,
            },
            gr_send_wgas_per_byte: Weight {
                ref_time: 501,
                proof_size: 0,
            },
            gr_send_init: Weight {
                ref_time: 1098261,
                proof_size: 0,
            },
            gr_send_push: Weight {
                ref_time: 2021961,
                proof_size: 0,
            },
            gr_send_push_per_byte: Weight {
                ref_time: 503,
                proof_size: 0,
            },
            gr_send_commit: Weight {
                ref_time: 2453953,
                proof_size: 0,
            },
            gr_send_commit_wgas: Weight {
                ref_time: 2493379,
                proof_size: 0,
            },
            gr_reservation_send: Weight {
                ref_time: 3161393,
                proof_size: 0,
            },
            gr_reservation_send_per_byte: Weight {
                ref_time: 503,
                proof_size: 0,
            },
            gr_reservation_send_commit: Weight {
                ref_time: 2661854,
                proof_size: 0,
            },
            gr_reply_commit: Weight {
                ref_time: 20275066,
                proof_size: 0,
            },
            gr_reply_commit_wgas: Weight {
                ref_time: 20158062,
                proof_size: 0,
            },
            gr_reservation_reply: Weight {
                ref_time: 7839490,
                proof_size: 0,
            },
            gr_reservation_reply_per_byte: Weight {
                ref_time: 675230,
                proof_size: 0,
            },
            gr_reservation_reply_commit: Weight {
                ref_time: 6918144,
                proof_size: 0,
            },
            gr_reply_push: Weight {
                ref_time: 1736576,
                proof_size: 0,
            },
            gr_reply: Weight {
                ref_time: 21457412,
                proof_size: 0,
            },
            gr_reply_per_byte: Weight {
                ref_time: 657,
                proof_size: 0,
            },
            gr_reply_wgas: Weight {
                ref_time: 20948196,
                proof_size: 0,
            },
            gr_reply_wgas_per_byte: Weight {
                ref_time: 659,
                proof_size: 0,
            },
            gr_reply_push_per_byte: Weight {
                ref_time: 656,
                proof_size: 0,
            },
            gr_reply_to: Weight {
                ref_time: 924572,
                proof_size: 0,
            },
            gr_signal_code: Weight {
                ref_time: 892505,
                proof_size: 0,
            },
            gr_signal_from: Weight {
                ref_time: 933365,
                proof_size: 0,
            },
            gr_reply_input: Weight {
                ref_time: 19389162,
                proof_size: 0,
            },
            gr_reply_input_wgas: Weight {
                ref_time: 24915830,
                proof_size: 0,
            },
            gr_reply_push_input: Weight {
                ref_time: 1167299,
                proof_size: 0,
            },
            gr_reply_push_input_per_byte: Weight {
                ref_time: 143,
                proof_size: 0,
            },
            gr_send_input: Weight {
                ref_time: 2834923,
                proof_size: 0,
            },
            gr_send_input_wgas: Weight {
                ref_time: 2867253,
                proof_size: 0,
            },
            gr_send_push_input: Weight {
                ref_time: 1452976,
                proof_size: 0,
            },
            gr_send_push_input_per_byte: Weight {
                ref_time: 163,
                proof_size: 0,
            },
            gr_debug: Weight {
                ref_time: 1212178,
                proof_size: 0,
            },
            gr_debug_per_byte: Weight {
                ref_time: 449,
                proof_size: 0,
            },
            gr_reply_code: Weight {
                ref_time: 923322,
                proof_size: 0,
            },
            gr_exit: Weight {
                ref_time: 23384948,
                proof_size: 0,
            },
            gr_leave: Weight {
                ref_time: 12007632,
                proof_size: 0,
            },
            gr_wait: Weight {
                ref_time: 10988944,
                proof_size: 0,
            },
            gr_wait_for: Weight {
                ref_time: 11474984,
                proof_size: 0,
            },
            gr_wait_up_to: Weight {
                ref_time: 11597666,
                proof_size: 0,
            },
            gr_wake: Weight {
                ref_time: 3727659,
                proof_size: 0,
            },
            gr_create_program: Weight {
                ref_time: 3745776,
                proof_size: 0,
            },
            gr_create_program_payload_per_byte: Weight {
                ref_time: 118,
                proof_size: 0,
            },
            gr_create_program_salt_per_byte: Weight {
                ref_time: 1411,
                proof_size: 0,
            },
            gr_create_program_wgas: Weight {
                ref_time: 3787131,
                proof_size: 0,
            },
            gr_create_program_wgas_payload_per_byte: Weight {
                ref_time: 118,
                proof_size: 0,
            },
            gr_create_program_wgas_salt_per_byte: Weight {
                ref_time: 1414,
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
    pub mem_grow_per_page: Weight,
    pub parachain_read_heuristic: Weight,
}

impl Default for MemoryWeights {
    fn default() -> Self {
        Self {
            lazy_pages_signal_read: Weight {
                ref_time: 28385632,
                proof_size: 0,
            },
            lazy_pages_signal_write: Weight {
                ref_time: 33746629,
                proof_size: 0,
            },
            lazy_pages_signal_write_after_read: Weight {
                ref_time: 8663807,
                proof_size: 0,
            },
            lazy_pages_host_func_read: Weight {
                ref_time: 31201248,
                proof_size: 0,
            },
            lazy_pages_host_func_write: Weight {
                ref_time: 37498840,
                proof_size: 0,
            },
            lazy_pages_host_func_write_after_read: Weight {
                ref_time: 11240289,
                proof_size: 0,
            },
            load_page_data: Weight {
                ref_time: 10630903,
                proof_size: 0,
            },
            upload_page_data: Weight {
                ref_time: 103888768,
                proof_size: 0,
            },
            static_page: Weight {
                ref_time: 100,
                proof_size: 0,
            },
            mem_grow: Weight {
                ref_time: 810343,
                proof_size: 0,
            },
            mem_grow_per_page: Weight {
                ref_time: 0,
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
