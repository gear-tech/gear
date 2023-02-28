// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
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

//! This module contains the cost schedule and supporting code that constructs a
//! sane default schedule from a `WeightInfo` implementation.

#![allow(unused_parens)]

use crate::{weights::WeightInfo, Config};

use codec::{Decode, Encode};
use frame_support::{weights::Weight, DefaultNoBound};
use gear_core::{code, costs::HostFnWeights as CoreHostFnWeights, message};
use gear_wasm_instrument::{parity_wasm::elements, wasm_instrument::gas_metering};
use pallet_gear_proc_macro::{ScheduleDebug, WeightDebug};
use scale_info::TypeInfo;
#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};
use sp_runtime::RuntimeDebug;
use sp_std::{marker::PhantomData, vec::Vec};

/// How many API calls are executed in a single batch. The reason for increasing the amount
/// of API calls in batches (per benchmark component increase) is so that the linear regression
/// has an easier time determining the contribution of that component.
pub const API_BENCHMARK_BATCH_SIZE: u32 = 80;

/// How many instructions are executed in a single batch. The reasoning is the same
/// as for `API_BENCHMARK_BATCH_SIZE`.
pub const INSTR_BENCHMARK_BATCH_SIZE: u32 = 100;

/// Definition of the cost schedule and other parameterization for the wasm vm.
///
/// Its [`Default`] implementation is the designated way to initialize this type. It uses
/// the benchmarked information supplied by [`Config::WeightInfo`]. All of its fields are
/// public and can therefore be modified. For example in order to change some of the limits
/// and set a custom instruction weight version the following code could be used:
/// ```rust
/// use pallet_gear::{Schedule, Limits, InstructionWeights, Config};
///
/// fn create_schedule<T: Config>() -> Schedule<T> {
///     Schedule {
///         limits: Limits {
///                 globals: 3,
///                 parameters: 3,
///                 memory_pages: 16,
///                 table_size: 3,
///                 br_table_size: 3,
///                 .. Default::default()
///             },
///         instruction_weights: InstructionWeights {
///                 version: 5,
///             .. Default::default()
///         },
///             .. Default::default()
///     }
/// }
/// ```
///
/// # Note
///
/// Please make sure to bump the [`InstructionWeights::version`] whenever substantial
/// changes are made to its values.
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "std", serde(bound(serialize = "", deserialize = "")))]
#[derive(Clone, Encode, Decode, PartialEq, Eq, ScheduleDebug, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct Schedule<T: Config> {
    /// Describes the upper limits on various metrics.
    pub limits: Limits,

    /// The weights for individual wasm instructions.
    pub instruction_weights: InstructionWeights<T>,

    /// The weights for each imported function a program is allowed to call.
    pub host_fn_weights: HostFnWeights<T>,

    /// The weights for memory interaction.
    pub memory_weights: MemoryWeights<T>,

    /// WASM module instantiation per byte cost.
    pub module_instantiation_per_byte: Weight,

    /// Single db write per byte cost.
    pub db_write_per_byte: Weight,

    /// Single db read per byte cost.
    pub db_read_per_byte: Weight,

    /// WASM code instrumentation base cost.
    pub code_instrumentation_cost: Weight,

    /// WASM code instrumentation per-byte cost.
    pub code_instrumentation_byte_cost: Weight,
}

/// Describes the upper limits on various metrics.
///
/// # Note
///
/// The values in this struct should never be decreased. The reason is that decreasing those
/// values will break existing programs which are above the new limits when a
/// re-instrumentation is triggered.
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[derive(Clone, Encode, Decode, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub struct Limits {
    /// Maximum allowed stack height in number of elements.
    ///
    /// See <https://wiki.parity.io/WebAssembly-StackHeight> to find out
    /// how the stack frame cost is calculated. Each element can be of one of the
    /// wasm value types. This means the maximum size per element is 64bit.
    ///
    /// # Note
    ///
    /// It is safe to disable (pass `None`) the `stack_height` when the execution engine
    /// is part of the runtime and hence there can be no indeterminism between different
    /// client resident execution engines.
    pub stack_height: Option<u32>,

    /// Maximum number of globals a module is allowed to declare.
    ///
    /// Globals are not limited through the `stack_height` as locals are. Neither does
    /// the linear memory limit `memory_pages` applies to them.
    pub globals: u32,

    /// Maximum numbers of parameters a function can have.
    ///
    /// Those need to be limited to prevent a potentially exploitable interaction with
    /// the stack height instrumentation: The costs of executing the stack height
    /// instrumentation for an indirectly called function scales linearly with the amount
    /// of parameters of this function. Because the stack height instrumentation itself is
    /// is not weight metered its costs must be static (via this limit) and included in
    /// the costs of the instructions that cause them (call, call_indirect).
    pub parameters: u32,

    /// Maximum number of memory pages allowed for a program.
    pub memory_pages: u16,

    /// Maximum number of elements allowed in a table.
    ///
    /// Currently, the only type of element that is allowed in a table is funcref.
    pub table_size: u32,

    /// Maximum number of elements that can appear as immediate value to the br_table instruction.
    pub br_table_size: u32,

    /// The maximum length of a subject in bytes used for PRNG generation.
    pub subject_len: u32,

    /// The maximum nesting level of the call stack.
    pub call_depth: u32,

    /// The maximum size of a message payload in bytes.
    pub payload_len: u32,

    /// The maximum length of a program code in bytes. This limit applies to the instrumented
    /// version of the code. Therefore `instantiate_with_code` can fail even when supplying
    /// a wasm binary below this maximum size.
    pub code_len: u32,
}

impl Limits {
    /// The maximum memory size in bytes that a program can occupy.
    pub fn max_memory_size(&self) -> u32 {
        self.memory_pages as u32 * 64 * 1024
    }
}

/// Describes the weight for all categories of supported wasm instructions.
///
/// There there is one field for each wasm instruction that describes the weight to
/// execute one instruction of that name. There are a few exceptions:
///
/// 1. If there is a i64 and a i32 variant of an instruction we use the weight
///    of the former for both.
/// 2. The following instructions are free of charge because they merely structure the
///    wasm module and cannot be spammed without making the module invalid (and rejected):
///    End, Unreachable, Return, Else
/// 3. The following instructions cannot be benchmarked because they are removed by any
///    real world execution engine as a preprocessing step and therefore don't yield a
///    meaningful benchmark result. However, in contrast to the instructions mentioned
///    in 2. they can be spammed. We price them with the same weight as the "default"
///    instruction (i64.const): Block, Loop, Nop
/// 4. We price both i64.const and drop as InstructionWeights.i64const / 2. The reason
///    for that is that we cannot benchmark either of them on its own but we need their
///    individual values to derive (by subtraction) the weight of all other instructions
///    that use them as supporting instructions. Supporting means mainly pushing arguments
///    and dropping return values in order to maintain a valid module.
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[derive(Clone, Encode, Decode, PartialEq, Eq, ScheduleDebug, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct InstructionWeights<T: Config> {
    /// Version of the instruction weights.
    ///
    /// # Note
    ///
    /// Should be incremented whenever any instruction weight is changed. The
    /// reason is that changes to instruction weights require a re-instrumentation
    /// in order to apply the changes to an already deployed code. The re-instrumentation
    /// is triggered by comparing the version of the current schedule with the version the code was
    /// instrumented with. Changes usually happen when pallet_gear is re-benchmarked.
    ///
    /// Changes to other parts of the schedule should not increment the version in
    /// order to avoid unnecessary re-instrumentations.
    pub version: u32,
    pub i64const: u32,
    pub i64load: u32,
    pub i64store: u32,
    pub select: u32,
    pub r#if: u32,
    pub br: u32,
    pub br_if: u32,
    pub br_table: u32,
    pub br_table_per_entry: u32,
    pub call: u32,
    pub call_indirect: u32,
    pub call_indirect_per_param: u32,
    // pub call_per_local: u32,
    pub local_get: u32,
    pub local_set: u32,
    pub local_tee: u32,
    pub global_get: u32,
    pub global_set: u32,
    pub memory_current: u32,
    pub i64clz: u32,
    pub i64ctz: u32,
    pub i64popcnt: u32,
    pub i64eqz: u32,
    pub i64extendsi32: u32,
    pub i64extendui32: u32,
    pub i32wrapi64: u32,
    pub i64eq: u32,
    pub i64ne: u32,
    pub i64lts: u32,
    pub i64ltu: u32,
    pub i64gts: u32,
    pub i64gtu: u32,
    pub i64les: u32,
    pub i64leu: u32,
    pub i64ges: u32,
    pub i64geu: u32,
    pub i64add: u32,
    pub i64sub: u32,
    pub i64mul: u32,
    pub i64divs: u32,
    pub i64divu: u32,
    pub i64rems: u32,
    pub i64remu: u32,
    pub i64and: u32,
    pub i64or: u32,
    pub i64xor: u32,
    pub i64shl: u32,
    pub i64shrs: u32,
    pub i64shru: u32,
    pub i64rotl: u32,
    pub i64rotr: u32,
    /// The type parameter is used in the default implementation.
    #[codec(skip)]
    pub _phantom: PhantomData<T>,
}

/// Describes the weight for each imported function that a program is allowed to call.
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[derive(Clone, Encode, Decode, PartialEq, Eq, WeightDebug, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct HostFnWeights<T: Config> {
    /// Weight of calling `alloc`.
    pub alloc: Weight,

    /// Weight of calling `alloc`.
    pub free: Weight,

    /// Weight of calling `gr_reserve_gas`.
    pub gr_reserve_gas: Weight,

    /// Weight of calling `gr_unreserve_gas`
    pub gr_unreserve_gas: Weight,

    /// Weight of calling `gr_system_reserve_gas`
    pub gr_system_reserve_gas: Weight,

    /// Weight of calling `gr_gas_available`.
    pub gr_gas_available: Weight,

    /// Weight of calling `gr_message_id`.
    pub gr_message_id: Weight,

    /// Weight of calling `gr_origin`.
    pub gr_origin: Weight,

    /// Weight of calling `gr_program_id`.
    pub gr_program_id: Weight,

    /// Weight of calling `gr_source`.
    pub gr_source: Weight,

    /// Weight of calling `gr_value`.
    pub gr_value: Weight,

    /// Weight of calling `gr_value_available`.
    pub gr_value_available: Weight,

    /// Weight of calling `gr_size`.
    pub gr_size: Weight,

    /// Weight of calling `gr_read`.
    pub gr_read: Weight,

    /// Weight per payload byte by `gr_read`.
    pub gr_read_per_byte: Weight,

    /// Weight of calling `gr_block_height`.
    pub gr_block_height: Weight,

    /// Weight of calling `gr_block_timestamp`.
    pub gr_block_timestamp: Weight,

    /// Weight of calling `gr_random`.
    pub gr_random: Weight,

    /// Weight of calling `gr_value_available`.
    pub gr_send_init: Weight,

    /// Weight of calling `gr_send_push`.
    pub gr_send_push: Weight,

    /// Weight per payload byte by `gr_send_push`.
    pub gr_send_push_per_byte: Weight,

    /// Weight of calling `gr_send_commit`.
    pub gr_send_commit: Weight,

    /// Weight per payload byte by `gr_send_commit`.
    pub gr_send_commit_per_byte: Weight,

    /// Weight of calling `gr_reservation_send_commit`.
    pub gr_reservation_send_commit: Weight,

    /// Weight per payload byte by `gr_reservation_send_commit`.
    pub gr_reservation_send_commit_per_byte: Weight,

    /// Weight of calling `gr_reply_commit`.
    pub gr_reply_commit: Weight,

    /// Weight of calling `gr_reservation_reply_commit`.
    pub gr_reservation_reply_commit: Weight,

    /// Weight of calling `gr_reply_push`.
    pub gr_reply_push: Weight,

    /// Weight per payload byte by `gr_reply_push`.
    pub gr_reply_push_per_byte: Weight,

    /// Weight of calling `gr_reply_to`.
    pub gr_reply_to: Weight,

    /// Weight of calling `gr_signal_from`.
    pub gr_signal_from: Weight,

    /// Weight of calling `gr_reply_push_input`.
    pub gr_reply_push_input: Weight,

    /// Weight per payload byte by `gr_reply_push_input`.
    pub gr_reply_push_input_per_byte: Weight,

    /// Weight of calling `gr_send_push_input`.
    pub gr_send_push_input: Weight,

    /// Weight per payload byte by `gr_send_push_input`.
    pub gr_send_push_input_per_byte: Weight,

    /// Weight of calling `gr_debug`.
    pub gr_debug: Weight,

    /// Weight per payload byte by `gr_debug_per_byte`.
    pub gr_debug_per_byte: Weight,

    /// Weight of calling `gr_error`.
    pub gr_error: Weight,

    /// Weight of calling `gr_status_code`.
    pub gr_status_code: Weight,

    /// Weight of calling `gr_exit`.
    pub gr_exit: Weight,

    /// Weight of calling `gr_leave`.
    pub gr_leave: Weight,

    /// Weight of calling `gr_wait`.
    pub gr_wait: Weight,

    /// Weight of calling `gr_wait_for`.
    pub gr_wait_for: Weight,

    /// Weight of calling `gr_wait_up_to`.
    pub gr_wait_up_to: Weight,

    /// Weight of calling `gr_wake`.
    pub gr_wake: Weight,

    /// Weight of calling `create_program_wgas`.
    pub gr_create_program_wgas: Weight,

    /// Weight per payload byte by `create_program_wgas`.
    pub gr_create_program_wgas_payload_per_byte: Weight,

    /// Weight per salt byte by `create_program_wgas`.
    pub gr_create_program_wgas_salt_per_byte: Weight,

    /// The type parameter is used in the default implementation.
    #[codec(skip)]
    pub _phantom: PhantomData<T>,
}

/// Describes the weight for memory interaction.
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[derive(Clone, Encode, Decode, PartialEq, Eq, WeightDebug, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct MemoryWeights<T: Config> {
    /// Lazy-pages read access cost per one gear page.
    pub lazy_pages_read: Weight,

    /// Lazy-pages write access cost per one gear page.
    pub lazy_pages_write: Weight,

    /// Lazy-pages write after read access cost per one gear page.
    pub lazy_pages_write_after_read: Weight,

    /// Weight of initial page.
    pub initial_cost: Weight,

    /// Weight of allocated page.
    pub allocation_cost: Weight,

    /// Weight of growing page.
    pub grow_cost: Weight,

    /// Weight of loading page.
    pub load_cost: Weight,

    /// The type parameter is used in the default implementation.
    #[codec(skip)]
    pub _phantom: PhantomData<T>,
}

macro_rules! replace_token {
    ($_in:tt $replacement:tt) => {
        $replacement
    };
}

macro_rules! call_zero {
    ($name:ident, $( $arg:expr ),*) => {
        <T as Config>::WeightInfo::$name($( replace_token!($arg 0) ),*)
    };
}

macro_rules! cost_args {
    ($name:ident, $( $arg: expr ),+) => {
        (<T as Config>::WeightInfo::$name($( $arg ),+).saturating_sub(call_zero!($name, $( $arg ),+))).ref_time()
    }
}

macro_rules! cost_batched_args {
    ($name:ident, $( $arg: expr ),+) => {
        cost_args!($name, $( $arg ),+) / u64::from(API_BENCHMARK_BATCH_SIZE)
    }
}

macro_rules! cost_instr_no_params_with_batch_size {
    ($name:ident, $batch_size:expr) => {
        (cost_args!($name, 1) / u64::from($batch_size)) as u32
    };
}

macro_rules! cost_instr_with_batch_size {
    ($name:ident, $num_params:expr, $batch_size:expr) => {
        cost_instr_no_params_with_batch_size!($name, $batch_size).saturating_sub(
            (cost_instr_no_params_with_batch_size!(instr_i64const, $batch_size) / 2)
                .saturating_mul($num_params),
        )
    };
}

macro_rules! cost_instr {
    ($name:ident, $num_params:expr) => {
        cost_instr_with_batch_size!($name, $num_params, INSTR_BENCHMARK_BATCH_SIZE)
    };
}

macro_rules! cost_instr_batched {
    ($name:ident) => {
        (cost_args!($name, 1) / INSTR_BENCHMARK_BATCH_SIZE as u64) as u32
    };
}

macro_rules! cost_byte_args {
    ($name:ident, $( $arg: expr ),+) => {
        cost_args!($name, $( $arg ),+) / 1024
    }
}

macro_rules! cost_byte_batched_args {
    ($name:ident, $( $arg: expr ),+) => {
        cost_batched_args!($name, $( $arg ),+) / 1024
    }
}

macro_rules! cost {
    ($name:ident) => {
        cost_args!($name, 1)
    };
}

macro_rules! cost_batched {
    ($name:ident) => {
        cost_batched_args!($name, 1)
    };
}

macro_rules! cost_byte {
    ($name:ident) => {
        cost_byte_args!($name, 1)
    };
}

macro_rules! cost_byte_batched {
    ($name:ident) => {
        cost_byte_batched_args!($name, 1)
    };
}

macro_rules! to_weight {
    ($ref_time:expr $(, $proof_size:expr )?) => {
        Weight::from_ref_time($ref_time)$(.set_proof_size($proof_size))?
    };
}

impl<T: Config> Default for Schedule<T> {
    fn default() -> Self {
        Self {
            limits: Default::default(),
            instruction_weights: Default::default(),
            host_fn_weights: Default::default(),
            memory_weights: Default::default(),
            db_write_per_byte: to_weight!(cost_byte!(db_write_per_kb)),
            db_read_per_byte: to_weight!(cost_byte!(db_read_per_kb)),
            module_instantiation_per_byte: to_weight!(cost_byte!(instantiate_module_per_kb)),
            code_instrumentation_cost: call_zero!(reinstrument_per_kb, 0),
            code_instrumentation_byte_cost: to_weight!(cost_byte!(reinstrument_per_kb)),
        }
    }
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            stack_height: None,
            globals: 256,
            parameters: 128,
            memory_pages: code::MAX_WASM_PAGE_COUNT,
            // 4k function pointers (This is in count not bytes).
            table_size: 4096,
            br_table_size: 256,
            subject_len: 32,
            call_depth: 32,
            payload_len: message::MAX_PAYLOAD_SIZE as u32,
            code_len: 512 * 1024,
        }
    }
}

impl<T: Config> Default for InstructionWeights<T> {
    fn default() -> Self {
        let call = cost_instr_batched!(instr_call);
        let call_const = cost_instr_batched!(instr_call_const);
        let i64const = call_const.saturating_sub(call);

        macro_rules! cost {
            // $name.BenchWeight - $num * I64ConstWeight
            ($name:ident, $num:expr) => {
                (cost_instr_batched!($name).saturating_sub((i64const).saturating_mul($num)) as u32)
            };
        }

        Self {
            version: 5,
            i64const,
            i64load: cost!(instr_i64load, 2),
            i64store: cost!(instr_i64store, 2),
            select: cost!(instr_select, 4),
            r#if: cost!(instr_if, 3),
            br: cost!(instr_br, 2),
            br_if: cost!(instr_br_if, 3),
            br_table: cost!(instr_br_table, 3),
            br_table_per_entry: cost!(instr_br_table_per_entry, 0),
            call: cost!(instr_call, 2),
            call_indirect: cost!(instr_call_indirect, 3),
            call_indirect_per_param: cost!(instr_call_indirect_per_param, 1),
            // call_per_local: cost!(instr_call_per_local, 1),
            local_get: cost!(instr_local_get, 1),
            local_set: cost!(instr_local_set, 1),
            local_tee: cost!(instr_local_tee, 2),
            global_get: cost!(instr_global_get, 1),
            global_set: cost!(instr_global_set, 1),
            memory_current: cost!(instr_memory_current, 1),
            i64clz: cost!(instr_i64clz, 2),
            i64ctz: cost!(instr_i64ctz, 2),
            i64popcnt: cost!(instr_i64popcnt, 2),
            i64eqz: cost!(instr_i64eqz, 2),
            i64extendsi32: cost!(instr_i64extendsi32, 2),
            i64extendui32: cost!(instr_i64extendui32, 2),
            i32wrapi64: cost!(instr_i32wrapi64, 2),
            i64eq: cost!(instr_i64eq, 3),
            i64ne: cost!(instr_i64ne, 3),
            i64lts: cost!(instr_i64lts, 3),
            i64ltu: cost!(instr_i64ltu, 3),
            i64gts: cost!(instr_i64gts, 3),
            i64gtu: cost!(instr_i64gtu, 3),
            i64les: cost!(instr_i64les, 3),
            i64leu: cost!(instr_i64leu, 3),
            i64ges: cost!(instr_i64ges, 3),
            i64geu: cost!(instr_i64geu, 3),
            i64add: cost!(instr_i64add, 3),
            i64sub: cost!(instr_i64sub, 3),
            i64mul: cost!(instr_i64mul, 3),
            i64divs: cost!(instr_i64divs, 3),
            i64divu: cost!(instr_i64divu, 3),
            i64rems: cost!(instr_i64rems, 3),
            i64remu: cost!(instr_i64remu, 3),
            i64and: cost!(instr_i64and, 3),
            i64or: cost!(instr_i64or, 3),
            i64xor: cost!(instr_i64xor, 3),
            i64shl: cost!(instr_i64shl, 3),
            i64shrs: cost!(instr_i64shrs, 3),
            i64shru: cost!(instr_i64shru, 3),
            i64rotl: cost!(instr_i64rotl, 3),
            i64rotr: cost!(instr_i64rotr, 3),
            _phantom: PhantomData,
        }
    }
}

impl<T: Config> HostFnWeights<T> {
    pub fn into_core(self) -> CoreHostFnWeights {
        CoreHostFnWeights {
            alloc: self.alloc.ref_time(),
            free: self.free.ref_time(),
            gr_reserve_gas: self.gr_reserve_gas.ref_time(),
            gr_unreserve_gas: self.gr_unreserve_gas.ref_time(),
            gr_system_reserve_gas: self.gr_system_reserve_gas.ref_time(),
            gr_gas_available: self.gr_gas_available.ref_time(),
            gr_message_id: self.gr_message_id.ref_time(),
            gr_origin: self.gr_origin.ref_time(),
            gr_program_id: self.gr_program_id.ref_time(),
            gr_source: self.gr_source.ref_time(),
            gr_value: self.gr_value.ref_time(),
            gr_value_available: self.gr_value_available.ref_time(),
            gr_size: self.gr_size.ref_time(),
            gr_read: self.gr_read.ref_time(),
            gr_read_per_byte: self.gr_read_per_byte.ref_time(),
            gr_block_height: self.gr_block_height.ref_time(),
            gr_block_timestamp: self.gr_block_timestamp.ref_time(),
            gr_random: self.gr_random.ref_time(),
            gr_send_init: self.gr_send_init.ref_time(),
            gr_send_push: self.gr_send_push.ref_time(),
            gr_send_push_per_byte: self.gr_send_push_per_byte.ref_time(),
            gr_send_commit: self.gr_send_commit.ref_time(),
            gr_send_commit_per_byte: self.gr_send_commit_per_byte.ref_time(),
            gr_reservation_send_commit: self.gr_reservation_send_commit.ref_time(),
            gr_reservation_send_commit_per_byte: self
                .gr_reservation_send_commit_per_byte
                .ref_time(),
            gr_reply_commit: self.gr_reply_commit.ref_time(),
            gr_reservation_reply_commit: self.gr_reservation_reply_commit.ref_time(),
            gr_reply_push: self.gr_reply_push.ref_time(),
            gr_reply_push_per_byte: self.gr_reply_push_per_byte.ref_time(),
            gr_debug: self.gr_debug.ref_time(),
            gr_debug_per_byte: self.gr_debug_per_byte.ref_time(),
            gr_error: self.gr_error.ref_time(),
            gr_reply_to: self.gr_reply_to.ref_time(),
            gr_signal_from: self.gr_signal_from.ref_time(),
            gr_status_code: self.gr_status_code.ref_time(),
            gr_exit: self.gr_exit.ref_time(),
            gr_leave: self.gr_leave.ref_time(),
            gr_wait: self.gr_wait.ref_time(),
            gr_wait_for: self.gr_wait_for.ref_time(),
            gr_wait_up_to: self.gr_wait_up_to.ref_time(),
            gr_wake: self.gr_wake.ref_time(),
            gr_create_program_wgas: self.gr_create_program_wgas.ref_time(),
            gr_create_program_wgas_payload_per_byte: self
                .gr_create_program_wgas_payload_per_byte
                .ref_time(),
            gr_create_program_wgas_salt_per_byte: self
                .gr_create_program_wgas_salt_per_byte
                .ref_time(),
            gr_send_push_input: self.gr_send_push_input.ref_time(),
            gr_send_push_input_per_byte: self.gr_send_push_input_per_byte.ref_time(),
            gr_reply_push_input: self.gr_reply_push_input.ref_time(),
            gr_reply_push_input_per_byte: self.gr_reply_push_input_per_byte.ref_time(),
        }
    }
}

impl<T: Config> Default for HostFnWeights<T> {
    fn default() -> Self {
        Self {
            alloc: to_weight!(cost_batched!(alloc)),
            free: to_weight!(cost_batched!(free)),
            gr_reserve_gas: to_weight!(cost_batched!(gr_reserve_gas)),
            gr_system_reserve_gas: to_weight!(cost_batched!(gr_system_reserve_gas)),
            gr_unreserve_gas: to_weight!(cost_batched!(gr_unreserve_gas)),
            gr_gas_available: to_weight!(cost_batched!(gr_gas_available)),
            gr_message_id: to_weight!(cost_batched!(gr_message_id)),
            gr_origin: to_weight!(cost_batched!(gr_origin)),
            gr_program_id: to_weight!(cost_batched!(gr_program_id)),
            gr_source: to_weight!(cost_batched!(gr_source)),
            gr_value: to_weight!(cost_batched!(gr_value)),
            gr_value_available: to_weight!(cost_batched!(gr_value_available)),
            gr_size: to_weight!(cost_batched!(gr_size)),
            gr_read: to_weight!(cost_batched!(gr_read)),
            gr_read_per_byte: to_weight!(cost_byte_batched!(gr_read_per_kb)),
            gr_block_height: to_weight!(cost_batched!(gr_block_height)),
            gr_block_timestamp: to_weight!(cost_batched!(gr_block_timestamp)),
            gr_random: to_weight!(cost_batched!(gr_random)),
            gr_send_init: to_weight!(cost_batched!(gr_send_init)),
            gr_send_push: to_weight!(cost_batched!(gr_send_push)),
            gr_send_push_per_byte: to_weight!(cost_byte_batched!(gr_send_push_per_kb)),
            gr_send_commit: to_weight!(cost_batched!(gr_send_commit)),
            gr_send_commit_per_byte: to_weight!(cost_byte_batched!(gr_send_commit_per_kb)),
            gr_reservation_send_commit: to_weight!(cost_batched!(gr_reservation_send_commit)),
            gr_reservation_send_commit_per_byte: to_weight!(cost_byte_batched!(
                gr_reservation_send_commit_per_kb
            )),
            gr_reply_commit: to_weight!(cost_batched!(gr_reply_commit)),
            gr_reservation_reply_commit: to_weight!(cost_batched!(gr_reservation_reply_commit)),
            gr_reply_push: to_weight!(cost_batched!(gr_reply_push)),
            gr_reply_push_per_byte: to_weight!(cost_byte_batched!(gr_reply_push_per_kb)),
            gr_debug: to_weight!(cost_batched!(gr_debug)),
            gr_debug_per_byte: to_weight!(cost_byte_batched!(gr_debug_per_kb)),
            // TODO: https://github.com/gear-tech/gear/issues/1846
            gr_error: to_weight!(cost_batched!(gr_error)),
            gr_reply_to: to_weight!(cost_batched!(gr_reply_to)),
            gr_signal_from: to_weight!(cost_batched!(gr_signal_from)),
            gr_status_code: to_weight!(cost_batched!(gr_status_code)),
            gr_exit: to_weight!(cost!(gr_exit)),
            gr_leave: to_weight!(cost!(gr_leave)),
            gr_wait: to_weight!(cost!(gr_wait)),
            gr_wait_for: to_weight!(cost!(gr_wait_for)),
            gr_wait_up_to: to_weight!(cost!(gr_wait_up_to)),
            gr_wake: to_weight!(cost_batched!(gr_wake)),
            gr_create_program_wgas: to_weight!(cost_batched!(gr_create_program_wgas)),
            gr_create_program_wgas_payload_per_byte: to_weight!(cost_byte_batched_args!(
                gr_create_program_wgas_per_kb,
                1,
                0
            )),
            gr_create_program_wgas_salt_per_byte: to_weight!(cost_byte_batched_args!(
                gr_create_program_wgas_per_kb,
                0,
                1
            )),
            gr_send_push_input: to_weight!(cost_batched!(gr_send_push_input)),
            gr_send_push_input_per_byte: to_weight!(cost_byte_batched!(gr_send_push_input_per_kb)),
            gr_reply_push_input: to_weight!(cost_batched!(gr_reply_push_input)),
            gr_reply_push_input_per_byte: to_weight!(cost_byte_batched!(
                gr_reply_push_input_per_kb
            )),
            _phantom: PhantomData,
        }
    }
}

impl<T: Config> Default for MemoryWeights<T> {
    fn default() -> Self {
        Self {
            // TODO: set values for lazy-pages from WeightInfo (issue #1893)
            lazy_pages_read: Weight::from_ref_time(100),
            lazy_pages_write: Weight::from_ref_time(100),
            lazy_pages_write_after_read: Weight::from_ref_time(100),
            initial_cost: <T as Config>::WeightInfo::initial_cost(),
            allocation_cost: <T as Config>::WeightInfo::allocation_cost(),
            grow_cost: <T as Config>::WeightInfo::grow_cost(),
            load_cost: <T as Config>::WeightInfo::load_cost(),
            _phantom: PhantomData,
        }
    }
}

struct ScheduleRules<'a, T: Config> {
    schedule: &'a Schedule<T>,
    params: Vec<u32>,
}

impl<T: Config> Schedule<T> {
    pub fn rules(&self, module: &elements::Module) -> impl gas_metering::Rules + '_ {
        ScheduleRules {
            schedule: self,
            params: module
                .type_section()
                .iter()
                .flat_map(|section| section.types())
                .map(|func| {
                    let elements::Type::Function(func) = func;
                    func.params().len() as u32
                })
                .collect(),
        }
    }
}

impl<'a, T: Config> gas_metering::Rules for ScheduleRules<'a, T> {
    fn instruction_cost(&self, instruction: &elements::Instruction) -> Option<u32> {
        use self::elements::Instruction::*;
        let w = &self.schedule.instruction_weights;
        let max_params = self.schedule.limits.parameters;

        let weight = match *instruction {
            End | Unreachable | Return | Else | Block(_) | Loop(_) | Nop | Drop => 0,
            I32Const(_) | I64Const(_) => w.i64const,
            I32Load(_, _)
            | I32Load8S(_, _)
            | I32Load8U(_, _)
            | I32Load16S(_, _)
            | I32Load16U(_, _)
            | I64Load(_, _)
            | I64Load8S(_, _)
            | I64Load8U(_, _)
            | I64Load16S(_, _)
            | I64Load16U(_, _)
            | I64Load32S(_, _)
            | I64Load32U(_, _) => w.i64load,
            I32Store(_, _)
            | I32Store8(_, _)
            | I32Store16(_, _)
            | I64Store(_, _)
            | I64Store8(_, _)
            | I64Store16(_, _)
            | I64Store32(_, _) => w.i64store,
            Select => w.select,
            If(_) => w.r#if,
            Br(_) => w.br,
            BrIf(_) => w.br_if,
            Call(_) => w.call,
            GetLocal(_) => w.local_get,
            SetLocal(_) => w.local_set,
            TeeLocal(_) => w.local_tee,
            GetGlobal(_) => w.global_get,
            SetGlobal(_) => w.global_set,
            CurrentMemory(_) => w.memory_current,
            CallIndirect(idx, _) => *self.params.get(idx as usize).unwrap_or(&max_params),
            BrTable(ref data) => w
                .br_table
                .saturating_add(w.br_table_per_entry.saturating_mul(data.table.len() as u32)),
            I32Clz | I64Clz => w.i64clz,
            I32Ctz | I64Ctz => w.i64ctz,
            I32Popcnt | I64Popcnt => w.i64popcnt,
            I32Eqz | I64Eqz => w.i64eqz,
            I64ExtendSI32 => w.i64extendsi32,
            I64ExtendUI32 => w.i64extendui32,
            I32WrapI64 => w.i32wrapi64,
            I32Eq | I64Eq => w.i64eq,
            I32Ne | I64Ne => w.i64ne,
            I32LtS | I64LtS => w.i64lts,
            I32LtU | I64LtU => w.i64ltu,
            I32GtS | I64GtS => w.i64gts,
            I32GtU | I64GtU => w.i64gtu,
            I32LeS | I64LeS => w.i64les,
            I32LeU | I64LeU => w.i64leu,
            I32GeS | I64GeS => w.i64ges,
            I32GeU | I64GeU => w.i64geu,
            I32Add | I64Add => w.i64add,
            I32Sub | I64Sub => w.i64sub,
            I32Mul | I64Mul => w.i64mul,
            I32DivS | I64DivS => w.i64divs,
            I32DivU | I64DivU => w.i64divu,
            I32RemS | I64RemS => w.i64rems,
            I32RemU | I64RemU => w.i64remu,
            I32And | I64And => w.i64and,
            I32Or | I64Or => w.i64or,
            I32Xor | I64Xor => w.i64xor,
            I32Shl | I64Shl => w.i64shl,
            I32ShrS | I64ShrS => w.i64shrs,
            I32ShrU | I64ShrU => w.i64shru,
            I32Rotl | I64Rotl => w.i64rotl,
            I32Rotr | I64Rotr => w.i64rotr,

            // Returning None makes the gas instrumentation fail which we intend for
            // unsupported or unknown instructions.
            _ => return None,
        };
        Some(weight)
    }

    fn memory_grow_cost(&self) -> gas_metering::MemoryGrowCost {
        gas_metering::MemoryGrowCost::Free
    }

    // fn call_per_local_cost(&self) -> u32 {
    //     self.schedule.instruction_weights.call_per_local
    // }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::mock::Test;
    use gas_metering::Rules;

    fn all_measured_instructions() -> Vec<elements::Instruction> {
        use elements::{BlockType, BrTableData, Instruction::*};
        let default_table_data = BrTableData {
            table: Default::default(),
            default: 0,
        };

        // A set of instructions weights for which the Gear provides.
        // Instruction must not be removed (!), but can be added.
        vec![
            End,
            Unreachable,
            Return,
            Else,
            I32Const(0),
            I64Const(0),
            Block(BlockType::NoResult),
            Loop(BlockType::NoResult),
            Nop,
            Drop,
            I32Load(0, 0),
            I32Load8S(0, 0),
            I32Load8U(0, 0),
            I32Load16S(0, 0),
            I32Load16U(0, 0),
            I64Load(0, 0),
            I64Load8S(0, 0),
            I64Load8U(0, 0),
            I64Load16S(0, 0),
            I64Load16U(0, 0),
            I64Load32S(0, 0),
            I64Load32U(0, 0),
            I32Store(0, 0),
            I32Store8(0, 0),
            I32Store16(0, 0),
            I64Store(0, 0),
            I64Store8(0, 0),
            I64Store16(0, 0),
            I64Store32(0, 0),
            Select,
            If(BlockType::NoResult),
            Br(0),
            BrIf(0),
            Call(0),
            GetLocal(0),
            SetLocal(0),
            TeeLocal(0),
            GetGlobal(0),
            SetGlobal(0),
            CurrentMemory(0),
            CallIndirect(0, 0),
            BrTable(default_table_data.into()),
            I32Clz,
            I64Clz,
            I32Ctz,
            I64Ctz,
            I32Popcnt,
            I64Popcnt,
            I32Eqz,
            I64Eqz,
            I64ExtendSI32,
            I64ExtendUI32,
            I32WrapI64,
            I32Eq,
            I64Eq,
            I32Ne,
            I64Ne,
            I32LtS,
            I64LtS,
            I32LtU,
            I64LtU,
            I32GtS,
            I64GtS,
            I32GtU,
            I64GtU,
            I32LeS,
            I64LeS,
            I32LeU,
            I64LeU,
            I32GeS,
            I64GeS,
            I32GeU,
            I64GeU,
            I32Add,
            I64Add,
            I32Sub,
            I64Sub,
            I32Mul,
            I64Mul,
            I32DivS,
            I64DivS,
            I32DivU,
            I64DivU,
            I32RemS,
            I64RemS,
            I32RemU,
            I64RemU,
            I32And,
            I64And,
            I32Or,
            I64Or,
            I32Xor,
            I64Xor,
            I32Shl,
            I64Shl,
            I32ShrS,
            I64ShrS,
            I32ShrU,
            I64ShrU,
            I32Rotl,
            I64Rotl,
            I32Rotr,
            I64Rotr,
        ]
    }

    fn default_wasm_module() -> elements::Module {
        let simple_wat = r#"
        (module
            (import "env" "memory" (memory 1))
            (export "handle" (func $handle))
            (export "init" (func $init))
            (func $handle)
            (func $init)
        )"#;
        elements::Module::from_bytes(
            wabt::Wat2Wasm::new()
                .validate(false)
                .convert(simple_wat)
                .expect("failed to parse module"),
        )
        .expect("module instantiation failed")
    }

    // This test must never fail during local development/release.
    //
    // The instruction set in the test mustn't be changed. Test checks
    // whether no instruction weight was removed from Rules, so backward
    // compatibility is reached.
    #[test]
    fn instructions_backward_compatibility() {
        let schedule = Schedule::<Test>::default();
        let rules = schedule.rules(&default_wasm_module());
        all_measured_instructions()
            .iter()
            .for_each(|i| assert!(rules.instruction_cost(i).is_some()))
    }
}
