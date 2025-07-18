// This file is part of Gear.
//
// Copyright (C) 2021-2025 Gear Technologies Inc.
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

#![allow(rustdoc::broken_intra_doc_links, missing_docs)]
#![doc = r" This is auto-generated module that contains cost schedule from"]
#![doc = r" `pallets/gear/src/schedule.rs`."]
#![doc = r""]
#![doc = r" See `./scripts/weight-dump.sh` if you want to update it."]

use crate::costs::*;

#[derive(Debug, Clone)]
#[doc = " Definition of the cost schedule and other parameterization for the wasm vm."]
pub struct Schedule {
    #[doc = " Describes the upper limits on various metrics."]
    pub limits: Limits,
    #[doc = " The weights for individual wasm instructions."]
    pub instruction_weights: InstructionWeights,
    #[doc = " The weights for each imported function a program is allowed to call."]
    pub syscall_weights: SyscallWeights,
    #[doc = " The weights for memory interaction."]
    pub memory_weights: MemoryWeights,
    #[doc = " The weights for renting."]
    pub rent_weights: RentWeights,
    #[doc = " The weights for database access."]
    pub db_weights: DbWeights,
    #[doc = " The weights for executing tasks."]
    pub task_weights: TaskWeights,
    #[doc = " The weights for instantiation of the module."]
    pub instantiation_weights: InstantiationWeights,
    #[doc = " WASM code instrumentation base cost."]
    pub code_instrumentation_cost: Weight,
    #[doc = " WASM code instrumentation per-byte cost."]
    pub code_instrumentation_byte_cost: Weight,
    #[doc = " Load allocations weight."]
    pub load_allocations_weight: Weight,
}

impl Default for Schedule {
    fn default() -> Self {
        Self {
            limits: Limits::default(),
            instruction_weights: InstructionWeights::default(),
            syscall_weights: SyscallWeights::default(),
            memory_weights: MemoryWeights::default(),
            rent_weights: RentWeights::default(),
            db_weights: DbWeights::default(),
            task_weights: TaskWeights::default(),
            instantiation_weights: InstantiationWeights::default(),
            code_instrumentation_cost: Weight {
                ref_time: 367783651,
                proof_size: 3760,
            },
            code_instrumentation_byte_cost: Weight {
                ref_time: 699903,
                proof_size: 0,
            },
            load_allocations_weight: Weight {
                ref_time: 23381,
                proof_size: 0,
            },
        }
    }
}

#[derive(Debug, Clone)]
#[doc = " Describes the upper limits on various metrics."]
#[doc = ""]
#[doc = " # Note"]
#[doc = ""]
#[doc = " The values in this struct should never be decreased. The reason is that decreasing those"]
#[doc = " values will break existing programs which are above the new limits when a"]
#[doc = " re-instrumentation is triggered."]
pub struct Limits {
    #[doc = " Maximum allowed stack height in number of elements."]
    #[doc = ""]
    #[doc = " See <https://wiki.parity.io/WebAssembly-StackHeight> to find out"]
    #[doc = " how the stack frame cost is calculated. Each element can be of one of the"]
    #[doc = " wasm value types. This means the maximum size per element is 64bit."]
    #[doc = ""]
    #[doc = " # Note"]
    #[doc = ""]
    #[doc = " It is safe to disable (pass `None`) the `stack_height` when the execution engine"]
    #[doc = " is part of the runtime and hence there can be no indeterminism between different"]
    #[doc = " client resident execution engines."]
    pub stack_height: Option<u32>,
    #[doc = " Maximum number of globals a module is allowed to declare."]
    #[doc = ""]
    #[doc = " Globals are not limited through the linear memory limit `memory_pages`."]
    pub globals: u32,
    #[doc = " Maximum number of locals a function can have."]
    #[doc = ""]
    #[doc = " As wasm engine initializes each of the local, we need to limit their number to confine"]
    #[doc = " execution costs."]
    pub locals: u32,
    #[doc = " Maximum numbers of parameters a function can have."]
    #[doc = ""]
    #[doc = " Those need to be limited to prevent a potentially exploitable interaction with"]
    #[doc = " the stack height instrumentation: The costs of executing the stack height"]
    #[doc = " instrumentation for an indirectly called function scales linearly with the amount"]
    #[doc = " of parameters of this function. Because the stack height instrumentation itself is"]
    #[doc = " is not weight metered its costs must be static (via this limit) and included in"]
    #[doc = " the costs of the instructions that cause them (call, call_indirect)."]
    pub parameters: u32,
    #[doc = " Maximum number of memory pages allowed for a program."]
    pub memory_pages: u16,
    #[doc = " Maximum number of elements allowed in a table."]
    #[doc = ""]
    #[doc = " Currently, the only type of element that is allowed in a table is funcref."]
    pub table_size: u32,
    #[doc = " Maximum number of elements that can appear as immediate value to the br_table instruction."]
    pub br_table_size: u32,
    #[doc = " The maximum length of a subject in bytes used for PRNG generation."]
    pub subject_len: u32,
    #[doc = " The maximum nesting level of the call stack."]
    pub call_depth: u32,
    #[doc = " The maximum size of a message payload in bytes."]
    pub payload_len: u32,
    #[doc = " The maximum length of a program code in bytes. This limit applies to the instrumented"]
    #[doc = " version of the code. Therefore `instantiate_with_code` can fail even when supplying"]
    #[doc = " a wasm binary below this maximum size."]
    pub code_len: u32,
    #[doc = " The maximum number of wasm data segments allowed for a program."]
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
            br_table_size: 256,
            subject_len: 32,
            call_depth: 32,
            payload_len: 8388608,
            code_len: 524288,
            data_segments_amount: 1024,
        }
    }
}

#[derive(Debug, Clone)]
#[doc = " Describes the weight for all categories of supported wasm instructions."]
#[doc = ""]
#[doc = " There there is one field for each wasm instruction that describes the weight to"]
#[doc = " execute one instruction of that name. There are a few exceptions:"]
#[doc = ""]
#[doc = " 1. If there is a i64 and a i32 variant of an instruction we use the weight"]
#[doc = "    of the former for both."]
#[doc = " 2. The following instructions are free of charge because they merely structure the"]
#[doc = "    wasm module and cannot be spammed without making the module invalid (and rejected):"]
#[doc = "    End, Unreachable, Return, Else"]
#[doc = " 3. The following instructions cannot be benchmarked because they are removed by any"]
#[doc = "    real world execution engine as a preprocessing step and therefore don't yield a"]
#[doc = "    meaningful benchmark result. However, in contrast to the instructions mentioned"]
#[doc = "    in 2. they can be spammed. We price them with the same weight as the \"default\""]
#[doc = "    instruction (i64.const): Block, Loop, Nop"]
#[doc = " 4. We price both i64.const and drop as InstructionWeights.i64const / 2. The reason"]
#[doc = "    for that is that we cannot benchmark either of them on its own but we need their"]
#[doc = "    individual values to derive (by subtraction) the weight of all other instructions"]
#[doc = "    that use them as supporting instructions. Supporting means mainly pushing arguments"]
#[doc = "    and dropping return values in order to maintain a valid module."]
pub struct InstructionWeights {
    #[doc = " Version of the instruction weights."]
    #[doc = ""]
    #[doc = " # Note"]
    #[doc = ""]
    #[doc = " Should be incremented whenever any instruction weight is changed. The"]
    #[doc = " reason is that changes to instruction weights require a re-instrumentation"]
    #[doc = " in order to apply the changes to an already deployed code. The re-instrumentation"]
    #[doc = " is triggered by comparing the version of the current schedule with the version the code was"]
    #[doc = " instrumented with. Changes usually happen when pallet_gear is re-benchmarked."]
    #[doc = ""]
    #[doc = " Changes to other parts of the schedule should not increment the version in"]
    #[doc = " order to avoid unnecessary re-instrumentations."]
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
            version: 1700,
            i64const: 199,
            i64load: 5889,
            i32load: 5381,
            i64store: 10843,
            i32store: 10822,
            select: 7110,
            r#if: 5394,
            br: 3844,
            br_if: 6300,
            br_table: 11492,
            br_table_per_entry: 155,
            call: 5424,
            call_indirect: 25121,
            call_indirect_per_param: 1304,
            call_per_local: 0,
            local_get: 670,
            local_set: 1441,
            local_tee: 1436,
            global_get: 731,
            global_set: 1108,
            memory_current: 14495,
            i64clz: 546,
            i32clz: 259,
            i64ctz: 613,
            i32ctz: 242,
            i64popcnt: 496,
            i32popcnt: 375,
            i64eqz: 1793,
            i32eqz: 1030,
            i32extend8s: 309,
            i32extend16s: 238,
            i64extend8s: 385,
            i64extend16s: 457,
            i64extend32s: 354,
            i64extendsi32: 219,
            i64extendui32: 240,
            i32wrapi64: 227,
            i64eq: 1815,
            i32eq: 1170,
            i64ne: 1633,
            i32ne: 1068,
            i64lts: 1698,
            i32lts: 987,
            i64ltu: 1722,
            i32ltu: 970,
            i64gts: 1666,
            i32gts: 969,
            i64gtu: 1665,
            i32gtu: 980,
            i64les: 2039,
            i32les: 1104,
            i64leu: 1632,
            i32leu: 1203,
            i64ges: 1703,
            i32ges: 982,
            i64geu: 1818,
            i32geu: 1003,
            i64add: 954,
            i32add: 653,
            i64sub: 918,
            i32sub: 507,
            i64mul: 1597,
            i32mul: 885,
            i64divs: 3689,
            i32divs: 2571,
            i64divu: 3831,
            i32divu: 2672,
            i64rems: 21313,
            i32rems: 17555,
            i64remu: 3652,
            i32remu: 2665,
            i64and: 937,
            i32and: 509,
            i64or: 1088,
            i32or: 510,
            i64xor: 981,
            i32xor: 662,
            i64shl: 781,
            i32shl: 240,
            i64shrs: 805,
            i32shrs: 245,
            i64shru: 838,
            i32shru: 302,
            i64rotl: 757,
            i32rotl: 259,
            i64rotr: 920,
            i32rotr: 254,
        }
    }
}

#[derive(Debug, Clone)]
#[doc = " Describes the weight for each imported function that a program is allowed to call."]
pub struct SyscallWeights {
    #[doc = " Weight of calling `alloc`."]
    pub alloc: Weight,
    #[doc = " Weight of calling `free`."]
    pub free: Weight,
    #[doc = " Weight of calling `free_range`."]
    pub free_range: Weight,
    #[doc = " Weight of calling `free_range` per page."]
    pub free_range_per_page: Weight,
    #[doc = " Weight of calling `gr_reserve_gas`."]
    pub gr_reserve_gas: Weight,
    #[doc = " Weight of calling `gr_unreserve_gas`"]
    pub gr_unreserve_gas: Weight,
    #[doc = " Weight of calling `gr_system_reserve_gas`"]
    pub gr_system_reserve_gas: Weight,
    #[doc = " Weight of calling `gr_gas_available`."]
    pub gr_gas_available: Weight,
    #[doc = " Weight of calling `gr_message_id`."]
    pub gr_message_id: Weight,
    #[doc = " Weight of calling `gr_program_id`."]
    pub gr_program_id: Weight,
    #[doc = " Weight of calling `gr_source`."]
    pub gr_source: Weight,
    #[doc = " Weight of calling `gr_value`."]
    pub gr_value: Weight,
    #[doc = " Weight of calling `gr_value_available`."]
    pub gr_value_available: Weight,
    #[doc = " Weight of calling `gr_size`."]
    pub gr_size: Weight,
    #[doc = " Weight of calling `gr_read`."]
    pub gr_read: Weight,
    #[doc = " Weight per payload byte by `gr_read`."]
    pub gr_read_per_byte: Weight,
    #[doc = " Weight of calling `gr_env_vars`."]
    pub gr_env_vars: Weight,
    #[doc = " Weight of calling `gr_block_height`."]
    pub gr_block_height: Weight,
    #[doc = " Weight of calling `gr_block_timestamp`."]
    pub gr_block_timestamp: Weight,
    #[doc = " Weight of calling `gr_random`."]
    pub gr_random: Weight,
    #[doc = " Weight of calling `gr_reply_deposit`."]
    pub gr_reply_deposit: Weight,
    #[doc = " Weight of calling `gr_send`."]
    pub gr_send: Weight,
    #[doc = " Weight per payload byte in `gr_send`."]
    pub gr_send_per_byte: Weight,
    #[doc = " Weight of calling `gr_send_wgas`."]
    pub gr_send_wgas: Weight,
    #[doc = " Weight per payload byte in `gr_send_wgas`."]
    pub gr_send_wgas_per_byte: Weight,
    #[doc = " Weight of calling `gr_value_available`."]
    pub gr_send_init: Weight,
    #[doc = " Weight of calling `gr_send_push`."]
    pub gr_send_push: Weight,
    #[doc = " Weight per payload byte by `gr_send_push`."]
    pub gr_send_push_per_byte: Weight,
    #[doc = " Weight of calling `gr_send_commit`."]
    pub gr_send_commit: Weight,
    #[doc = " Weight of calling `gr_send_commit_wgas`."]
    pub gr_send_commit_wgas: Weight,
    #[doc = " Weight of calling `gr_reservation_send`."]
    pub gr_reservation_send: Weight,
    #[doc = " Weight per payload byte in `gr_reservation_send`."]
    pub gr_reservation_send_per_byte: Weight,
    #[doc = " Weight of calling `gr_reservation_send_commit`."]
    pub gr_reservation_send_commit: Weight,
    #[doc = " Weight of calling `gr_reply_commit`."]
    pub gr_reply_commit: Weight,
    #[doc = " Weight of calling `gr_reply_commit_wgas`."]
    pub gr_reply_commit_wgas: Weight,
    #[doc = " Weight of calling `gr_reservation_reply`."]
    pub gr_reservation_reply: Weight,
    #[doc = " Weight of calling `gr_reservation_reply` per one payload byte."]
    pub gr_reservation_reply_per_byte: Weight,
    #[doc = " Weight of calling `gr_reservation_reply_commit`."]
    pub gr_reservation_reply_commit: Weight,
    #[doc = " Weight of calling `gr_reply_push`."]
    pub gr_reply_push: Weight,
    #[doc = " Weight of calling `gr_reply`."]
    pub gr_reply: Weight,
    #[doc = " Weight of calling `gr_reply` per one payload byte."]
    pub gr_reply_per_byte: Weight,
    #[doc = " Weight of calling `gr_reply_wgas`."]
    pub gr_reply_wgas: Weight,
    #[doc = " Weight of calling `gr_reply_wgas` per one payload byte."]
    pub gr_reply_wgas_per_byte: Weight,
    #[doc = " Weight per payload byte by `gr_reply_push`."]
    pub gr_reply_push_per_byte: Weight,
    #[doc = " Weight of calling `gr_reply_to`."]
    pub gr_reply_to: Weight,
    #[doc = " Weight of calling `gr_signal_code`."]
    pub gr_signal_code: Weight,
    #[doc = " Weight of calling `gr_signal_from`."]
    pub gr_signal_from: Weight,
    #[doc = " Weight of calling `gr_reply_input`."]
    pub gr_reply_input: Weight,
    #[doc = " Weight of calling `gr_reply_input_wgas`."]
    pub gr_reply_input_wgas: Weight,
    #[doc = " Weight of calling `gr_reply_push_input`."]
    pub gr_reply_push_input: Weight,
    #[doc = " Weight per payload byte by `gr_reply_push_input`."]
    pub gr_reply_push_input_per_byte: Weight,
    #[doc = " Weight of calling `gr_send_input`."]
    pub gr_send_input: Weight,
    #[doc = " Weight of calling `gr_send_input_wgas`."]
    pub gr_send_input_wgas: Weight,
    #[doc = " Weight of calling `gr_send_push_input`."]
    pub gr_send_push_input: Weight,
    #[doc = " Weight per payload byte by `gr_send_push_input`."]
    pub gr_send_push_input_per_byte: Weight,
    #[doc = " Weight of calling `gr_debug`."]
    pub gr_debug: Weight,
    #[doc = " Weight per payload byte by `gr_debug_per_byte`."]
    pub gr_debug_per_byte: Weight,
    #[doc = " Weight of calling `gr_reply_code`."]
    pub gr_reply_code: Weight,
    #[doc = " Weight of calling `gr_exit`."]
    pub gr_exit: Weight,
    #[doc = " Weight of calling `gr_leave`."]
    pub gr_leave: Weight,
    #[doc = " Weight of calling `gr_wait`."]
    pub gr_wait: Weight,
    #[doc = " Weight of calling `gr_wait_for`."]
    pub gr_wait_for: Weight,
    #[doc = " Weight of calling `gr_wait_up_to`."]
    pub gr_wait_up_to: Weight,
    #[doc = " Weight of calling `gr_wake`."]
    pub gr_wake: Weight,
    #[doc = " Weight of calling `gr_create_program`."]
    pub gr_create_program: Weight,
    #[doc = " Weight per payload byte in `gr_create_program`."]
    pub gr_create_program_payload_per_byte: Weight,
    #[doc = " Weight per salt byte in `gr_create_program`"]
    pub gr_create_program_salt_per_byte: Weight,
    #[doc = " Weight of calling `create_program_wgas`."]
    pub gr_create_program_wgas: Weight,
    #[doc = " Weight per payload byte by `create_program_wgas`."]
    pub gr_create_program_wgas_payload_per_byte: Weight,
    #[doc = " Weight per salt byte by `create_program_wgas`."]
    pub gr_create_program_wgas_salt_per_byte: Weight,
}

impl Default for SyscallWeights {
    fn default() -> Self {
        Self {
            alloc: Weight {
                ref_time: 1886850,
                proof_size: 0,
            },
            free: Weight {
                ref_time: 1034738,
                proof_size: 0,
            },
            free_range: Weight {
                ref_time: 1066622,
                proof_size: 0,
            },
            free_range_per_page: Weight {
                ref_time: 53387,
                proof_size: 0,
            },
            gr_reserve_gas: Weight {
                ref_time: 2599146,
                proof_size: 0,
            },
            gr_unreserve_gas: Weight {
                ref_time: 2593118,
                proof_size: 0,
            },
            gr_system_reserve_gas: Weight {
                ref_time: 1309314,
                proof_size: 0,
            },
            gr_gas_available: Weight {
                ref_time: 1262273,
                proof_size: 0,
            },
            gr_message_id: Weight {
                ref_time: 1256877,
                proof_size: 0,
            },
            gr_program_id: Weight {
                ref_time: 1246556,
                proof_size: 0,
            },
            gr_source: Weight {
                ref_time: 1254831,
                proof_size: 0,
            },
            gr_value: Weight {
                ref_time: 1327999,
                proof_size: 0,
            },
            gr_value_available: Weight {
                ref_time: 1275125,
                proof_size: 0,
            },
            gr_size: Weight {
                ref_time: 1272015,
                proof_size: 0,
            },
            gr_read: Weight {
                ref_time: 1922402,
                proof_size: 0,
            },
            gr_read_per_byte: Weight {
                ref_time: 216,
                proof_size: 0,
            },
            gr_env_vars: Weight {
                ref_time: 1312321,
                proof_size: 0,
            },
            gr_block_height: Weight {
                ref_time: 1256084,
                proof_size: 0,
            },
            gr_block_timestamp: Weight {
                ref_time: 1357303,
                proof_size: 0,
            },
            gr_random: Weight {
                ref_time: 2239792,
                proof_size: 0,
            },
            gr_reply_deposit: Weight {
                ref_time: 5750124,
                proof_size: 0,
            },
            gr_send: Weight {
                ref_time: 3176151,
                proof_size: 0,
            },
            gr_send_per_byte: Weight {
                ref_time: 524,
                proof_size: 0,
            },
            gr_send_wgas: Weight {
                ref_time: 3200852,
                proof_size: 0,
            },
            gr_send_wgas_per_byte: Weight {
                ref_time: 524,
                proof_size: 0,
            },
            gr_send_init: Weight {
                ref_time: 1409873,
                proof_size: 0,
            },
            gr_send_push: Weight {
                ref_time: 2182427,
                proof_size: 0,
            },
            gr_send_push_per_byte: Weight {
                ref_time: 522,
                proof_size: 0,
            },
            gr_send_commit: Weight {
                ref_time: 2663948,
                proof_size: 0,
            },
            gr_send_commit_wgas: Weight {
                ref_time: 2667072,
                proof_size: 0,
            },
            gr_reservation_send: Weight {
                ref_time: 3846443,
                proof_size: 0,
            },
            gr_reservation_send_per_byte: Weight {
                ref_time: 533,
                proof_size: 0,
            },
            gr_reservation_send_commit: Weight {
                ref_time: 3336483,
                proof_size: 0,
            },
            gr_reply_commit: Weight {
                ref_time: 12578360,
                proof_size: 0,
            },
            gr_reply_commit_wgas: Weight {
                ref_time: 13002300,
                proof_size: 0,
            },
            gr_reservation_reply: Weight {
                ref_time: 9802148,
                proof_size: 0,
            },
            gr_reservation_reply_per_byte: Weight {
                ref_time: 761,
                proof_size: 0,
            },
            gr_reservation_reply_commit: Weight {
                ref_time: 9264888,
                proof_size: 0,
            },
            gr_reply_push: Weight {
                ref_time: 2018962,
                proof_size: 0,
            },
            gr_reply: Weight {
                ref_time: 14982002,
                proof_size: 0,
            },
            gr_reply_per_byte: Weight {
                ref_time: 770,
                proof_size: 0,
            },
            gr_reply_wgas: Weight {
                ref_time: 13378528,
                proof_size: 0,
            },
            gr_reply_wgas_per_byte: Weight {
                ref_time: 771,
                proof_size: 0,
            },
            gr_reply_push_per_byte: Weight {
                ref_time: 713,
                proof_size: 0,
            },
            gr_reply_to: Weight {
                ref_time: 1285864,
                proof_size: 0,
            },
            gr_signal_code: Weight {
                ref_time: 1259032,
                proof_size: 0,
            },
            gr_signal_from: Weight {
                ref_time: 1286231,
                proof_size: 0,
            },
            gr_reply_input: Weight {
                ref_time: 13147006,
                proof_size: 0,
            },
            gr_reply_input_wgas: Weight {
                ref_time: 14588786,
                proof_size: 0,
            },
            gr_reply_push_input: Weight {
                ref_time: 1425301,
                proof_size: 0,
            },
            gr_reply_push_input_per_byte: Weight {
                ref_time: 128,
                proof_size: 0,
            },
            gr_send_input: Weight {
                ref_time: 3266792,
                proof_size: 0,
            },
            gr_send_input_wgas: Weight {
                ref_time: 3123676,
                proof_size: 0,
            },
            gr_send_push_input: Weight {
                ref_time: 1788529,
                proof_size: 0,
            },
            gr_send_push_input_per_byte: Weight {
                ref_time: 160,
                proof_size: 0,
            },
            gr_debug: Weight {
                ref_time: 1428928,
                proof_size: 0,
            },
            gr_debug_per_byte: Weight {
                ref_time: 489,
                proof_size: 0,
            },
            gr_reply_code: Weight {
                ref_time: 1248751,
                proof_size: 0,
            },
            gr_exit: Weight {
                ref_time: 21378752,
                proof_size: 0,
            },
            gr_leave: Weight {
                ref_time: 16097686,
                proof_size: 0,
            },
            gr_wait: Weight {
                ref_time: 15756598,
                proof_size: 0,
            },
            gr_wait_for: Weight {
                ref_time: 14917930,
                proof_size: 0,
            },
            gr_wait_up_to: Weight {
                ref_time: 15682964,
                proof_size: 0,
            },
            gr_wake: Weight {
                ref_time: 3307798,
                proof_size: 0,
            },
            gr_create_program: Weight {
                ref_time: 4244731,
                proof_size: 0,
            },
            gr_create_program_payload_per_byte: Weight {
                ref_time: 126,
                proof_size: 0,
            },
            gr_create_program_salt_per_byte: Weight {
                ref_time: 1638,
                proof_size: 0,
            },
            gr_create_program_wgas: Weight {
                ref_time: 4148544,
                proof_size: 0,
            },
            gr_create_program_wgas_payload_per_byte: Weight {
                ref_time: 127,
                proof_size: 0,
            },
            gr_create_program_wgas_salt_per_byte: Weight {
                ref_time: 1638,
                proof_size: 0,
            },
        }
    }
}

#[derive(Debug, Clone)]
#[doc = " Describes the weight for memory interaction."]
#[doc = ""]
#[doc = " Each weight with `lazy_pages_` prefix includes weight for storage read,"]
#[doc = " because for each first page access we need to at least check whether page exists in storage."]
#[doc = " But they do not include cost for loading page data from storage into program memory."]
#[doc = " This weight is taken in account separately, when loading occurs."]
#[doc = ""]
#[doc = " Lazy-pages write accesses does not include cost for uploading page data to storage,"]
#[doc = " because uploading happens after execution, so benchmarks do not include this cost."]
#[doc = " But they include cost for processing changed page data in runtime."]
pub struct MemoryWeights {
    #[doc = " Cost per one [GearPage] signal `read` processing in lazy-pages,"]
    pub lazy_pages_signal_read: Weight,
    #[doc = " Cost per one [GearPage] signal `write` processing in lazy-pages,"]
    pub lazy_pages_signal_write: Weight,
    #[doc = " Cost per one [GearPage] signal `write after read` processing in lazy-pages,"]
    pub lazy_pages_signal_write_after_read: Weight,
    #[doc = " Cost per one [GearPage] host func `read` access processing in lazy-pages,"]
    pub lazy_pages_host_func_read: Weight,
    #[doc = " Cost per one [GearPage] host func `write` access processing in lazy-pages,"]
    pub lazy_pages_host_func_write: Weight,
    #[doc = " Cost per one [GearPage] host func `write after read` access processing in lazy-pages,"]
    pub lazy_pages_host_func_write_after_read: Weight,
    #[doc = " Cost per one [GearPage] data loading from storage and moving it in program memory."]
    #[doc = " Does not include cost for storage read, because it is taken in account separately."]
    pub load_page_data: Weight,
    #[doc = " Cost per one [GearPage] uploading data to storage."]
    #[doc = " Does not include cost for processing changed page data in runtime,"]
    #[doc = " cause it is taken in account separately."]
    pub upload_page_data: Weight,
    #[doc = " Cost per one [WasmPage] for memory growing."]
    pub mem_grow: Weight,
    #[doc = " Cost per one [WasmPage] for memory growing."]
    pub mem_grow_per_page: Weight,
    #[doc = " Cost per one [GearPage]."]
    #[doc = " When we read page data from storage in para-chain, then it should be sent to relay-chain,"]
    #[doc = " in order to use it for process queue execution. So, reading from storage cause"]
    #[doc = " additional resources consumption after block(s) production on para-chain."]
    pub parachain_read_heuristic: Weight,
}

impl Default for MemoryWeights {
    fn default() -> Self {
        Self {
            lazy_pages_signal_read: Weight {
                ref_time: 29141947,
                proof_size: 0,
            },
            lazy_pages_signal_write: Weight {
                ref_time: 35935406,
                proof_size: 0,
            },
            lazy_pages_signal_write_after_read: Weight {
                ref_time: 10645361,
                proof_size: 0,
            },
            lazy_pages_host_func_read: Weight {
                ref_time: 30565091,
                proof_size: 0,
            },
            lazy_pages_host_func_write: Weight {
                ref_time: 37475277,
                proof_size: 0,
            },
            lazy_pages_host_func_write_after_read: Weight {
                ref_time: 12151498,
                proof_size: 0,
            },
            load_page_data: Weight {
                ref_time: 10750145,
                proof_size: 0,
            },
            upload_page_data: Weight {
                ref_time: 103766096,
                proof_size: 0,
            },
            mem_grow: Weight {
                ref_time: 730024,
                proof_size: 0,
            },
            mem_grow_per_page: Weight {
                ref_time: 3,
                proof_size: 0,
            },
            parachain_read_heuristic: Weight {
                ref_time: 0,
                proof_size: 0,
            },
        }
    }
}

#[derive(Debug, Clone)]
#[doc = " Describes the weight for instantiation of the module."]
pub struct InstantiationWeights {
    #[doc = " WASM module code section instantiation per byte cost."]
    pub code_section_per_byte: Weight,
    #[doc = " WASM module data section instantiation per byte cost."]
    pub data_section_per_byte: Weight,
    #[doc = " WASM module global section instantiation per byte cost."]
    pub global_section_per_byte: Weight,
    #[doc = " WASM module table section instantiation per byte cost."]
    pub table_section_per_byte: Weight,
    #[doc = " WASM module element section instantiation per byte cost."]
    pub element_section_per_byte: Weight,
    #[doc = " WASM module type section instantiation per byte cost."]
    pub type_section_per_byte: Weight,
}

impl Default for InstantiationWeights {
    fn default() -> Self {
        Self {
            code_section_per_byte: Weight {
                ref_time: 2940,
                proof_size: 0,
            },
            data_section_per_byte: Weight {
                ref_time: 641,
                proof_size: 0,
            },
            global_section_per_byte: Weight {
                ref_time: 2907,
                proof_size: 0,
            },
            table_section_per_byte: Weight {
                ref_time: 603,
                proof_size: 0,
            },
            element_section_per_byte: Weight {
                ref_time: 2805,
                proof_size: 0,
            },
            type_section_per_byte: Weight {
                ref_time: 1,
                proof_size: 0,
            },
        }
    }
}

#[derive(Debug, Clone)]
#[doc = " Describes the weight for renting."]
pub struct RentWeights {
    #[doc = " Holding message in waitlist weight."]
    pub waitlist: Weight,
    #[doc = " Holding message in dispatch stash weight."]
    pub dispatch_stash: Weight,
    #[doc = " Holding reservation weight."]
    pub reservation: Weight,
    #[doc = " Holding message in mailbox weight."]
    pub mailbox: Weight,
    #[doc = " The minimal gas amount for message to be inserted in mailbox."]
    pub mailbox_threshold: Weight,
}

impl Default for RentWeights {
    fn default() -> Self {
        Self {
            waitlist: Weight {
                ref_time: 100,
                proof_size: 0,
            },
            dispatch_stash: Weight {
                ref_time: 100,
                proof_size: 0,
            },
            reservation: Weight {
                ref_time: 100,
                proof_size: 0,
            },
            mailbox: Weight {
                ref_time: 100,
                proof_size: 0,
            },
            mailbox_threshold: Weight {
                ref_time: 3000,
                proof_size: 0,
            },
        }
    }
}

#[derive(Debug, Clone)]
#[doc = " Describes DB access weights."]
pub struct DbWeights {
    pub read: Weight,
    pub read_per_byte: Weight,
    pub write: Weight,
    pub write_per_byte: Weight,
}

impl Default for DbWeights {
    fn default() -> Self {
        Self {
            read: Weight {
                ref_time: 25000000,
                proof_size: 0,
            },
            read_per_byte: Weight {
                ref_time: 2205,
                proof_size: 0,
            },
            write: Weight {
                ref_time: 100000000,
                proof_size: 0,
            },
            write_per_byte: Weight {
                ref_time: 229,
                proof_size: 0,
            },
        }
    }
}

#[derive(Debug, Clone)]
#[doc = " Describes weights for running tasks."]
pub struct TaskWeights {
    pub remove_gas_reservation: Weight,
    pub send_user_message_to_mailbox: Weight,
    pub send_user_message: Weight,
    pub send_dispatch: Weight,
    pub wake_message: Weight,
    pub wake_message_no_wake: Weight,
    pub remove_from_waitlist: Weight,
    pub remove_from_mailbox: Weight,
}

impl Default for TaskWeights {
    fn default() -> Self {
        Self {
            remove_gas_reservation: Weight {
                ref_time: 954310000,
                proof_size: 6196,
            },
            send_user_message_to_mailbox: Weight {
                ref_time: 709925000,
                proof_size: 4290,
            },
            send_user_message: Weight {
                ref_time: 1470895000,
                proof_size: 6196,
            },
            send_dispatch: Weight {
                ref_time: 815457000,
                proof_size: 4126,
            },
            wake_message: Weight {
                ref_time: 857469000,
                proof_size: 4371,
            },
            wake_message_no_wake: Weight {
                ref_time: 31851000,
                proof_size: 3545,
            },
            remove_from_waitlist: Weight {
                ref_time: 1911646000,
                proof_size: 7598,
            },
            remove_from_mailbox: Weight {
                ref_time: 1864710000,
                proof_size: 7321,
            },
        }
    }
}

#[doc = r" Represents the computational time and storage space required for an operation."]
#[derive(Debug, Clone, Copy)]
pub struct Weight {
    #[doc = r" The weight of computational time used based on some reference hardware."]
    pub ref_time: u64,
    #[doc = r" The weight of storage space used by proof of validity."]
    pub proof_size: u64,
}

impl Weight {
    #[doc = r" Return the reference time part of the weight."]
    #[doc(hidden)]
    pub const fn ref_time(&self) -> u64 {
        self.ref_time
    }
    #[doc = r" Saturating [`Weight`] addition. Computes `self + rhs`, saturating at the numeric bounds of"]
    #[doc = r" all fields instead of overflowing."]
    #[doc(hidden)]
    pub const fn saturating_add(&self, other: Self) -> Self {
        Self {
            ref_time: self.ref_time.saturating_add(other.ref_time),
            proof_size: self.proof_size.saturating_add(other.proof_size),
        }
    }
}

impl From<SyscallWeights> for SyscallCosts {
    fn from(val: SyscallWeights) -> Self {
        Self {
            alloc: val.alloc.ref_time().into(),
            free: val.free.ref_time().into(),
            free_range: val.free_range.ref_time().into(),
            free_range_per_page: val.free_range_per_page.ref_time().into(),
            gr_reserve_gas: val.gr_reserve_gas.ref_time().into(),
            gr_unreserve_gas: val.gr_unreserve_gas.ref_time().into(),
            gr_system_reserve_gas: val.gr_system_reserve_gas.ref_time().into(),
            gr_gas_available: val.gr_gas_available.ref_time().into(),
            gr_message_id: val.gr_message_id.ref_time().into(),
            gr_program_id: val.gr_program_id.ref_time().into(),
            gr_source: val.gr_source.ref_time().into(),
            gr_value: val.gr_value.ref_time().into(),
            gr_value_available: val.gr_value_available.ref_time().into(),
            gr_size: val.gr_size.ref_time().into(),
            gr_read: val.gr_read.ref_time().into(),
            gr_read_per_byte: val.gr_read_per_byte.ref_time().into(),
            gr_env_vars: val.gr_env_vars.ref_time().into(),
            gr_block_height: val.gr_block_height.ref_time().into(),
            gr_block_timestamp: val.gr_block_timestamp.ref_time().into(),
            gr_random: val.gr_random.ref_time().into(),
            gr_reply_deposit: val.gr_reply_deposit.ref_time().into(),
            gr_send: val.gr_send.ref_time().into(),
            gr_send_per_byte: val.gr_send_per_byte.ref_time().into(),
            gr_send_wgas: val.gr_send_wgas.ref_time().into(),
            gr_send_wgas_per_byte: val.gr_send_wgas_per_byte.ref_time().into(),
            gr_send_init: val.gr_send_init.ref_time().into(),
            gr_send_push: val.gr_send_push.ref_time().into(),
            gr_send_push_per_byte: val.gr_send_push_per_byte.ref_time().into(),
            gr_send_commit: val.gr_send_commit.ref_time().into(),
            gr_send_commit_wgas: val.gr_send_commit_wgas.ref_time().into(),
            gr_reservation_send: val.gr_reservation_send.ref_time().into(),
            gr_reservation_send_per_byte: val.gr_reservation_send_per_byte.ref_time().into(),
            gr_reservation_send_commit: val.gr_reservation_send_commit.ref_time().into(),
            gr_send_input: val.gr_send_input.ref_time().into(),
            gr_send_input_wgas: val.gr_send_input_wgas.ref_time().into(),
            gr_send_push_input: val.gr_send_push_input.ref_time().into(),
            gr_send_push_input_per_byte: val.gr_send_push_input_per_byte.ref_time().into(),
            gr_reply: val.gr_reply.ref_time().into(),
            gr_reply_per_byte: val.gr_reply_per_byte.ref_time().into(),
            gr_reply_wgas: val.gr_reply_wgas.ref_time().into(),
            gr_reply_wgas_per_byte: val.gr_reply_wgas_per_byte.ref_time().into(),
            gr_reply_push: val.gr_reply_push.ref_time().into(),
            gr_reply_push_per_byte: val.gr_reply_push_per_byte.ref_time().into(),
            gr_reply_commit: val.gr_reply_commit.ref_time().into(),
            gr_reply_commit_wgas: val.gr_reply_commit_wgas.ref_time().into(),
            gr_reservation_reply: val.gr_reservation_reply.ref_time().into(),
            gr_reservation_reply_per_byte: val.gr_reservation_reply_per_byte.ref_time().into(),
            gr_reservation_reply_commit: val.gr_reservation_reply_commit.ref_time().into(),
            gr_reply_input: val.gr_reply_input.ref_time().into(),
            gr_reply_input_wgas: val.gr_reply_input_wgas.ref_time().into(),
            gr_reply_push_input: val.gr_reply_push_input.ref_time().into(),
            gr_reply_push_input_per_byte: val.gr_reply_push_input_per_byte.ref_time().into(),
            gr_debug: val.gr_debug.ref_time().into(),
            gr_debug_per_byte: val.gr_debug_per_byte.ref_time().into(),
            gr_reply_to: val.gr_reply_to.ref_time().into(),
            gr_signal_code: val.gr_signal_code.ref_time().into(),
            gr_signal_from: val.gr_signal_from.ref_time().into(),
            gr_reply_code: val.gr_reply_code.ref_time().into(),
            gr_exit: val.gr_exit.ref_time().into(),
            gr_leave: val.gr_leave.ref_time().into(),
            gr_wait: val.gr_wait.ref_time().into(),
            gr_wait_for: val.gr_wait_for.ref_time().into(),
            gr_wait_up_to: val.gr_wait_up_to.ref_time().into(),
            gr_wake: val.gr_wake.ref_time().into(),
            gr_create_program: val.gr_create_program.ref_time().into(),
            gr_create_program_payload_per_byte: val
                .gr_create_program_payload_per_byte
                .ref_time()
                .into(),
            gr_create_program_salt_per_byte: val.gr_create_program_salt_per_byte.ref_time().into(),
            gr_create_program_wgas: val.gr_create_program_wgas.ref_time().into(),
            gr_create_program_wgas_payload_per_byte: val
                .gr_create_program_wgas_payload_per_byte
                .ref_time()
                .into(),
            gr_create_program_wgas_salt_per_byte: val
                .gr_create_program_wgas_salt_per_byte
                .ref_time()
                .into(),
        }
    }
}

impl From<MemoryWeights> for IoCosts {
    fn from(val: MemoryWeights) -> Self {
        Self {
            common: PagesCosts::from(val.clone()),
            lazy_pages: LazyPagesCosts::from(val),
        }
    }
}

impl From<MemoryWeights> for PagesCosts {
    fn from(val: MemoryWeights) -> Self {
        Self {
            load_page_data: val.load_page_data.ref_time().into(),
            upload_page_data: val.upload_page_data.ref_time().into(),
            mem_grow: val.mem_grow.ref_time().into(),
            mem_grow_per_page: val.mem_grow_per_page.ref_time().into(),
            parachain_read_heuristic: val.parachain_read_heuristic.ref_time().into(),
        }
    }
}

impl From<MemoryWeights> for LazyPagesCosts {
    fn from(val: MemoryWeights) -> Self {
        Self {
            signal_read: val.lazy_pages_signal_read.ref_time().into(),
            signal_write: val
                .lazy_pages_signal_write
                .saturating_add(val.upload_page_data)
                .ref_time()
                .into(),
            signal_write_after_read: val
                .lazy_pages_signal_write_after_read
                .saturating_add(val.upload_page_data)
                .ref_time()
                .into(),
            host_func_read: val.lazy_pages_host_func_read.ref_time().into(),
            host_func_write: val
                .lazy_pages_host_func_write
                .saturating_add(val.upload_page_data)
                .ref_time()
                .into(),
            host_func_write_after_read: val
                .lazy_pages_host_func_write_after_read
                .saturating_add(val.upload_page_data)
                .ref_time()
                .into(),
            load_page_storage_data: val
                .load_page_data
                .saturating_add(val.parachain_read_heuristic)
                .ref_time()
                .into(),
        }
    }
}

impl From<RentWeights> for RentCosts {
    fn from(val: RentWeights) -> Self {
        Self {
            waitlist: val.waitlist.ref_time().into(),
            dispatch_stash: val.dispatch_stash.ref_time().into(),
            reservation: val.reservation.ref_time().into(),
        }
    }
}

impl From<InstantiationWeights> for InstantiationCosts {
    fn from(val: InstantiationWeights) -> Self {
        Self {
            code_section_per_byte: val.code_section_per_byte.ref_time().into(),
            data_section_per_byte: val.data_section_per_byte.ref_time().into(),
            global_section_per_byte: val.global_section_per_byte.ref_time().into(),
            table_section_per_byte: val.table_section_per_byte.ref_time().into(),
            element_section_per_byte: val.element_section_per_byte.ref_time().into(),
            type_section_per_byte: val.type_section_per_byte.ref_time().into(),
        }
    }
}

impl Schedule {
    pub fn process_costs(&self) -> ProcessCosts {
        ProcessCosts {
            ext: ExtCosts {
                syscalls: self.syscall_weights.clone().into(),
                rent: self.rent_weights.clone().into(),
                mem_grow: self.memory_weights.mem_grow.ref_time().into(),
                mem_grow_per_page: self.memory_weights.mem_grow_per_page.ref_time().into(),
            },
            lazy_pages: self.memory_weights.clone().into(),
            read: self.db_weights.read.ref_time().into(),
            read_per_byte: self.db_weights.read_per_byte.ref_time().into(),
            write: self.db_weights.write.ref_time().into(),
            instrumentation: self.code_instrumentation_cost.ref_time().into(),
            instrumentation_per_byte: self.code_instrumentation_byte_cost.ref_time().into(),
            instantiation_costs: self.instantiation_weights.clone().into(),
            load_allocations_per_interval: self.load_allocations_weight.ref_time().into(),
        }
    }
}
