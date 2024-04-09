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
                ref_time: 251,
                proof_size: 0,
            },
            db_write_per_byte: Weight {
                ref_time: 240,
                proof_size: 0,
            },
            db_read_per_byte: Weight {
                ref_time: 571,
                proof_size: 0,
            },
            code_instrumentation_cost: Weight {
                ref_time: 298878000,
                proof_size: 3682,
            },
            code_instrumentation_byte_cost: Weight {
                ref_time: 61212,
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
            i64const: 148,
            i64load: 6653,
            i32load: 6543,
            i64store: 12274,
            i32store: 12978,
            select: 4730,
            r#if: 4190,
            br: 3160,
            br_if: 3613,
            br_table: 7898,
            br_table_per_entry: 108,
            call: 4293,
            call_indirect: 16563,
            call_indirect_per_param: 926,
            call_per_local: 0,
            local_get: 702,
            local_set: 1405,
            local_tee: 1350,
            global_get: 687,
            global_set: 1035,
            memory_current: 11700,
            i64clz: 4139,
            i32clz: 3925,
            i64ctz: 3909,
            i32ctz: 3571,
            i64popcnt: 565,
            i32popcnt: 392,
            i64eqz: 1589,
            i32eqz: 1202,
            i32extend8s: 548,
            i32extend16s: 552,
            i64extend8s: 589,
            i64extend16s: 549,
            i64extend32s: 488,
            i64extendsi32: 510,
            i64extendui32: 323,
            i32wrapi64: 231,
            i64eq: 1774,
            i32eq: 1188,
            i64ne: 2737,
            i32ne: 2285,
            i64lts: 2332,
            i32lts: 1408,
            i64ltu: 2645,
            i32ltu: 1045,
            i64gts: 1868,
            i32gts: 1158,
            i64gtu: 2008,
            i32gtu: 2198,
            i64les: 2421,
            i32les: 994,
            i64leu: 2516,
            i32leu: 988,
            i64ges: 1847,
            i32ges: 893,
            i64geu: 1412,
            i32geu: 938,
            i64add: 1141,
            i32add: 680,
            i64sub: 1343,
            i32sub: 702,
            i64mul: 1647,
            i32mul: 1120,
            i64divs: 2763,
            i32divs: 2237,
            i64divu: 2901,
            i32divu: 2204,
            i64rems: 10776,
            i32rems: 7125,
            i64remu: 3956,
            i32remu: 2687,
            i64and: 1220,
            i32and: 783,
            i64or: 1625,
            i32or: 627,
            i64xor: 1227,
            i32xor: 698,
            i64shl: 1253,
            i32shl: 625,
            i64shrs: 1372,
            i32shrs: 587,
            i64shru: 1585,
            i32shru: 574,
            i64rotl: 1537,
            i32rotl: 734,
            i64rotr: 1388,
            i32rotr: 676,
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
                ref_time: 8171686,
                proof_size: 0,
            },
            alloc_per_page: Weight {
                ref_time: 258440,
                proof_size: 0,
            },
            free: Weight {
                ref_time: 637617,
                proof_size: 0,
            },
            free_range: Weight {
                ref_time: 779178,
                proof_size: 0,
            },
            free_range_per_page: Weight {
                ref_time: 60969,
                proof_size: 0,
            },
            gr_reserve_gas: Weight {
                ref_time: 2274859,
                proof_size: 0,
            },
            gr_unreserve_gas: Weight {
                ref_time: 2010881,
                proof_size: 0,
            },
            gr_system_reserve_gas: Weight {
                ref_time: 1077846,
                proof_size: 0,
            },
            gr_gas_available: Weight {
                ref_time: 938213,
                proof_size: 0,
            },
            gr_message_id: Weight {
                ref_time: 935891,
                proof_size: 0,
            },
            gr_program_id: Weight {
                ref_time: 936894,
                proof_size: 0,
            },
            gr_source: Weight {
                ref_time: 937462,
                proof_size: 0,
            },
            gr_value: Weight {
                ref_time: 951772,
                proof_size: 0,
            },
            gr_value_available: Weight {
                ref_time: 946533,
                proof_size: 0,
            },
            gr_size: Weight {
                ref_time: 926935,
                proof_size: 0,
            },
            gr_read: Weight {
                ref_time: 1712676,
                proof_size: 0,
            },
            gr_read_per_byte: Weight {
                ref_time: 169,
                proof_size: 0,
            },
            gr_env_vars: Weight {
                ref_time: 1080182,
                proof_size: 0,
            },
            gr_block_height: Weight {
                ref_time: 921134,
                proof_size: 0,
            },
            gr_block_timestamp: Weight {
                ref_time: 928382,
                proof_size: 0,
            },
            gr_random: Weight {
                ref_time: 1919444,
                proof_size: 0,
            },
            gr_reply_deposit: Weight {
                ref_time: 6317130,
                proof_size: 0,
            },
            gr_send: Weight {
                ref_time: 3210858,
                proof_size: 0,
            },
            gr_send_per_byte: Weight {
                ref_time: 385,
                proof_size: 0,
            },
            gr_send_wgas: Weight {
                ref_time: 3269042,
                proof_size: 0,
            },
            gr_send_wgas_per_byte: Weight {
                ref_time: 386,
                proof_size: 0,
            },
            gr_send_init: Weight {
                ref_time: 1031434,
                proof_size: 0,
            },
            gr_send_push: Weight {
                ref_time: 1950042,
                proof_size: 0,
            },
            gr_send_push_per_byte: Weight {
                ref_time: 379,
                proof_size: 0,
            },
            gr_send_commit: Weight {
                ref_time: 2711662,
                proof_size: 0,
            },
            gr_send_commit_wgas: Weight {
                ref_time: 2721465,
                proof_size: 0,
            },
            gr_reservation_send: Weight {
                ref_time: 3416918,
                proof_size: 0,
            },
            gr_reservation_send_per_byte: Weight {
                ref_time: 385,
                proof_size: 0,
            },
            gr_reservation_send_commit: Weight {
                ref_time: 2934376,
                proof_size: 0,
            },
            gr_reply_commit: Weight {
                ref_time: 57281442,
                proof_size: 0,
            },
            gr_reply_commit_wgas: Weight {
                ref_time: 57943462,
                proof_size: 0,
            },
            gr_reservation_reply: Weight {
                ref_time: 50825214,
                proof_size: 0,
            },
            gr_reservation_reply_per_byte: Weight {
                ref_time: 682811,
                proof_size: 0,
            },
            gr_reservation_reply_commit: Weight {
                ref_time: 44558068,
                proof_size: 0,
            },
            gr_reply_push: Weight {
                ref_time: 1698001,
                proof_size: 0,
            },
            gr_reply: Weight {
                ref_time: 59587476,
                proof_size: 0,
            },
            gr_reply_per_byte: Weight {
                ref_time: 666,
                proof_size: 0,
            },
            gr_reply_wgas: Weight {
                ref_time: 62502718,
                proof_size: 0,
            },
            gr_reply_wgas_per_byte: Weight {
                ref_time: 663,
                proof_size: 0,
            },
            gr_reply_push_per_byte: Weight {
                ref_time: 785,
                proof_size: 0,
            },
            gr_reply_to: Weight {
                ref_time: 955079,
                proof_size: 0,
            },
            gr_signal_code: Weight {
                ref_time: 917529,
                proof_size: 0,
            },
            gr_signal_from: Weight {
                ref_time: 944538,
                proof_size: 0,
            },
            gr_reply_input: Weight {
                ref_time: 51687606,
                proof_size: 0,
            },
            gr_reply_input_wgas: Weight {
                ref_time: 43750150,
                proof_size: 0,
            },
            gr_reply_push_input: Weight {
                ref_time: 1207793,
                proof_size: 0,
            },
            gr_reply_push_input_per_byte: Weight {
                ref_time: 150,
                proof_size: 0,
            },
            gr_send_input: Weight {
                ref_time: 3079573,
                proof_size: 0,
            },
            gr_send_input_wgas: Weight {
                ref_time: 3171335,
                proof_size: 0,
            },
            gr_send_push_input: Weight {
                ref_time: 1500226,
                proof_size: 0,
            },
            gr_send_push_input_per_byte: Weight {
                ref_time: 168,
                proof_size: 0,
            },
            gr_debug: Weight {
                ref_time: 1260677,
                proof_size: 0,
            },
            gr_debug_per_byte: Weight {
                ref_time: 309,
                proof_size: 0,
            },
            gr_reply_code: Weight {
                ref_time: 922370,
                proof_size: 0,
            },
            gr_exit: Weight {
                ref_time: 52618926,
                proof_size: 0,
            },
            gr_leave: Weight {
                ref_time: 12314724,
                proof_size: 0,
            },
            gr_wait: Weight {
                ref_time: 8973480,
                proof_size: 0,
            },
            gr_wait_for: Weight {
                ref_time: 10726118,
                proof_size: 0,
            },
            gr_wait_up_to: Weight {
                ref_time: 11342806,
                proof_size: 0,
            },
            gr_wake: Weight {
                ref_time: 3527824,
                proof_size: 0,
            },
            gr_create_program: Weight {
                ref_time: 4111651,
                proof_size: 0,
            },
            gr_create_program_payload_per_byte: Weight {
                ref_time: 103,
                proof_size: 0,
            },
            gr_create_program_salt_per_byte: Weight {
                ref_time: 1871,
                proof_size: 0,
            },
            gr_create_program_wgas: Weight {
                ref_time: 4177259,
                proof_size: 0,
            },
            gr_create_program_wgas_payload_per_byte: Weight {
                ref_time: 83,
                proof_size: 0,
            },
            gr_create_program_wgas_salt_per_byte: Weight {
                ref_time: 1864,
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
                ref_time: 28819751,
                proof_size: 0,
            },
            lazy_pages_signal_write: Weight {
                ref_time: 35200022,
                proof_size: 0,
            },
            lazy_pages_signal_write_after_read: Weight {
                ref_time: 10486437,
                proof_size: 0,
            },
            lazy_pages_host_func_read: Weight {
                ref_time: 30144063,
                proof_size: 0,
            },
            lazy_pages_host_func_write: Weight {
                ref_time: 34257106,
                proof_size: 0,
            },
            lazy_pages_host_func_write_after_read: Weight {
                ref_time: 10240575,
                proof_size: 0,
            },
            load_page_data: Weight {
                ref_time: 10211517,
                proof_size: 0,
            },
            upload_page_data: Weight {
                ref_time: 103935856,
                proof_size: 0,
            },
            static_page: Weight {
                ref_time: 100,
                proof_size: 0,
            },
            mem_grow: Weight {
                ref_time: 1141541,
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
