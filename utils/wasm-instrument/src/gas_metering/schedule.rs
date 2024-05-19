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
                ref_time: 255,
                proof_size: 0,
            },
            db_write_per_byte: Weight {
                ref_time: 238,
                proof_size: 0,
            },
            db_read_per_byte: Weight {
                ref_time: 584,
                proof_size: 0,
            },
            code_instrumentation_cost: Weight {
                ref_time: 299592000,
                proof_size: 3682,
            },
            code_instrumentation_byte_cost: Weight {
                ref_time: 60235,
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
            i64const: 139,
            i64load: 6691,
            i32load: 6697,
            i64store: 15437,
            i32store: 16097,
            select: 4454,
            r#if: 4343,
            br: 3162,
            br_if: 3589,
            br_table: 8591,
            br_table_per_entry: 97,
            call: 4338,
            call_indirect: 15343,
            call_indirect_per_param: 1091,
            call_per_local: 0,
            local_get: 696,
            local_set: 1353,
            local_tee: 1353,
            global_get: 673,
            global_set: 983,
            memory_current: 10963,
            i64clz: 4053,
            i32clz: 3854,
            i64ctz: 4006,
            i32ctz: 3674,
            i64popcnt: 586,
            i32popcnt: 370,
            i64eqz: 1350,
            i32eqz: 1179,
            i32extend8s: 546,
            i32extend16s: 675,
            i64extend8s: 732,
            i64extend16s: 681,
            i64extend32s: 544,
            i64extendsi32: 623,
            i64extendui32: 338,
            i32wrapi64: 254,
            i64eq: 1813,
            i32eq: 1058,
            i64ne: 1654,
            i32ne: 982,
            i64lts: 1222,
            i32lts: 708,
            i64ltu: 1118,
            i32ltu: 769,
            i64gts: 1286,
            i32gts: 734,
            i64gtu: 1143,
            i32gtu: 732,
            i64les: 1132,
            i32les: 684,
            i64leu: 1248,
            i32leu: 699,
            i64ges: 1302,
            i32ges: 763,
            i64geu: 1371,
            i32geu: 858,
            i64add: 1361,
            i32add: 618,
            i64sub: 1262,
            i32sub: 660,
            i64mul: 1583,
            i32mul: 1678,
            i64divs: 3047,
            i32divs: 2396,
            i64divu: 3177,
            i32divu: 2469,
            i64rems: 11818,
            i32rems: 7504,
            i64remu: 3369,
            i32remu: 2246,
            i64and: 1113,
            i32and: 877,
            i64or: 1245,
            i32or: 558,
            i64xor: 1084,
            i32xor: 588,
            i64shl: 918,
            i32shl: 463,
            i64shrs: 896,
            i32shrs: 494,
            i64shru: 816,
            i32shru: 476,
            i64rotl: 1350,
            i32rotl: 419,
            i64rotr: 924,
            i32rotr: 484,
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
                ref_time: 8229320,
                proof_size: 0,
            },
            free: Weight {
                ref_time: 628089,
                proof_size: 0,
            },
            free_range: Weight {
                ref_time: 772457,
                proof_size: 0,
            },
            free_range_per_page: Weight {
                ref_time: 63356,
                proof_size: 0,
            },
            gr_reserve_gas: Weight {
                ref_time: 2264169,
                proof_size: 0,
            },
            gr_unreserve_gas: Weight {
                ref_time: 1946607,
                proof_size: 0,
            },
            gr_system_reserve_gas: Weight {
                ref_time: 1046098,
                proof_size: 0,
            },
            gr_gas_available: Weight {
                ref_time: 932311,
                proof_size: 0,
            },
            gr_message_id: Weight {
                ref_time: 926503,
                proof_size: 0,
            },
            gr_program_id: Weight {
                ref_time: 930098,
                proof_size: 0,
            },
            gr_source: Weight {
                ref_time: 930744,
                proof_size: 0,
            },
            gr_value: Weight {
                ref_time: 945665,
                proof_size: 0,
            },
            gr_value_available: Weight {
                ref_time: 967869,
                proof_size: 0,
            },
            gr_size: Weight {
                ref_time: 926934,
                proof_size: 0,
            },
            gr_read: Weight {
                ref_time: 1672342,
                proof_size: 0,
            },
            gr_read_per_byte: Weight {
                ref_time: 157,
                proof_size: 0,
            },
            gr_env_vars: Weight {
                ref_time: 1038244,
                proof_size: 0,
            },
            gr_block_height: Weight {
                ref_time: 925915,
                proof_size: 0,
            },
            gr_block_timestamp: Weight {
                ref_time: 932995,
                proof_size: 0,
            },
            gr_random: Weight {
                ref_time: 1886715,
                proof_size: 0,
            },
            gr_reply_deposit: Weight {
                ref_time: 6537061,
                proof_size: 0,
            },
            gr_send: Weight {
                ref_time: 3192510,
                proof_size: 0,
            },
            gr_send_per_byte: Weight {
                ref_time: 373,
                proof_size: 0,
            },
            gr_send_wgas: Weight {
                ref_time: 3251723,
                proof_size: 0,
            },
            gr_send_wgas_per_byte: Weight {
                ref_time: 382,
                proof_size: 0,
            },
            gr_send_init: Weight {
                ref_time: 1034507,
                proof_size: 0,
            },
            gr_send_push: Weight {
                ref_time: 1993804,
                proof_size: 0,
            },
            gr_send_push_per_byte: Weight {
                ref_time: 379,
                proof_size: 0,
            },
            gr_send_commit: Weight {
                ref_time: 2677702,
                proof_size: 0,
            },
            gr_send_commit_wgas: Weight {
                ref_time: 2671610,
                proof_size: 0,
            },
            gr_reservation_send: Weight {
                ref_time: 3386292,
                proof_size: 0,
            },
            gr_reservation_send_per_byte: Weight {
                ref_time: 376,
                proof_size: 0,
            },
            gr_reservation_send_commit: Weight {
                ref_time: 2867905,
                proof_size: 0,
            },
            gr_reply_commit: Weight {
                ref_time: 21336676,
                proof_size: 0,
            },
            gr_reply_commit_wgas: Weight {
                ref_time: 19220242,
                proof_size: 0,
            },
            gr_reservation_reply: Weight {
                ref_time: 8253250,
                proof_size: 0,
            },
            gr_reservation_reply_per_byte: Weight {
                ref_time: 584512,
                proof_size: 0,
            },
            gr_reservation_reply_commit: Weight {
                ref_time: 10434360,
                proof_size: 0,
            },
            gr_reply_push: Weight {
                ref_time: 1695918,
                proof_size: 0,
            },
            gr_reply: Weight {
                ref_time: 22480174,
                proof_size: 0,
            },
            gr_reply_per_byte: Weight {
                ref_time: 564,
                proof_size: 0,
            },
            gr_reply_wgas: Weight {
                ref_time: 21374240,
                proof_size: 0,
            },
            gr_reply_wgas_per_byte: Weight {
                ref_time: 575,
                proof_size: 0,
            },
            gr_reply_push_per_byte: Weight {
                ref_time: 640,
                proof_size: 0,
            },
            gr_reply_to: Weight {
                ref_time: 950250,
                proof_size: 0,
            },
            gr_signal_code: Weight {
                ref_time: 962511,
                proof_size: 0,
            },
            gr_signal_from: Weight {
                ref_time: 941460,
                proof_size: 0,
            },
            gr_reply_input: Weight {
                ref_time: 25865578,
                proof_size: 0,
            },
            gr_reply_input_wgas: Weight {
                ref_time: 24582802,
                proof_size: 0,
            },
            gr_reply_push_input: Weight {
                ref_time: 1153424,
                proof_size: 0,
            },
            gr_reply_push_input_per_byte: Weight {
                ref_time: 146,
                proof_size: 0,
            },
            gr_send_input: Weight {
                ref_time: 3110053,
                proof_size: 0,
            },
            gr_send_input_wgas: Weight {
                ref_time: 3075631,
                proof_size: 0,
            },
            gr_send_push_input: Weight {
                ref_time: 1520063,
                proof_size: 0,
            },
            gr_send_push_input_per_byte: Weight {
                ref_time: 165,
                proof_size: 0,
            },
            gr_debug: Weight {
                ref_time: 1212358,
                proof_size: 0,
            },
            gr_debug_per_byte: Weight {
                ref_time: 316,
                proof_size: 0,
            },
            gr_reply_code: Weight {
                ref_time: 919778,
                proof_size: 0,
            },
            gr_exit: Weight {
                ref_time: 24122888,
                proof_size: 0,
            },
            gr_leave: Weight {
                ref_time: 12535280,
                proof_size: 0,
            },
            gr_wait: Weight {
                ref_time: 11429924,
                proof_size: 0,
            },
            gr_wait_for: Weight {
                ref_time: 9854244,
                proof_size: 0,
            },
            gr_wait_up_to: Weight {
                ref_time: 11603952,
                proof_size: 0,
            },
            gr_wake: Weight {
                ref_time: 3682047,
                proof_size: 0,
            },
            gr_create_program: Weight {
                ref_time: 4078859,
                proof_size: 0,
            },
            gr_create_program_payload_per_byte: Weight {
                ref_time: 78,
                proof_size: 0,
            },
            gr_create_program_salt_per_byte: Weight {
                ref_time: 1872,
                proof_size: 0,
            },
            gr_create_program_wgas: Weight {
                ref_time: 4127488,
                proof_size: 0,
            },
            gr_create_program_wgas_payload_per_byte: Weight {
                ref_time: 88,
                proof_size: 0,
            },
            gr_create_program_wgas_salt_per_byte: Weight {
                ref_time: 1870,
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
    // TODO: use real weight and add `mem_grow_per_page` #3970
    pub mem_grow: Weight,
    pub parachain_read_heuristic: Weight,
}

impl Default for MemoryWeights {
    fn default() -> Self {
        Self {
            lazy_pages_signal_read: Weight {
                ref_time: 28765030,
                proof_size: 0,
            },
            lazy_pages_signal_write: Weight {
                ref_time: 34699068,
                proof_size: 0,
            },
            lazy_pages_signal_write_after_read: Weight {
                ref_time: 10094010,
                proof_size: 0,
            },
            lazy_pages_host_func_read: Weight {
                ref_time: 29972769,
                proof_size: 0,
            },
            lazy_pages_host_func_write: Weight {
                ref_time: 34180401,
                proof_size: 0,
            },
            lazy_pages_host_func_write_after_read: Weight {
                ref_time: 9259113,
                proof_size: 0,
            },
            load_page_data: Weight {
                ref_time: 10396457,
                proof_size: 0,
            },
            upload_page_data: Weight {
                ref_time: 103915328,
                proof_size: 0,
            },
            static_page: Weight {
                ref_time: 100,
                proof_size: 0,
            },
            mem_grow: Weight {
                ref_time: 1107477,
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
