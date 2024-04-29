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
                ref_time: 517,
                proof_size: 0,
            },
            db_write_per_byte: Weight {
                ref_time: 212,
                proof_size: 0,
            },
            db_read_per_byte: Weight {
                ref_time: 647,
                proof_size: 0,
            },
            code_instrumentation_cost: Weight {
                ref_time: 309169000,
                proof_size: 3682,
            },
            code_instrumentation_byte_cost: Weight {
                ref_time: 61877,
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
            i64const: 139,
            i64load: 4997,
            i32load: 6811,
            i64store: 14615,
            i32store: 12942,
            select: 8457,
            r#if: 6545,
            br: 3374,
            br_if: 5097,
            br_table: 11982,
            br_table_per_entry: 95,
            call: 4383,
            call_indirect: 17057,
            call_indirect_per_param: 2378,
            call_per_local: 0,
            local_get: 1309,
            local_set: 2519,
            local_tee: 3164,
            global_get: 1789,
            global_set: 3302,
            memory_current: 11063,
            i64clz: 5984,
            i32clz: 6498,
            i64ctz: 6804,
            i32ctz: 5622,
            i64popcnt: 2256,
            i32popcnt: 1605,
            i64eqz: 4026,
            i32eqz: 2321,
            i32extend8s: 1624,
            i32extend16s: 1563,
            i64extend8s: 2053,
            i64extend16s: 1876,
            i64extend32s: 1812,
            i64extendsi32: 928,
            i64extendui32: 1008,
            i32wrapi64: 877,
            i64eq: 4872,
            i32eq: 3359,
            i64ne: 5016,
            i32ne: 2699,
            i64lts: 4671,
            i32lts: 2827,
            i64ltu: 4852,
            i32ltu: 3143,
            i64gts: 3490,
            i32gts: 2745,
            i64gtu: 4084,
            i32gtu: 2684,
            i64les: 4793,
            i32les: 2990,
            i64leu: 3857,
            i32leu: 3348,
            i64ges: 4980,
            i32ges: 3636,
            i64geu: 5290,
            i32geu: 2539,
            i64add: 3896,
            i32add: 2120,
            i64sub: 3859,
            i32sub: 2302,
            i64mul: 4768,
            i32mul: 2823,
            i64divs: 6282,
            i32divs: 4897,
            i64divu: 7265,
            i32divu: 5630,
            i64rems: 18531,
            i32rems: 12815,
            i64remu: 6959,
            i32remu: 6367,
            i64and: 3889,
            i32and: 1246,
            i64or: 3762,
            i32or: 1803,
            i64xor: 3971,
            i32xor: 2241,
            i64shl: 2773,
            i32shl: 1952,
            i64shrs: 3141,
            i32shrs: 1641,
            i64shru: 3501,
            i32shru: 2052,
            i64rotl: 2881,
            i32rotl: 2205,
            i64rotr: 3302,
            i32rotr: 2139,
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
                ref_time: 1506316,
                proof_size: 0,
            },
            free: Weight {
                ref_time: 944618,
                proof_size: 0,
            },
            free_range: Weight {
                ref_time: 955713,
                proof_size: 0,
            },
            free_range_per_page: Weight {
                ref_time: 38966,
                proof_size: 0,
            },
            gr_reserve_gas: Weight {
                ref_time: 2224275,
                proof_size: 0,
            },
            gr_unreserve_gas: Weight {
                ref_time: 2045606,
                proof_size: 0,
            },
            gr_system_reserve_gas: Weight {
                ref_time: 1149969,
                proof_size: 0,
            },
            gr_gas_available: Weight {
                ref_time: 954171,
                proof_size: 0,
            },
            gr_message_id: Weight {
                ref_time: 1027623,
                proof_size: 0,
            },
            gr_program_id: Weight {
                ref_time: 956072,
                proof_size: 0,
            },
            gr_source: Weight {
                ref_time: 1030350,
                proof_size: 0,
            },
            gr_value: Weight {
                ref_time: 959888,
                proof_size: 0,
            },
            gr_value_available: Weight {
                ref_time: 1021879,
                proof_size: 0,
            },
            gr_size: Weight {
                ref_time: 947494,
                proof_size: 0,
            },
            gr_read: Weight {
                ref_time: 1816928,
                proof_size: 0,
            },
            gr_read_per_byte: Weight {
                ref_time: 175,
                proof_size: 0,
            },
            gr_env_vars: Weight {
                ref_time: 1102873,
                proof_size: 0,
            },
            gr_block_height: Weight {
                ref_time: 979743,
                proof_size: 0,
            },
            gr_block_timestamp: Weight {
                ref_time: 946706,
                proof_size: 0,
            },
            gr_random: Weight {
                ref_time: 2081853,
                proof_size: 0,
            },
            gr_reply_deposit: Weight {
                ref_time: 6374293,
                proof_size: 0,
            },
            gr_send: Weight {
                ref_time: 3165081,
                proof_size: 0,
            },
            gr_send_per_byte: Weight {
                ref_time: 433,
                proof_size: 0,
            },
            gr_send_wgas: Weight {
                ref_time: 3197413,
                proof_size: 0,
            },
            gr_send_wgas_per_byte: Weight {
                ref_time: 429,
                proof_size: 0,
            },
            gr_send_init: Weight {
                ref_time: 1048953,
                proof_size: 0,
            },
            gr_send_push: Weight {
                ref_time: 2047352,
                proof_size: 0,
            },
            gr_send_push_per_byte: Weight {
                ref_time: 431,
                proof_size: 0,
            },
            gr_send_commit: Weight {
                ref_time: 2648913,
                proof_size: 0,
            },
            gr_send_commit_wgas: Weight {
                ref_time: 2708890,
                proof_size: 0,
            },
            gr_reservation_send: Weight {
                ref_time: 3408308,
                proof_size: 0,
            },
            gr_reservation_send_per_byte: Weight {
                ref_time: 436,
                proof_size: 0,
            },
            gr_reservation_send_commit: Weight {
                ref_time: 2845154,
                proof_size: 0,
            },
            gr_reply_commit: Weight {
                ref_time: 21471336,
                proof_size: 0,
            },
            gr_reply_commit_wgas: Weight {
                ref_time: 21687794,
                proof_size: 0,
            },
            gr_reservation_reply: Weight {
                ref_time: 10138892,
                proof_size: 0,
            },
            gr_reservation_reply_per_byte: Weight {
                ref_time: 614608,
                proof_size: 0,
            },
            gr_reservation_reply_commit: Weight {
                ref_time: 7627564,
                proof_size: 0,
            },
            gr_reply_push: Weight {
                ref_time: 1774396,
                proof_size: 0,
            },
            gr_reply: Weight {
                ref_time: 23386704,
                proof_size: 0,
            },
            gr_reply_per_byte: Weight {
                ref_time: 609,
                proof_size: 0,
            },
            gr_reply_wgas: Weight {
                ref_time: 22493056,
                proof_size: 0,
            },
            gr_reply_wgas_per_byte: Weight {
                ref_time: 625,
                proof_size: 0,
            },
            gr_reply_push_per_byte: Weight {
                ref_time: 730,
                proof_size: 0,
            },
            gr_reply_to: Weight {
                ref_time: 967667,
                proof_size: 0,
            },
            gr_signal_code: Weight {
                ref_time: 960874,
                proof_size: 0,
            },
            gr_signal_from: Weight {
                ref_time: 973437,
                proof_size: 0,
            },
            gr_reply_input: Weight {
                ref_time: 11220658,
                proof_size: 0,
            },
            gr_reply_input_wgas: Weight {
                ref_time: 55725388,
                proof_size: 0,
            },
            gr_reply_push_input: Weight {
                ref_time: 1185177,
                proof_size: 0,
            },
            gr_reply_push_input_per_byte: Weight {
                ref_time: 118,
                proof_size: 0,
            },
            gr_send_input: Weight {
                ref_time: 3114534,
                proof_size: 0,
            },
            gr_send_input_wgas: Weight {
                ref_time: 3076974,
                proof_size: 0,
            },
            gr_send_push_input: Weight {
                ref_time: 1553684,
                proof_size: 0,
            },
            gr_send_push_input_per_byte: Weight {
                ref_time: 155,
                proof_size: 0,
            },
            gr_debug: Weight {
                ref_time: 1308527,
                proof_size: 0,
            },
            gr_debug_per_byte: Weight {
                ref_time: 381,
                proof_size: 0,
            },
            gr_reply_code: Weight {
                ref_time: 965095,
                proof_size: 0,
            },
            gr_exit: Weight {
                ref_time: 26167562,
                proof_size: 0,
            },
            gr_leave: Weight {
                ref_time: 14672752,
                proof_size: 0,
            },
            gr_wait: Weight {
                ref_time: 8622556,
                proof_size: 0,
            },
            gr_wait_for: Weight {
                ref_time: 8595760,
                proof_size: 0,
            },
            gr_wait_up_to: Weight {
                ref_time: 11017502,
                proof_size: 0,
            },
            gr_wake: Weight {
                ref_time: 3718575,
                proof_size: 0,
            },
            gr_create_program: Weight {
                ref_time: 4030689,
                proof_size: 0,
            },
            gr_create_program_payload_per_byte: Weight {
                ref_time: 103,
                proof_size: 0,
            },
            gr_create_program_salt_per_byte: Weight {
                ref_time: 1922,
                proof_size: 0,
            },
            gr_create_program_wgas: Weight {
                ref_time: 4136806,
                proof_size: 0,
            },
            gr_create_program_wgas_payload_per_byte: Weight {
                ref_time: 108,
                proof_size: 0,
            },
            gr_create_program_wgas_salt_per_byte: Weight {
                ref_time: 1919,
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
                ref_time: 28629679,
                proof_size: 0,
            },
            lazy_pages_signal_write: Weight {
                ref_time: 36308639,
                proof_size: 0,
            },
            lazy_pages_signal_write_after_read: Weight {
                ref_time: 11104186,
                proof_size: 0,
            },
            lazy_pages_host_func_read: Weight {
                ref_time: 29827587,
                proof_size: 0,
            },
            lazy_pages_host_func_write: Weight {
                ref_time: 37755699,
                proof_size: 0,
            },
            lazy_pages_host_func_write_after_read: Weight {
                ref_time: 9998196,
                proof_size: 0,
            },
            load_page_data: Weight {
                ref_time: 10981376,
                proof_size: 0,
            },
            upload_page_data: Weight {
                ref_time: 103475280,
                proof_size: 0,
            },
            static_page: Weight {
                ref_time: 100,
                proof_size: 0,
            },
            mem_grow: Weight {
                ref_time: 779042,
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
