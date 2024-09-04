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
    pub instantiation_weights: InstantiationWeights,
    pub db_write_per_byte: Weight,
    pub db_read_per_byte: Weight,
    pub code_instrumentation_cost: Weight,
    pub code_instrumentation_byte_cost: Weight,
    pub load_allocations_weight: Weight,
}

impl Default for Schedule {
    fn default() -> Self {
        Self {
            limits: Limits::default(),
            instruction_weights: InstructionWeights::default(),
            syscall_weights: SyscallWeights::default(),
            memory_weights: MemoryWeights::default(),
            instantiation_weights: InstantiationWeights::default(),
            db_write_per_byte: Weight {
                ref_time: 234,
                proof_size: 0,
            },
            db_read_per_byte: Weight {
                ref_time: 569,
                proof_size: 0,
            },
            code_instrumentation_cost: Weight {
                ref_time: 306821000,
                proof_size: 3793,
            },
            code_instrumentation_byte_cost: Weight {
                ref_time: 627777,
                proof_size: 0,
            },
            load_allocations_weight: Weight {
                ref_time: 20729,
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
            memory_pages: 32768,
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
            version: 1501,
            i64const: 186,
            i64load: 5844,
            i32load: 5810,
            i64store: 10110,
            i32store: 10955,
            select: 6108,
            r#if: 4860,
            br: 3318,
            br_if: 5383,
            br_table: 10309,
            br_table_per_entry: 162,
            call: 4691,
            call_indirect: 21442,
            call_indirect_per_param: 1259,
            call_per_local: 0,
            local_get: 682,
            local_set: 1322,
            local_tee: 1291,
            global_get: 642,
            global_set: 1243,
            memory_current: 12424,
            i64clz: 386,
            i32clz: 258,
            i64ctz: 404,
            i32ctz: 210,
            i64popcnt: 406,
            i32popcnt: 244,
            i64eqz: 1820,
            i32eqz: 907,
            i32extend8s: 191,
            i32extend16s: 188,
            i64extend8s: 352,
            i64extend16s: 377,
            i64extend32s: 368,
            i64extendsi32: 154,
            i64extendui32: 205,
            i32wrapi64: 192,
            i64eq: 1847,
            i32eq: 1002,
            i64ne: 2249,
            i32ne: 1075,
            i64lts: 1646,
            i32lts: 941,
            i64ltu: 1600,
            i32ltu: 960,
            i64gts: 1718,
            i32gts: 975,
            i64gtu: 1583,
            i32gtu: 916,
            i64les: 1632,
            i32les: 960,
            i64leu: 1719,
            i32leu: 887,
            i64ges: 1912,
            i32ges: 917,
            i64geu: 1842,
            i32geu: 913,
            i64add: 924,
            i32add: 526,
            i64sub: 904,
            i32sub: 416,
            i64mul: 1683,
            i32mul: 839,
            i64divs: 3949,
            i32divs: 2848,
            i64divu: 3537,
            i32divu: 2593,
            i64rems: 18869,
            i32rems: 15274,
            i64remu: 3541,
            i32remu: 2526,
            i64and: 1000,
            i32and: 483,
            i64or: 924,
            i32or: 480,
            i64xor: 969,
            i32xor: 531,
            i64shl: 741,
            i32shl: 231,
            i64shrs: 692,
            i32shrs: 233,
            i64shru: 766,
            i32shru: 312,
            i64rotl: 749,
            i32rotl: 338,
            i64rotr: 724,
            i32rotr: 306,
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
                ref_time: 1565142,
                proof_size: 0,
            },
            free: Weight {
                ref_time: 868476,
                proof_size: 0,
            },
            free_range: Weight {
                ref_time: 919826,
                proof_size: 0,
            },
            free_range_per_page: Weight {
                ref_time: 37915,
                proof_size: 0,
            },
            gr_reserve_gas: Weight {
                ref_time: 2195328,
                proof_size: 0,
            },
            gr_unreserve_gas: Weight {
                ref_time: 2244307,
                proof_size: 0,
            },
            gr_system_reserve_gas: Weight {
                ref_time: 1040553,
                proof_size: 0,
            },
            gr_gas_available: Weight {
                ref_time: 902353,
                proof_size: 0,
            },
            gr_message_id: Weight {
                ref_time: 902698,
                proof_size: 0,
            },
            gr_program_id: Weight {
                ref_time: 900381,
                proof_size: 0,
            },
            gr_source: Weight {
                ref_time: 908094,
                proof_size: 0,
            },
            gr_value: Weight {
                ref_time: 904173,
                proof_size: 0,
            },
            gr_value_available: Weight {
                ref_time: 902487,
                proof_size: 0,
            },
            gr_size: Weight {
                ref_time: 897599,
                proof_size: 0,
            },
            gr_read: Weight {
                ref_time: 1671922,
                proof_size: 0,
            },
            gr_read_per_byte: Weight {
                ref_time: 197,
                proof_size: 0,
            },
            gr_env_vars: Weight {
                ref_time: 1032776,
                proof_size: 0,
            },
            gr_block_height: Weight {
                ref_time: 979678,
                proof_size: 0,
            },
            gr_block_timestamp: Weight {
                ref_time: 893761,
                proof_size: 0,
            },
            gr_random: Weight {
                ref_time: 1850133,
                proof_size: 0,
            },
            gr_reply_deposit: Weight {
                ref_time: 4907182,
                proof_size: 0,
            },
            gr_send: Weight {
                ref_time: 2964123,
                proof_size: 0,
            },
            gr_send_per_byte: Weight {
                ref_time: 492,
                proof_size: 0,
            },
            gr_send_wgas: Weight {
                ref_time: 2997732,
                proof_size: 0,
            },
            gr_send_wgas_per_byte: Weight {
                ref_time: 492,
                proof_size: 0,
            },
            gr_send_init: Weight {
                ref_time: 1086936,
                proof_size: 0,
            },
            gr_send_push: Weight {
                ref_time: 1944435,
                proof_size: 0,
            },
            gr_send_push_per_byte: Weight {
                ref_time: 492,
                proof_size: 0,
            },
            gr_send_commit: Weight {
                ref_time: 2460027,
                proof_size: 0,
            },
            gr_send_commit_wgas: Weight {
                ref_time: 2499815,
                proof_size: 0,
            },
            gr_reservation_send: Weight {
                ref_time: 3420647,
                proof_size: 0,
            },
            gr_reservation_send_per_byte: Weight {
                ref_time: 493,
                proof_size: 0,
            },
            gr_reservation_send_commit: Weight {
                ref_time: 2916856,
                proof_size: 0,
            },
            gr_reply_commit: Weight {
                ref_time: 12018944,
                proof_size: 0,
            },
            gr_reply_commit_wgas: Weight {
                ref_time: 12137604,
                proof_size: 0,
            },
            gr_reservation_reply: Weight {
                ref_time: 8379472,
                proof_size: 0,
            },
            gr_reservation_reply_per_byte: Weight {
                ref_time: 720353,
                proof_size: 0,
            },
            gr_reservation_reply_commit: Weight {
                ref_time: 7809250,
                proof_size: 0,
            },
            gr_reply_push: Weight {
                ref_time: 1701621,
                proof_size: 0,
            },
            gr_reply: Weight {
                ref_time: 13603312,
                proof_size: 0,
            },
            gr_reply_per_byte: Weight {
                ref_time: 711,
                proof_size: 0,
            },
            gr_reply_wgas: Weight {
                ref_time: 11943522,
                proof_size: 0,
            },
            gr_reply_wgas_per_byte: Weight {
                ref_time: 711,
                proof_size: 0,
            },
            gr_reply_push_per_byte: Weight {
                ref_time: 652,
                proof_size: 0,
            },
            gr_reply_to: Weight {
                ref_time: 947649,
                proof_size: 0,
            },
            gr_signal_code: Weight {
                ref_time: 993041,
                proof_size: 0,
            },
            gr_signal_from: Weight {
                ref_time: 951017,
                proof_size: 0,
            },
            gr_reply_input: Weight {
                ref_time: 13351726,
                proof_size: 0,
            },
            gr_reply_input_wgas: Weight {
                ref_time: 10595976,
                proof_size: 0,
            },
            gr_reply_push_input: Weight {
                ref_time: 1147079,
                proof_size: 0,
            },
            gr_reply_push_input_per_byte: Weight {
                ref_time: 144,
                proof_size: 0,
            },
            gr_send_input: Weight {
                ref_time: 2836419,
                proof_size: 0,
            },
            gr_send_input_wgas: Weight {
                ref_time: 2890461,
                proof_size: 0,
            },
            gr_send_push_input: Weight {
                ref_time: 1439174,
                proof_size: 0,
            },
            gr_send_push_input_per_byte: Weight {
                ref_time: 161,
                proof_size: 0,
            },
            gr_debug: Weight {
                ref_time: 1275542,
                proof_size: 0,
            },
            gr_debug_per_byte: Weight {
                ref_time: 438,
                proof_size: 0,
            },
            gr_reply_code: Weight {
                ref_time: 900950,
                proof_size: 0,
            },
            gr_exit: Weight {
                ref_time: 96563242,
                proof_size: 0,
            },
            gr_leave: Weight {
                ref_time: 130303114,
                proof_size: 0,
            },
            gr_wait: Weight {
                ref_time: 112591140,
                proof_size: 0,
            },
            gr_wait_for: Weight {
                ref_time: 92188166,
                proof_size: 0,
            },
            gr_wait_up_to: Weight {
                ref_time: 127918232,
                proof_size: 0,
            },
            gr_wake: Weight {
                ref_time: 3011481,
                proof_size: 0,
            },
            gr_create_program: Weight {
                ref_time: 3690192,
                proof_size: 0,
            },
            gr_create_program_payload_per_byte: Weight {
                ref_time: 116,
                proof_size: 0,
            },
            gr_create_program_salt_per_byte: Weight {
                ref_time: 1403,
                proof_size: 0,
            },
            gr_create_program_wgas: Weight {
                ref_time: 3758182,
                proof_size: 0,
            },
            gr_create_program_wgas_payload_per_byte: Weight {
                ref_time: 114,
                proof_size: 0,
            },
            gr_create_program_wgas_salt_per_byte: Weight {
                ref_time: 1399,
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
    pub mem_grow: Weight,
    pub mem_grow_per_page: Weight,
    pub parachain_read_heuristic: Weight,
}

impl Default for MemoryWeights {
    fn default() -> Self {
        Self {
            lazy_pages_signal_read: Weight {
                ref_time: 28366609,
                proof_size: 0,
            },
            lazy_pages_signal_write: Weight {
                ref_time: 34110270,
                proof_size: 0,
            },
            lazy_pages_signal_write_after_read: Weight {
                ref_time: 9140438,
                proof_size: 0,
            },
            lazy_pages_host_func_read: Weight {
                ref_time: 29943865,
                proof_size: 0,
            },
            lazy_pages_host_func_write: Weight {
                ref_time: 36653194,
                proof_size: 0,
            },
            lazy_pages_host_func_write_after_read: Weight {
                ref_time: 11758882,
                proof_size: 0,
            },
            load_page_data: Weight {
                ref_time: 9171648,
                proof_size: 0,
            },
            upload_page_data: Weight {
                ref_time: 103834672,
                proof_size: 0,
            },
            mem_grow: Weight {
                ref_time: 858927,
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

pub struct InstantiationWeights {
    pub code_section_per_byte: Weight,
    pub data_section_per_byte: Weight,
    pub global_section_per_byte: Weight,
    pub table_section_per_byte: Weight,
    pub element_section_per_byte: Weight,
    pub type_section_per_byte: Weight,
}

impl Default for InstantiationWeights {
    fn default() -> Self {
        Self {
            code_section_per_byte: Weight {
                ref_time: 1990,
                proof_size: 0,
            },
            data_section_per_byte: Weight {
                ref_time: 457,
                proof_size: 0,
            },
            global_section_per_byte: Weight {
                ref_time: 1756,
                proof_size: 0,
            },
            table_section_per_byte: Weight {
                ref_time: 629,
                proof_size: 0,
            },
            element_section_per_byte: Weight {
                ref_time: 2193,
                proof_size: 0,
            },
            type_section_per_byte: Weight {
                ref_time: 15225,
                proof_size: 0,
            },
        }
    }
}

pub struct Weight {
    pub ref_time: u64,
    pub proof_size: u64,
}
