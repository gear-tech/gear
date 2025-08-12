// This file is part of Gear.

// Copyright (C) 2022-2025 Gear Technologies Inc.
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

use crate::{Config, CostsPerBlockOf, DbWeightOf, weights::WeightInfo};
use common::scheduler::SchedulingCostsPerBlock;
use frame_support::{traits::Get, weights::Weight};
use gear_core::{
    code::MAX_WASM_PAGES_AMOUNT,
    costs::{
        DbCosts, ExtCosts, InstantiationCosts, InstrumentationCosts, IoCosts, LazyPagesCosts,
        PagesCosts, ProcessCosts, RentCosts, SyscallCosts,
    },
    pages::{GearPage, WasmPage},
};
use gear_wasm_instrument::{
    Instruction, Module,
    gas_metering::{MemoryGrowCost, Rules},
};
use pallet_gear_proc_macro::{ScheduleDebug, WeightDebug};
use scale_info::TypeInfo;
#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};
use sp_runtime::{
    RuntimeDebug,
    codec::{Decode, Encode},
};
use sp_std::{marker::PhantomData, vec::Vec};

/// How many API calls are executed in a single batch. The reason for increasing the amount
/// of API calls in batches (per benchmark component increase) is so that the linear regression
/// has an easier time determining the contribution of that component.
pub const API_BENCHMARK_BATCH_SIZE: u32 = 80;

/// How many instructions are executed in a single batch. The reasoning is the same
/// as for `API_BENCHMARK_BATCH_SIZE`.
pub const INSTR_BENCHMARK_BATCH_SIZE: u32 = 500;

/// Constant for `stack_height` is calculated via `calc-stack-height` utility to be small enough
/// to avoid stack overflow in wasmer and wasmi executors.
/// To avoid potential stack overflow problems we have a panic in sandbox in case,
/// execution is ended with stack overflow error. So, process queue execution will be
/// stopped and we will be able to investigate the problem and decrease this constant if needed.
#[cfg(not(fuzz))]
pub const STACK_HEIGHT_LIMIT: u32 = 36_743;

/// For the fuzzer, we take the maximum possible stack limit calculated by the `calc-stack-height`
/// utility, which would be suitable for Linux machines. This has a positive effect on code coverage.
#[cfg(fuzz)]
pub const FUZZER_STACK_HEIGHT_LIMIT: u32 = 65_000;

/// Maximum number of data segments in a wasm module.
/// It has been determined that the maximum number of data segments in a wasm module
/// does not exceed 1024 by a large margin.
pub const DATA_SEGMENTS_AMOUNT_LIMIT: u32 = 1024;

/// The maximum length of a type section in bytes.
pub const TYPE_SECTION_LEN_LIMIT: u32 = 1024 * 20;

/// Maximum number of parameters per type in the type section.
/// 256 parameters per type should be enough for any type section in a wasm module.
pub const TYPE_SECTION_PARAMS_PER_TYPE_LIMIT: u32 = 128;

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
///                 locals: 3,
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
    pub syscall_weights: SyscallWeights<T>,

    /// The weights for memory interaction.
    pub memory_weights: MemoryWeights<T>,

    /// The weights for renting.
    pub rent_weights: RentWeights<T>,

    /// The weights for database access.
    pub db_weights: DbWeights<T>,

    /// The weights for executing tasks.
    pub task_weights: TaskWeights<T>,

    /// The weights for instantiation of the module.
    pub instantiation_weights: InstantiationWeights<T>,

    /// The weights for WASM code instrumentation.
    pub instrumentation_weights: InstrumentationWeights<T>,

    /// Load allocations weight.
    pub load_allocations_weight: Weight,
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
    /// Globals are not limited through the linear memory limit `memory_pages`.
    pub globals: u32,

    /// Maximum number of locals a function can have.
    ///
    /// As wasm engine initializes each of the local, we need to limit their number to confine
    /// execution costs.
    pub locals: u32,

    /// Maximum numbers of parameters a function can have.
    ///
    /// Those need to be limited to prevent a potentially exploitable interaction with
    /// the stack height instrumentation: The costs of executing the stack height
    /// instrumentation for an indirectly called function scales linearly with the amount
    /// of parameters of this function. Because the stack height instrumentation itself is
    /// is not weight metered its costs must be static (via this limit) and included in
    /// the costs of the instructions that cause them (call, call_indirect).
    ///
    /// NOTE: Also the limit checked against type in type section during a code validation.
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

    /// The maximum number of wasm data segments allowed for a program.
    pub data_segments_amount: u32,

    /// The maximum length of a type section in bytes.
    pub type_section_len: u32,
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
    /// The type parameter is used in the default implementation.
    #[codec(skip)]
    #[cfg_attr(feature = "std", serde(skip))]
    pub _phantom: PhantomData<T>,
}

/// Describes the weight for each imported function that a program is allowed to call.
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[derive(Clone, Encode, Decode, PartialEq, Eq, WeightDebug, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct SyscallWeights<T: Config> {
    /// Weight of calling `alloc`.
    pub alloc: Weight,

    /// Weight of calling `free`.
    pub free: Weight,

    /// Weight of calling `free_range`.
    pub free_range: Weight,

    /// Weight of calling `free_range` per page.
    pub free_range_per_page: Weight,

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

    /// Weight of calling `gr_env_vars`.
    pub gr_env_vars: Weight,

    /// Weight of calling `gr_block_height`.
    pub gr_block_height: Weight,

    /// Weight of calling `gr_block_timestamp`.
    pub gr_block_timestamp: Weight,

    /// Weight of calling `gr_random`.
    pub gr_random: Weight,

    /// Weight of calling `gr_reply_deposit`.
    pub gr_reply_deposit: Weight,

    /// Weight of calling `gr_send`.
    pub gr_send: Weight,

    /// Weight per payload byte in `gr_send`.
    pub gr_send_per_byte: Weight,

    /// Weight of calling `gr_send_wgas`.
    pub gr_send_wgas: Weight,

    /// Weight per payload byte in `gr_send_wgas`.
    pub gr_send_wgas_per_byte: Weight,

    /// Weight of calling `gr_value_available`.
    pub gr_send_init: Weight,

    /// Weight of calling `gr_send_push`.
    pub gr_send_push: Weight,

    /// Weight per payload byte by `gr_send_push`.
    pub gr_send_push_per_byte: Weight,

    /// Weight of calling `gr_send_commit`.
    pub gr_send_commit: Weight,

    /// Weight of calling `gr_send_commit_wgas`.
    pub gr_send_commit_wgas: Weight,

    /// Weight of calling `gr_reservation_send`.
    pub gr_reservation_send: Weight,

    /// Weight per payload byte in `gr_reservation_send`.
    pub gr_reservation_send_per_byte: Weight,

    /// Weight of calling `gr_reservation_send_commit`.
    pub gr_reservation_send_commit: Weight,

    /// Weight of calling `gr_reply_commit`.
    pub gr_reply_commit: Weight,

    /// Weight of calling `gr_reply_commit_wgas`.
    pub gr_reply_commit_wgas: Weight,

    /// Weight of calling `gr_reservation_reply`.
    pub gr_reservation_reply: Weight,

    /// Weight of calling `gr_reservation_reply` per one payload byte.
    pub gr_reservation_reply_per_byte: Weight,

    /// Weight of calling `gr_reservation_reply_commit`.
    pub gr_reservation_reply_commit: Weight,

    /// Weight of calling `gr_reply_push`.
    pub gr_reply_push: Weight,

    /// Weight of calling `gr_reply`.
    pub gr_reply: Weight,

    /// Weight of calling `gr_reply` per one payload byte.
    pub gr_reply_per_byte: Weight,

    /// Weight of calling `gr_reply_wgas`.
    pub gr_reply_wgas: Weight,

    /// Weight of calling `gr_reply_wgas` per one payload byte.
    pub gr_reply_wgas_per_byte: Weight,

    /// Weight per payload byte by `gr_reply_push`.
    pub gr_reply_push_per_byte: Weight,

    /// Weight of calling `gr_reply_to`.
    pub gr_reply_to: Weight,

    /// Weight of calling `gr_signal_code`.
    pub gr_signal_code: Weight,

    /// Weight of calling `gr_signal_from`.
    pub gr_signal_from: Weight,

    /// Weight of calling `gr_reply_input`.
    pub gr_reply_input: Weight,

    /// Weight of calling `gr_reply_input_wgas`.
    pub gr_reply_input_wgas: Weight,

    /// Weight of calling `gr_reply_push_input`.
    pub gr_reply_push_input: Weight,

    /// Weight per payload byte by `gr_reply_push_input`.
    pub gr_reply_push_input_per_byte: Weight,

    /// Weight of calling `gr_send_input`.
    pub gr_send_input: Weight,

    /// Weight of calling `gr_send_input_wgas`.
    pub gr_send_input_wgas: Weight,

    /// Weight of calling `gr_send_push_input`.
    pub gr_send_push_input: Weight,

    /// Weight per payload byte by `gr_send_push_input`.
    pub gr_send_push_input_per_byte: Weight,

    /// Weight of calling `gr_debug`.
    pub gr_debug: Weight,

    /// Weight per payload byte by `gr_debug_per_byte`.
    pub gr_debug_per_byte: Weight,

    /// Weight of calling `gr_reply_code`.
    pub gr_reply_code: Weight,

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

    /// Weight of calling `gr_create_program`.
    pub gr_create_program: Weight,

    /// Weight per payload byte in `gr_create_program`.
    pub gr_create_program_payload_per_byte: Weight,

    /// Weight per salt byte in `gr_create_program`
    pub gr_create_program_salt_per_byte: Weight,

    /// Weight of calling `create_program_wgas`.
    pub gr_create_program_wgas: Weight,

    /// Weight per payload byte by `create_program_wgas`.
    pub gr_create_program_wgas_payload_per_byte: Weight,

    /// Weight per salt byte by `create_program_wgas`.
    pub gr_create_program_wgas_salt_per_byte: Weight,

    /// The type parameter is used in the default implementation.
    #[codec(skip)]
    #[cfg_attr(feature = "std", serde(skip))]
    pub _phantom: PhantomData<T>,
}

/// Describes the weight for memory interaction.
///
/// Each weight with `lazy_pages_` prefix includes weight for storage read,
/// because for each first page access we need to at least check whether page exists in storage.
/// But they do not include cost for loading page data from storage into program memory.
/// This weight is taken in account separately, when loading occurs.
///
/// Lazy-pages write accesses does not include cost for uploading page data to storage,
/// because uploading happens after execution, so benchmarks do not include this cost.
/// But they include cost for processing changed page data in runtime.
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[derive(Clone, Encode, Decode, PartialEq, Eq, WeightDebug, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct MemoryWeights<T: Config> {
    /// Cost per one [GearPage] signal `read` processing in lazy-pages,
    pub lazy_pages_signal_read: Weight,

    /// Cost per one [GearPage] signal `write` processing in lazy-pages,
    pub lazy_pages_signal_write: Weight,

    /// Cost per one [GearPage] signal `write after read` processing in lazy-pages,
    pub lazy_pages_signal_write_after_read: Weight,

    /// Cost per one [GearPage] host func `read` access processing in lazy-pages,
    pub lazy_pages_host_func_read: Weight,

    /// Cost per one [GearPage] host func `write` access processing in lazy-pages,
    pub lazy_pages_host_func_write: Weight,

    /// Cost per one [GearPage] host func `write after read` access processing in lazy-pages,
    pub lazy_pages_host_func_write_after_read: Weight,

    /// Cost per one [GearPage] data loading from storage and moving it in program memory.
    /// Does not include cost for storage read, because it is taken in account separately.
    pub load_page_data: Weight,

    /// Cost per one [GearPage] uploading data to storage.
    /// Does not include cost for processing changed page data in runtime,
    /// cause it is taken in account separately.
    pub upload_page_data: Weight,

    /// Cost per one [WasmPage] for memory growing.
    pub mem_grow: Weight,

    /// Cost per one [WasmPage] for memory growing.
    pub mem_grow_per_page: Weight,

    /// Cost per one [GearPage].
    /// When we read page data from storage in para-chain, then it should be sent to relay-chain,
    /// in order to use it for process queue execution. So, reading from storage cause
    /// additional resources consumption after block(s) production on para-chain.
    pub parachain_read_heuristic: Weight,

    /// The type parameter is used in the default implementation.
    #[codec(skip)]
    #[cfg_attr(feature = "std", serde(skip))]
    pub _phantom: PhantomData<T>,
}

/// Describes the weight for instantiation of the module.
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[derive(Clone, Encode, Decode, PartialEq, Eq, ScheduleDebug, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct InstantiationWeights<T: Config> {
    /// WASM module code section instantiation per byte cost.
    pub code_section_per_byte: Weight,

    /// WASM module data section instantiation per byte cost.
    pub data_section_per_byte: Weight,

    /// WASM module global section instantiation per byte cost.
    pub global_section_per_byte: Weight,

    /// WASM module table section instantiation per byte cost.
    pub table_section_per_byte: Weight,

    /// WASM module element section instantiation per byte cost.
    pub element_section_per_byte: Weight,

    /// WASM module type section instantiation per byte cost.
    pub type_section_per_byte: Weight,

    /// The type parameter is used in the default implementation.
    #[codec(skip)]
    #[cfg_attr(feature = "std", serde(skip))]
    pub _phantom: PhantomData<T>,
}

/// Describes the weight for renting.
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[derive(Clone, Encode, Decode, PartialEq, Eq, WeightDebug, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct RentWeights<T: Config> {
    /// Holding message in waitlist weight.
    pub waitlist: Weight,
    /// Holding message in dispatch stash weight.
    pub dispatch_stash: Weight,
    /// Holding reservation weight.
    pub reservation: Weight,
    /// Holding message in mailbox weight.
    pub mailbox: Weight,
    /// The minimal gas amount for message to be inserted in mailbox.
    pub mailbox_threshold: Weight,
    /// The type parameter is used in the default implementation.
    #[codec(skip)]
    #[cfg_attr(feature = "std", serde(skip))]
    pub _phantom: PhantomData<T>,
}

/// Describes DB access weights.
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[derive(Clone, Encode, Decode, PartialEq, Eq, WeightDebug, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct DbWeights<T: Config> {
    pub read: Weight,
    pub read_per_byte: Weight,
    pub write: Weight,
    pub write_per_byte: Weight,
    /// The type parameter is used in the default implementation.
    #[codec(skip)]
    #[cfg_attr(feature = "std", serde(skip))]
    pub _phantom: PhantomData<T>,
}

/// Describes weights for running tasks.
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[derive(Clone, Encode, Decode, PartialEq, Eq, WeightDebug, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct TaskWeights<T: Config> {
    pub remove_gas_reservation: Weight,
    pub send_user_message_to_mailbox: Weight,
    pub send_user_message: Weight,
    pub send_dispatch: Weight,
    pub wake_message: Weight,
    pub wake_message_no_wake: Weight,
    pub remove_from_waitlist: Weight,
    pub remove_from_mailbox: Weight,
    /// The type parameter is used in the default implementation.
    #[codec(skip)]
    #[cfg_attr(feature = "std", serde(skip))]
    pub _phantom: PhantomData<T>,
}
impl<T: Config> Default for TaskWeights<T> {
    fn default() -> Self {
        type W<T> = <T as Config>::WeightInfo;

        Self {
            remove_gas_reservation: W::<T>::tasks_remove_gas_reservation(),
            send_user_message_to_mailbox: W::<T>::tasks_send_user_message_to_mailbox(),
            send_user_message: W::<T>::tasks_send_user_message(),
            send_dispatch: W::<T>::tasks_send_dispatch(),
            wake_message: W::<T>::tasks_wake_message(),
            wake_message_no_wake: W::<T>::tasks_wake_message_no_wake(),
            remove_from_waitlist: W::<T>::tasks_remove_from_waitlist(),
            remove_from_mailbox: W::<T>::tasks_remove_from_mailbox(),
            _phantom: PhantomData,
        }
    }
}

/// Describes WASM code instrumentation weights.
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[derive(Clone, Encode, Decode, PartialEq, Eq, WeightDebug, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct InstrumentationWeights<T: Config> {
    /// WASM code instrumentation base cost.
    pub base: Weight,
    /// WASM code instrumentation per-byte cost.
    pub per_byte: Weight,
    /// The type parameter is used in the default implementation.
    #[codec(skip)]
    #[cfg_attr(feature = "std", serde(skip))]
    pub _phantom: PhantomData<T>,
}

impl<T: Config> Default for InstrumentationWeights<T> {
    fn default() -> Self {
        type W<T> = <T as Config>::WeightInfo;

        Self {
            base: cost_zero(W::<T>::reinstrument_per_kb),
            per_byte: cost_byte(W::<T>::reinstrument_per_kb),
            _phantom: PhantomData,
        }
    }
}

impl<T: Config> From<InstrumentationWeights<T>> for InstrumentationCosts {
    fn from(val: InstrumentationWeights<T>) -> Self {
        Self {
            base: val.base.ref_time().into(),
            per_byte: val.per_byte.ref_time().into(),
        }
    }
}

#[inline]
fn cost(w: fn(u32) -> Weight) -> Weight {
    Weight::from_parts(w(1).saturating_sub(w(0)).ref_time(), 0)
}

#[inline]
fn cost_byte(w: fn(u32) -> Weight) -> Weight {
    Weight::from_parts(cost(w).ref_time() / 1024, 0)
}

#[inline]
fn cost_batched(w: fn(u32) -> Weight) -> Weight {
    Weight::from_parts(cost(w).ref_time() / u64::from(API_BENCHMARK_BATCH_SIZE), 0)
}

#[inline]
fn cost_byte_batched(w: fn(u32) -> Weight) -> Weight {
    Weight::from_parts(cost_batched(w).ref_time() / 1024, 0)
}

#[inline]
fn cost_byte_batched_args(w: fn(u32, u32) -> Weight, arg1: u32, arg2: u32) -> Weight {
    Weight::from_parts(
        w(arg1, arg2).saturating_sub(w(0, 0)).ref_time()
            / u64::from(API_BENCHMARK_BATCH_SIZE)
            / 1024,
        0,
    )
}

#[inline]
fn cost_zero(w: fn(u32) -> Weight) -> Weight {
    let ref_time = w(0).ref_time();
    Weight::from_parts(ref_time, w(0).proof_size())
}

#[inline]
fn cost_instr_no_params_with_batch_size(w: fn(u32) -> Weight) -> u32 {
    ((w(1).saturating_sub(w(0))).ref_time() / u64::from(INSTR_BENCHMARK_BATCH_SIZE)) as u32
}

#[inline]
fn cost_instr<T: Config>(w: fn(u32) -> Weight, num_params: u32) -> u32 {
    cost_instr_no_params_with_batch_size(w)
        .saturating_sub(cost_i64const::<T>().saturating_mul(num_params))
}

#[inline]
fn cost_i64const<T: Config>() -> u32 {
    type W<T> = <T as Config>::WeightInfo;
    // Since we cannot directly benchmark the weight of `i64.const` (or `i32.const`; we consider their weights to be the same for our purposes),
    // we estimate it as the difference between the benchmarks `instr_call_const` and `instr_call`.
    // The difference between these two benchmarks will give us the weight of `i64.const`, which for x86-64
    // can be represented by a single `mov` instruction with the embedded const parameter
    // (for e.x: `mov reg,0xDEADBEAFDEABEAF`).
    //
    // This approach may work, but the estimation is not very accurate.
    // To reduce the impact of this inaccuracy on the assessment of other instructions
    // (as we subtract the weight of `i64.const` from other instructions),
    // we introduce a weight division coefficient called `I64CONST_WEIGHT_DIVIDER`.
    // This helps bring the weight of `i64.const` closer to that of the x86-64 `mov` instruction's estimate.
    const I64CONST_WEIGHT_DIVIDER: u32 = 2;

    cost_instr_no_params_with_batch_size(W::<T>::instr_i64const) / I64CONST_WEIGHT_DIVIDER
}

impl<T: Config> Default for Schedule<T> {
    fn default() -> Self {
        type W<T> = <T as Config>::WeightInfo;
        Self {
            limits: Default::default(),
            instruction_weights: Default::default(),
            syscall_weights: Default::default(),
            memory_weights: Default::default(),
            rent_weights: Default::default(),
            db_weights: Default::default(),
            task_weights: Default::default(),
            instantiation_weights: Default::default(),
            instrumentation_weights: Default::default(),
            load_allocations_weight: cost(W::<T>::load_allocations_per_interval),
        }
    }
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            #[cfg(not(fuzz))]
            stack_height: Some(STACK_HEIGHT_LIMIT),
            #[cfg(fuzz)]
            stack_height: Some(FUZZER_STACK_HEIGHT_LIMIT),
            data_segments_amount: DATA_SEGMENTS_AMOUNT_LIMIT,
            type_section_len: TYPE_SECTION_LEN_LIMIT,
            globals: 256,
            locals: 1024,
            parameters: TYPE_SECTION_PARAMS_PER_TYPE_LIMIT,
            memory_pages: MAX_WASM_PAGES_AMOUNT,
            // 4k function pointers (This is in count not bytes).
            table_size: 4096,
            br_table_size: 256,
            subject_len: 32,
            call_depth: 32,
            payload_len: gear_core::buffer::MAX_PAYLOAD_SIZE as u32,
            code_len: 512 * 1024,
        }
    }
}

impl<T: Config> Default for InstructionWeights<T> {
    fn default() -> Self {
        // # Wasmer's compiler optimization (relevant for version 4.3.5 single-pass compiler for x86-64 target)
        //
        // Wasmer's single-pass compiler implements an optimization for certain wasm i32 instructions where
        // `i64const`/`i32const` parameters can be embedded into native x86-64 instructions.
        //
        // This optimization works for the following types of instructions:
        //
        // - Single-parameter i32 instructions that compile to a single x86-64 `mov` instruction,
        //   e.g., `i64.extend_u/i32`, `i32.wrap_i64`.
        // - Binary operation i32 instructions that compile to an x86-64 `cmp` instruction,
        //   e.g., `i32.eq`, `i32.ne`, `i32.lt_s`, `i32.lt_u`, etc.
        // - `i32.add`, `i32.sub` instructions where one parameter is embedded into the instruction.
        // - Several logical i32 instructions: `i32.and`, `i32.or`, `i32.xor`.
        //
        // See below for the assembly listings of the mentioned instructions.
        type W<T> = <T as Config>::WeightInfo;
        Self {
            version: 1900,
            i64const: cost_i64const::<T>(),
            i64load: cost_instr::<T>(W::<T>::instr_i64load, 0),
            i32load: cost_instr::<T>(W::<T>::instr_i32load, 0),
            i64store: cost_instr::<T>(W::<T>::instr_i64store, 1),
            i32store: cost_instr::<T>(W::<T>::instr_i32store, 0),
            select: cost_instr::<T>(W::<T>::instr_select, 2),
            r#if: cost_instr::<T>(W::<T>::instr_if, 0),
            br: cost_instr::<T>(W::<T>::instr_br, 0),
            br_if: cost_instr::<T>(W::<T>::instr_br_if, 1),
            br_table: cost_instr::<T>(W::<T>::instr_br_table, 0),
            br_table_per_entry: cost_instr::<T>(W::<T>::instr_br_table_per_entry, 0),
            call: cost_instr::<T>(W::<T>::instr_call, 2),
            call_indirect: cost_instr::<T>(W::<T>::instr_call_indirect, 1),
            call_indirect_per_param: cost_instr::<T>(W::<T>::instr_call_indirect_per_param, 1),
            call_per_local: cost_instr::<T>(W::<T>::instr_call_per_local, 1),
            local_get: cost_instr::<T>(W::<T>::instr_local_get, 0),
            local_set: cost_instr::<T>(W::<T>::instr_local_set, 1),
            local_tee: cost_instr::<T>(W::<T>::instr_local_tee, 1),
            global_get: cost_instr::<T>(W::<T>::instr_global_get, 0),
            global_set: cost_instr::<T>(W::<T>::instr_global_set, 1),
            memory_current: cost_instr::<T>(W::<T>::instr_memory_current, 1),
            i64clz: cost_instr::<T>(W::<T>::instr_i64clz, 1),
            i32clz: cost_instr::<T>(W::<T>::instr_i32clz, 1),
            i64ctz: cost_instr::<T>(W::<T>::instr_i64ctz, 1),
            i32ctz: cost_instr::<T>(W::<T>::instr_i32ctz, 1),
            i64popcnt: cost_instr::<T>(W::<T>::instr_i64popcnt, 1),
            i32popcnt: cost_instr::<T>(W::<T>::instr_i32popcnt, 1),
            i64eqz: cost_instr::<T>(W::<T>::instr_i64eqz, 1),
            i32eqz: cost_instr::<T>(W::<T>::instr_i32eqz, 1),
            // `i32extend8s` compiles to:
            // ```assembly
            //     mov rax,0x3b578dc7  <- i64const
            //     movsx esi,al
            // ```
            i32extend8s: cost_instr::<T>(W::<T>::instr_i32extend8s, 1),
            // `i32extend16s` compiles to:
            // ```assembly
            //     mov rax,0xffffffffdd0b1b34  <- i64const
            //     movsx esi,ax
            // ```
            i32extend16s: cost_instr::<T>(W::<T>::instr_i32extend16s, 1),
            i64extend8s: cost_instr::<T>(W::<T>::instr_i64extend8s, 1),
            i64extend16s: cost_instr::<T>(W::<T>::instr_i64extend16s, 1),
            i64extend32s: cost_instr::<T>(W::<T>::instr_i64extend32s, 1),
            // `i64extendsi32` compiles to:
            // ```assembly
            //     mov rax,0xffffffffdd0b1b34  <- i64const
            //     movsxd rsi,eax
            // ```
            i64extendsi32: cost_instr::<T>(W::<T>::instr_i64extendsi32, 1),
            // `i64extendui32` compiles to:
            // ```assembly
            //     mov esi,0x3b578dc7  <- i64const embedded in the instruction
            // ```
            i64extendui32: cost_instr::<T>(W::<T>::instr_i64extendui32, 0),
            // `i32wrapi64` compiles to:
            // ```assembly
            //     mov esi,0x3b578dc7 <- i64const embedded in the instruction
            // ```
            i32wrapi64: cost_instr::<T>(W::<T>::instr_i32wrapi64, 0),
            i64eq: cost_instr::<T>(W::<T>::instr_i64eq, 2),
            // `i32eq` compiles to:
            // ```assembly
            //     mov eax,0x3b578dc7 <- i64const
            //     cmp eax,0xdd0b1b34 <- i64const embedded in the instruction
            //     setz sil
            //     and esi,0xff
            // ```
            i32eq: cost_instr::<T>(W::<T>::instr_i32eq, 1),
            i64ne: cost_instr::<T>(W::<T>::instr_i64ne, 2),
            // `i32ne` compiles to:
            // ```assembly
            //     mov eax,0xa9601ba6 <- i64const
            //     cmp eax,0x4b51bf3  <- i64const embedded in the instruction
            //     setnz sil
            //     and esi,0xff
            // ```
            i32ne: cost_instr::<T>(W::<T>::instr_i32ne, 1),
            i64lts: cost_instr::<T>(W::<T>::instr_i64lts, 2),
            // `i32lts` compiles to:
            // ```assembly
            //     mov eax,0x3b578dc7 <- i64const
            //     cmp eax,0xdd0b1b34 <- i64const embedded in the instruction
            //     setl sil
            //     and esi,0xff
            // ```
            i32lts: cost_instr::<T>(W::<T>::instr_i32lts, 1),
            i64ltu: cost_instr::<T>(W::<T>::instr_i64ltu, 2),
            // `i32ltu` compiles similarly to other i32 comparisons instructions,
            // so we subtract `i64const` (`num_params`) only 1 time.
            i32ltu: cost_instr::<T>(W::<T>::instr_i32ltu, 1),
            i64gts: cost_instr::<T>(W::<T>::instr_i64gts, 2),
            // `i32gts` compiles similarly to other i32 comparisons instructions,
            // so we subtract `i64const` (`num_params`) only 1 time.
            i32gts: cost_instr::<T>(W::<T>::instr_i32gts, 1),
            i64gtu: cost_instr::<T>(W::<T>::instr_i64gtu, 2),
            // `i32gtu` compiles similarly to other i32 comparisons instructions,
            // so we subtract `i64const` (`num_params`) only 1 time.
            i32gtu: cost_instr::<T>(W::<T>::instr_i32gtu, 1),
            i64les: cost_instr::<T>(W::<T>::instr_i64les, 2),
            // `i32les` compiles similarly to other i32 comparisons instructions,
            // so we subtract `i64const` (`num_params`) only 1 time.
            i32les: cost_instr::<T>(W::<T>::instr_i32les, 1),
            i64leu: cost_instr::<T>(W::<T>::instr_i64leu, 2),
            // `i32leu` compiles similarly to other i32 comparisons instructions,
            // so we subtract `i64const` (`num_params`) only 1 time.
            i32leu: cost_instr::<T>(W::<T>::instr_i32leu, 1),
            i64ges: cost_instr::<T>(W::<T>::instr_i64ges, 2),
            // `i32ges` compiles similarly to other i32 comparisons instructions,
            // so we subtract `i64const` (`num_params`) only 1 time.
            i32ges: cost_instr::<T>(W::<T>::instr_i32ges, 1),
            i64geu: cost_instr::<T>(W::<T>::instr_i64geu, 2),
            // `i32geu` compiles similarly to other i32 comparisons instructions,
            // so we subtract `i64const` (`num_params`) only 1 time.
            i32geu: cost_instr::<T>(W::<T>::instr_i32geu, 1),
            i64add: cost_instr::<T>(W::<T>::instr_i64add, 2),
            // `i32add` compiles to:
            // ```assembly
            //     mov eax,0x3b578dc7 <- i64const
            //     add eax,0xdd0b1b34 <- i64const embedded in the instruction
            //     mov esi,eax
            // ```
            i32add: cost_instr::<T>(W::<T>::instr_i32add, 1),
            i64sub: cost_instr::<T>(W::<T>::instr_i64sub, 2),
            // `i32sub` compiles to:
            // ```assembly
            //     mov eax,0x3b578dc7 <- i64const
            //     sub eax,0xdd0b1b34 <- i64const embedded in the instruction
            //     mov esi,eax
            // ```
            i32sub: cost_instr::<T>(W::<T>::instr_i32sub, 1),
            i64mul: cost_instr::<T>(W::<T>::instr_i64mul, 2),
            i32mul: cost_instr::<T>(W::<T>::instr_i32mul, 2),
            i64divs: cost_instr::<T>(W::<T>::instr_i64divs, 2),
            i32divs: cost_instr::<T>(W::<T>::instr_i32divs, 2),
            i64divu: cost_instr::<T>(W::<T>::instr_i64divu, 2),
            i32divu: cost_instr::<T>(W::<T>::instr_i32divu, 2),
            i64rems: cost_instr::<T>(W::<T>::instr_i64rems, 2),
            i32rems: cost_instr::<T>(W::<T>::instr_i32rems, 2),
            i64remu: cost_instr::<T>(W::<T>::instr_i64remu, 2),
            i32remu: cost_instr::<T>(W::<T>::instr_i32remu, 2),
            i64and: cost_instr::<T>(W::<T>::instr_i64and, 2),
            // `i32and` compiles to:
            // ```assembly
            //     mov eax,0x3b578dc7 <- i64const
            //     and eax,0xdd0b1b34 <- i64const embedded in the instruction
            //     mov esi,eax
            // ```
            i32and: cost_instr::<T>(W::<T>::instr_i32and, 1),
            i64or: cost_instr::<T>(W::<T>::instr_i64or, 2),
            // `i32or` compiles to:
            // ```assembly
            //     mov eax,0xf9e1253c <- i64const
            //     or eax,0xeaf224f  <- i64const embedded in the instruction
            //     mov esi,eax
            // ```
            i32or: cost_instr::<T>(W::<T>::instr_i32or, 1),
            i64xor: cost_instr::<T>(W::<T>::instr_i64xor, 2),
            // `i32xor` compiles to:
            // ```assembly
            //     mov eax,0x3b578dc7 <- i64const
            //     xor eax,0xdd0b1b34 <- i64const embedded in the instruction
            //     mov esi,eax
            // ```
            i32xor: cost_instr::<T>(W::<T>::instr_i32xor, 1),
            i64shl: cost_instr::<T>(W::<T>::instr_i64shl, 2),
            i32shl: cost_instr::<T>(W::<T>::instr_i32shl, 2),
            i64shrs: cost_instr::<T>(W::<T>::instr_i64shrs, 2),
            i32shrs: cost_instr::<T>(W::<T>::instr_i32shrs, 2),
            i64shru: cost_instr::<T>(W::<T>::instr_i64shru, 2),
            i32shru: cost_instr::<T>(W::<T>::instr_i32shru, 2),
            i64rotl: cost_instr::<T>(W::<T>::instr_i64rotl, 2),
            i32rotl: cost_instr::<T>(W::<T>::instr_i32rotl, 2),
            i64rotr: cost_instr::<T>(W::<T>::instr_i64rotr, 2),
            i32rotr: cost_instr::<T>(W::<T>::instr_i32rotr, 2),
            _phantom: PhantomData,
        }
    }
}

impl<T: Config> Default for SyscallWeights<T> {
    fn default() -> Self {
        type W<T> = <T as Config>::WeightInfo;
        Self {
            gr_reply_deposit: cost_batched(W::<T>::gr_reply_deposit)
                .saturating_sub(cost_batched(W::<T>::gr_send)),

            gr_send: cost_batched(W::<T>::gr_send),
            gr_send_per_byte: cost_byte_batched(W::<T>::gr_send_per_kb),
            gr_send_wgas: cost_batched(W::<T>::gr_send_wgas),
            gr_send_wgas_per_byte: cost_byte_batched(W::<T>::gr_send_wgas_per_kb),
            gr_send_init: cost_batched(W::<T>::gr_send_init),
            gr_send_push: cost_batched(W::<T>::gr_send_push),
            gr_send_push_per_byte: cost_byte_batched(W::<T>::gr_send_push_per_kb),
            gr_send_commit: cost_batched(W::<T>::gr_send_commit),
            gr_send_commit_wgas: cost_batched(W::<T>::gr_send_commit_wgas),
            gr_reservation_send: cost_batched(W::<T>::gr_reservation_send),
            gr_reservation_send_per_byte: cost_byte_batched(W::<T>::gr_reservation_send_per_kb),
            gr_reservation_send_commit: cost_batched(W::<T>::gr_reservation_send_commit),
            gr_send_input: cost_batched(W::<T>::gr_send_input),
            gr_send_input_wgas: cost_batched(W::<T>::gr_send_input_wgas),
            gr_send_push_input: cost_batched(W::<T>::gr_send_push_input),
            gr_send_push_input_per_byte: cost_byte_batched(W::<T>::gr_send_push_input_per_kb),

            gr_reply: cost(W::<T>::gr_reply),
            gr_reply_per_byte: cost_byte(W::<T>::gr_reply_per_kb),
            gr_reply_wgas: cost(W::<T>::gr_reply_wgas),
            gr_reply_wgas_per_byte: cost_byte(W::<T>::gr_reply_wgas_per_kb),
            gr_reply_push: cost_batched(W::<T>::gr_reply_push),
            gr_reply_push_per_byte: cost_byte(W::<T>::gr_reply_push_per_kb),
            gr_reply_commit: cost(W::<T>::gr_reply_commit),
            gr_reply_commit_wgas: cost(W::<T>::gr_reply_commit_wgas),
            gr_reservation_reply: cost(W::<T>::gr_reservation_reply),
            gr_reservation_reply_per_byte: cost_byte(W::<T>::gr_reservation_reply_per_kb),
            gr_reservation_reply_commit: cost(W::<T>::gr_reservation_reply_commit),
            gr_reply_input: cost(W::<T>::gr_reply_input),
            gr_reply_input_wgas: cost(W::<T>::gr_reply_input_wgas),
            gr_reply_push_input: cost_batched(W::<T>::gr_reply_push_input),
            gr_reply_push_input_per_byte: cost_byte(W::<T>::gr_reply_push_input_per_kb),

            alloc: cost_batched(W::<T>::alloc),
            free: cost_batched(W::<T>::free),
            free_range: cost_batched(W::<T>::free_range),
            free_range_per_page: cost_batched(W::<T>::free_range_per_page),

            gr_reserve_gas: cost(W::<T>::gr_reserve_gas),
            gr_system_reserve_gas: cost_batched(W::<T>::gr_system_reserve_gas),
            gr_unreserve_gas: cost(W::<T>::gr_unreserve_gas),
            gr_gas_available: cost_batched(W::<T>::gr_gas_available),
            gr_message_id: cost_batched(W::<T>::gr_message_id),
            gr_program_id: cost_batched(W::<T>::gr_program_id),
            gr_source: cost_batched(W::<T>::gr_source),
            gr_value: cost_batched(W::<T>::gr_value),
            gr_value_available: cost_batched(W::<T>::gr_value_available),
            gr_size: cost_batched(W::<T>::gr_size),
            gr_read: cost_batched(W::<T>::gr_read),
            gr_read_per_byte: cost_byte_batched(W::<T>::gr_read_per_kb),
            gr_env_vars: cost_batched(W::<T>::gr_env_vars),
            gr_block_height: cost_batched(W::<T>::gr_block_height),
            gr_block_timestamp: cost_batched(W::<T>::gr_block_timestamp),
            gr_random: cost_batched(W::<T>::gr_random),
            gr_debug: cost_batched(W::<T>::gr_debug),
            gr_debug_per_byte: cost_byte_batched(W::<T>::gr_debug_per_kb),
            gr_reply_to: cost_batched(W::<T>::gr_reply_to),
            gr_signal_code: cost_batched(W::<T>::gr_signal_code),
            gr_signal_from: cost_batched(W::<T>::gr_signal_from),
            gr_reply_code: cost_batched(W::<T>::gr_reply_code),
            gr_exit: cost(W::<T>::gr_exit),
            gr_leave: cost(W::<T>::gr_leave),
            gr_wait: cost(W::<T>::gr_wait),
            gr_wait_for: cost(W::<T>::gr_wait_for),
            gr_wait_up_to: cost(W::<T>::gr_wait_up_to),
            gr_wake: cost_batched(W::<T>::gr_wake),

            gr_create_program: cost_batched(W::<T>::gr_create_program),
            gr_create_program_payload_per_byte: cost_byte_batched_args(
                W::<T>::gr_create_program_per_kb,
                1,
                0,
            ),
            gr_create_program_salt_per_byte: cost_byte_batched_args(
                W::<T>::gr_create_program_per_kb,
                0,
                1,
            ),
            gr_create_program_wgas: cost_batched(W::<T>::gr_create_program_wgas),
            gr_create_program_wgas_payload_per_byte: cost_byte_batched_args(
                W::<T>::gr_create_program_wgas_per_kb,
                1,
                0,
            ),
            gr_create_program_wgas_salt_per_byte: cost_byte_batched_args(
                W::<T>::gr_create_program_wgas_per_kb,
                0,
                1,
            ),
            _phantom: PhantomData,
        }
    }
}

impl<T: Config> From<SyscallWeights<T>> for SyscallCosts {
    fn from(val: SyscallWeights<T>) -> Self {
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

impl<T: Config> Default for MemoryWeights<T> {
    fn default() -> Self {
        // In benchmarks we calculate cost per wasm page,
        // so here we must convert it to cost per gear page.
        fn to_cost_per_gear_page(w: fn(u32) -> Weight) -> Weight {
            Weight::from_parts(
                cost(w).ref_time() / (WasmPage::SIZE / GearPage::SIZE) as u64,
                0,
            )
        }

        const KB_SIZE: u64 = 1024;

        // Memory access thru host function benchmark uses a syscall,
        // which accesses memory. So, we have to subtract corresponding syscall weight.
        fn host_func_access(w: fn(u32) -> Weight, syscall: fn(u32) -> Weight) -> Weight {
            let syscall_per_kb_weight = cost_batched(syscall).ref_time();
            let syscall_per_gear_page_weight =
                (syscall_per_kb_weight / KB_SIZE) * GearPage::SIZE as u64;

            let ref_time = to_cost_per_gear_page(w)
                .ref_time()
                .saturating_sub(syscall_per_gear_page_weight);

            Weight::from_parts(ref_time, 0)
        }

        const KB_AMOUNT_IN_ONE_GEAR_PAGE: u64 = GearPage::SIZE as u64 / KB_SIZE;
        const {
            assert!(KB_AMOUNT_IN_ONE_GEAR_PAGE > 0);
            assert!((GearPage::SIZE as u64).is_multiple_of(KB_SIZE));
        }

        type W<T> = <T as Config>::WeightInfo;

        Self {
            lazy_pages_signal_read: to_cost_per_gear_page(W::<T>::lazy_pages_signal_read),
            lazy_pages_signal_write: to_cost_per_gear_page(W::<T>::lazy_pages_signal_write),
            lazy_pages_signal_write_after_read: to_cost_per_gear_page(
                W::<T>::lazy_pages_signal_write_after_read,
            ),
            lazy_pages_host_func_read: host_func_access(
                W::<T>::lazy_pages_host_func_read,
                W::<T>::gr_debug_per_kb,
            ),
            lazy_pages_host_func_write: host_func_access(
                W::<T>::lazy_pages_host_func_write,
                W::<T>::gr_read_per_kb,
            ),
            lazy_pages_host_func_write_after_read: host_func_access(
                W::<T>::lazy_pages_host_func_write_after_read,
                W::<T>::gr_read_per_kb,
            ),
            // As you can see from calculation: `load_page_data` doesn't include weight for db read.
            // This is correct situation, because this weight is already included in above
            // lazy-pages weights.
            load_page_data: to_cost_per_gear_page(W::<T>::lazy_pages_load_page_storage_data)
                .saturating_sub(to_cost_per_gear_page(W::<T>::lazy_pages_signal_read)),
            upload_page_data: cost(W::<T>::db_write_per_kb)
                .saturating_mul(KB_AMOUNT_IN_ONE_GEAR_PAGE)
                .saturating_add(T::DbWeight::get().writes(1)),
            mem_grow: cost_batched(W::<T>::mem_grow),
            mem_grow_per_page: cost_batched(W::<T>::mem_grow_per_page),
            // TODO: make it non-zero for para-chains (issue #2225)
            parachain_read_heuristic: Weight::zero(),
            _phantom: PhantomData,
        }
    }
}

impl<T: Config> From<MemoryWeights<T>> for IoCosts {
    fn from(val: MemoryWeights<T>) -> Self {
        Self {
            common: PagesCosts::from(val.clone()),
            lazy_pages: LazyPagesCosts::from(val),
        }
    }
}

impl<T: Config> From<MemoryWeights<T>> for PagesCosts {
    fn from(val: MemoryWeights<T>) -> Self {
        Self {
            load_page_data: val.load_page_data.ref_time().into(),
            upload_page_data: val.upload_page_data.ref_time().into(),
            mem_grow: val.mem_grow.ref_time().into(),
            mem_grow_per_page: val.mem_grow_per_page.ref_time().into(),
            parachain_read_heuristic: val.parachain_read_heuristic.ref_time().into(),
        }
    }
}

impl<T: Config> From<MemoryWeights<T>> for LazyPagesCosts {
    fn from(val: MemoryWeights<T>) -> Self {
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

impl<T: Config> Default for RentWeights<T> {
    fn default() -> Self {
        Self {
            waitlist: Weight::from_parts(CostsPerBlockOf::<T>::waitlist(), 0),
            dispatch_stash: Weight::from_parts(CostsPerBlockOf::<T>::dispatch_stash(), 0),
            reservation: Weight::from_parts(CostsPerBlockOf::<T>::reservation(), 0),
            mailbox: Weight::from_parts(CostsPerBlockOf::<T>::mailbox(), 0),
            mailbox_threshold: Weight::from_parts(T::MailboxThreshold::get(), 0),
            _phantom: PhantomData,
        }
    }
}

impl<T: Config> From<RentWeights<T>> for RentCosts {
    fn from(val: RentWeights<T>) -> Self {
        Self {
            waitlist: val.waitlist.ref_time().into(),
            dispatch_stash: val.dispatch_stash.ref_time().into(),
            reservation: val.reservation.ref_time().into(),
        }
    }
}

impl<T: Config> Default for DbWeights<T> {
    fn default() -> Self {
        type W<T> = <T as Config>::WeightInfo;
        Self {
            write: DbWeightOf::<T>::get().writes(1),
            read: DbWeightOf::<T>::get().reads(1),
            write_per_byte: cost_byte(W::<T>::db_write_per_kb),
            read_per_byte: cost_byte(W::<T>::db_read_per_kb),
            _phantom: PhantomData,
        }
    }
}

impl<T: Config> From<DbWeights<T>> for DbCosts {
    fn from(val: DbWeights<T>) -> Self {
        Self {
            write: val.write.ref_time().into(),
            read: val.read.ref_time().into(),
            write_per_byte: val.write_per_byte.ref_time().into(),
            read_per_byte: val.read_per_byte.ref_time().into(),
        }
    }
}

impl<T: Config> Default for InstantiationWeights<T> {
    fn default() -> Self {
        type W<T> = <T as Config>::WeightInfo;
        Self {
            code_section_per_byte: cost_byte(W::<T>::instantiate_module_code_section_per_kb),
            data_section_per_byte: cost_byte(W::<T>::instantiate_module_data_section_per_kb),
            global_section_per_byte: cost_byte(W::<T>::instantiate_module_global_section_per_kb),
            table_section_per_byte: cost_byte(W::<T>::instantiate_module_table_section_per_kb),
            element_section_per_byte: cost_byte(W::<T>::instantiate_module_element_section_per_kb),
            type_section_per_byte: cost_byte(W::<T>::instantiate_module_type_section_per_kb),
            _phantom: PhantomData,
        }
    }
}

impl<T: Config> From<InstantiationWeights<T>> for InstantiationCosts {
    fn from(val: InstantiationWeights<T>) -> Self {
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

struct ScheduleRules<'a, T: Config> {
    schedule: &'a Schedule<T>,
    params: Vec<u32>,
}

impl<T: Config> Rules for ScheduleRules<'_, T> {
    fn instruction_cost(&self, instruction: &Instruction) -> Option<u32> {
        use Instruction::*;

        let w = &self.schedule.instruction_weights;
        let max_params = self.schedule.limits.parameters;

        Some(match instruction {
            // Returning None makes the gas instrumentation fail which we intend for
            // unsupported or unknown instructions.
            MemoryGrow { .. } => return None,
            //
            End | Unreachable | Return | Else | Block { .. } | Loop { .. } | Nop | Drop => 0,
            I32Const { .. } | I64Const { .. } => w.i64const,
            I32Load { .. }
            | I32Load8S { .. }
            | I32Load8U { .. }
            | I32Load16S { .. }
            | I32Load16U { .. } => w.i32load,
            I64Load { .. }
            | I64Load8S { .. }
            | I64Load8U { .. }
            | I64Load16S { .. }
            | I64Load16U { .. }
            | I64Load32S { .. }
            | I64Load32U { .. } => w.i64load,
            I32Store { .. } | I32Store8 { .. } | I32Store16 { .. } => w.i32store,
            I64Store { .. } | I64Store8 { .. } | I64Store16 { .. } | I64Store32 { .. } => {
                w.i64store
            }
            Select => w.select,
            If { .. } => w.r#if,
            Br { .. } => w.br,
            BrIf { .. } => w.br_if,
            Call { .. } => w.call,
            LocalGet { .. } => w.local_get,
            LocalSet { .. } => w.local_set,
            LocalTee { .. } => w.local_tee,
            GlobalGet { .. } => w.global_get,
            GlobalSet { .. } => w.global_set,
            MemorySize { .. } => w.memory_current,
            CallIndirect(idx) => {
                let params = self
                    .params
                    .get(*idx as usize)
                    .copied()
                    .unwrap_or(max_params);
                w.call_indirect
                    .saturating_add(w.call_indirect_per_param.saturating_sub(params))
            }
            BrTable(targets) => w
                .br_table
                .saturating_add(w.br_table_per_entry.saturating_mul(targets.len())),
            I32Clz => w.i32clz,
            I64Clz => w.i64clz,
            I32Ctz => w.i32ctz,
            I64Ctz => w.i64ctz,
            I32Popcnt => w.i32popcnt,
            I64Popcnt => w.i64popcnt,
            I32Eqz => w.i32eqz,
            I64Eqz => w.i64eqz,
            // TODO: rename fields
            I64ExtendI32S => w.i64extendsi32,
            I64ExtendI32U => w.i64extendui32,
            I32WrapI64 => w.i32wrapi64,
            I32Eq => w.i32eq,
            I64Eq => w.i64eq,
            I32Ne => w.i32ne,
            I64Ne => w.i64ne,
            I32LtS => w.i32lts,
            I64LtS => w.i64lts,
            I32LtU => w.i32ltu,
            I64LtU => w.i64ltu,
            I32GtS => w.i32gts,
            I64GtS => w.i64gts,
            I32GtU => w.i32gtu,
            I64GtU => w.i64gtu,
            I32LeS => w.i32les,
            I64LeS => w.i64les,
            I32LeU => w.i32leu,
            I64LeU => w.i64leu,
            I32GeS => w.i32ges,
            I64GeS => w.i64ges,
            I32GeU => w.i32geu,
            I64GeU => w.i64geu,
            I32Add => w.i32add,
            I64Add => w.i64add,
            I32Sub => w.i32sub,
            I64Sub => w.i64sub,
            I32Mul => w.i32mul,
            I64Mul => w.i64mul,
            I32DivS => w.i32divs,
            I64DivS => w.i64divs,
            I32DivU => w.i32divu,
            I64DivU => w.i64divu,
            I32RemS => w.i32rems,
            I64RemS => w.i64rems,
            I32RemU => w.i32remu,
            I64RemU => w.i64remu,
            I32And => w.i32and,
            I64And => w.i64and,
            I32Or => w.i32or,
            I64Or => w.i64or,
            I32Xor => w.i32xor,
            I64Xor => w.i64xor,
            I32Shl => w.i32shl,
            I64Shl => w.i64shl,
            I32ShrS => w.i32shrs,
            I64ShrS => w.i64shrs,
            I32ShrU => w.i32shru,
            I64ShrU => w.i64shru,
            I32Rotl => w.i32rotl,
            I64Rotl => w.i64rotl,
            I32Rotr => w.i32rotr,
            I64Rotr => w.i64rotr,
            I32Extend8S => w.i32extend8s,
            I32Extend16S => w.i32extend16s,
            I64Extend8S => w.i64extend8s,
            I64Extend16S => w.i64extend16s,
            I64Extend32S => w.i64extend32s,
        })
    }

    fn memory_grow_cost(&self) -> MemoryGrowCost {
        MemoryGrowCost::Free
    }

    fn call_per_local_cost(&self) -> u32 {
        self.schedule.instruction_weights.call_per_local
    }
}

impl<T: Config> Schedule<T> {
    pub fn rules(&self, module: &Module) -> impl Rules + use<'_, T> {
        ScheduleRules {
            schedule: self,
            params: module
                .type_section
                .as_ref()
                .iter()
                .copied()
                .flatten()
                .map(|func| func.params().len() as u32)
                .collect(),
        }
    }

    pub fn process_costs(&self) -> ProcessCosts {
        ProcessCosts {
            ext: ExtCosts {
                syscalls: self.syscall_weights.clone().into(),
                rent: self.rent_weights.clone().into(),
                mem_grow: self.memory_weights.mem_grow.ref_time().into(),
                mem_grow_per_page: self.memory_weights.mem_grow_per_page.ref_time().into(),
            },
            db: self.db_weights.clone().into(),
            instrumentation: self.instrumentation_weights.clone().into(),
            lazy_pages: self.memory_weights.clone().into(),
            instantiation: self.instantiation_weights.clone().into(),
            load_allocations_per_interval: self.load_allocations_weight.ref_time().into(),
        }
    }
}
