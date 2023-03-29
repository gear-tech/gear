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

//! Functions to procedurally construct program code used for benchmarking.
//!
//! In order to be able to benchmark events that are triggered by program execution,
//! we need to generate programs that perform those events.
//! Because those programs can get very big we cannot simply define
//! them as text (.wat) as this will be too slow and consume too much memory. Therefore
//! we define this simple definition of a program that can be passed to `upload_code` that
//! compiles it down into a `WasmModule` that can be used as a program's code.

use crate::Config;
use common::Origin;
use frame_support::traits::Get;
use gear_core::{
    ids::CodeId,
    memory::{PageU32Size, WasmPage},
};
use gear_wasm_instrument::{
    parity_wasm::{
        builder,
        elements::{
            self, BlockType, CustomSection, FuncBody, Instruction, Instructions, Section, ValueType,
        },
    },
    syscalls::SysCallName,
    STACK_END_EXPORT_NAME,
};
use sp_sandbox::{
    default_executor::{EnvironmentDefinitionBuilder, Memory},
    SandboxEnvironmentBuilder, SandboxMemory,
};
use sp_std::{borrow::ToOwned, convert::TryFrom, marker::PhantomData, prelude::*};

/// The location where to put the generated code.
pub enum Location {
    /// Generate all code into the `init` exported function.
    Init,
    /// Generate all code into the `handle` exported function.
    Handle,
}

/// Pass to `upload_code` in order to create a compiled `WasmModule`.
///
/// This exists to have a more declarative way to describe a wasm module than to use
/// parity-wasm directly. It is tailored to fit the structure of programs that are
/// needed for benchmarking.
#[derive(Default)]
pub struct ModuleDefinition {
    /// Imported memory attached to the module. No memory is imported if `None`.
    pub memory: Option<ImportedMemory>,
    /// Initializers for the imported memory.
    pub data_segments: Vec<DataSegment>,
    /// Creates the supplied amount of i64 mutable globals initialized with random values.
    pub num_globals: u32,
    /// List of syscalls that the module should import. They start with index 0.
    pub imported_functions: Vec<SysCallName>,
    /// Function body of the exported `init` function. Body is empty if `None`.
    /// Its index is `imported_functions.len()`.
    pub init_body: Option<FuncBody>,
    /// Function body of the exported `handle` function. Body is empty if `None`.
    /// Its index is `imported_functions.len() + 1`.
    pub handle_body: Option<FuncBody>,
    /// Function body of the exported `handle_reply` function. Body is empty if `None`.
    /// Its index is `imported_functions.len() + 2`.
    pub reply_body: Option<FuncBody>,
    /// Function body of the exported `handle_signal` function. Body is empty if `None`.
    /// Its index is `imported_functions.len() + 3`.
    pub signal_body: Option<FuncBody>,
    /// Function body of a non-exported function with index `imported_functions.len() + 4`.
    pub aux_body: Option<FuncBody>,
    /// The amount of I64 arguments the aux function should have.
    pub aux_arg_num: u32,
    pub aux_res: Option<ValueType>,
    /// Create a table containing function pointers.
    pub table: Option<TableSegment>,
    /// Create a section named "dummy" of the specified size. This is useful in order to
    /// benchmark the overhead of loading and storing codes of specified sizes. The dummy
    /// section only contributes to the size of the program but does not affect execution.
    pub dummy_section: u32,
    /// Create global export [STACK_END_GLOBAL_NAME] with the given page offset addr.
    /// If None, then all memory supposed to be stack memory, so stack end will be equal to memory size.
    pub stack_end: Option<WasmPage>,
}

pub struct TableSegment {
    /// How many elements should be created inside the table.
    pub num_elements: u32,
    /// The function index with which all table elements should be initialized.
    pub function_index: u32,
}

pub struct DataSegment {
    pub offset: u32,
    pub value: Vec<u8>,
}

#[derive(Clone)]
pub struct ImportedMemory {
    // TODO: change to WasmPage (issue #2094)
    pub min_pages: WasmPage,
}

impl ImportedMemory {
    pub fn max<T: Config>() -> Self
    where
        T: Config,
    {
        Self {
            min_pages: max_pages::<T>().into(),
        }
    }

    pub fn new(min_pages: u16) -> Self {
        Self {
            min_pages: min_pages.into(),
        }
    }
}

pub struct ImportedFunction {
    pub module: &'static str,
    pub name: &'static str,
    pub params: Vec<ValueType>,
    pub return_type: Option<ValueType>,
}

/// A wasm module ready to be put on chain.
#[derive(Clone)]
pub struct WasmModule<T> {
    pub code: Vec<u8>,
    pub hash: CodeId,
    memory: Option<ImportedMemory>,
    _data: PhantomData<T>,
}

pub const OFFSET_INIT: u32 = 0;
pub const OFFSET_HANDLE: u32 = OFFSET_INIT + 1;
pub const OFFSET_REPLY: u32 = OFFSET_HANDLE + 1;
pub const OFFSET_SIGNAL: u32 = OFFSET_REPLY + 1;
pub const OFFSET_AUX: u32 = OFFSET_SIGNAL + 1;

impl<T: Config> From<ModuleDefinition> for WasmModule<T>
where
    T: Config,
    T::AccountId: Origin,
{
    fn from(def: ModuleDefinition) -> Self {
        // internal functions start at that offset.
        let func_offset = u32::try_from(def.imported_functions.len()).unwrap();

        // Every program must export "init" and "handle" functions
        let mut program = builder::module()
            // init function (first internal function)
            .function()
            .signature()
            .build()
            .with_body(def.init_body.unwrap_or_else(body::empty))
            .build()
            // handle function (second internal function)
            .function()
            .signature()
            .build()
            .with_body(def.handle_body.unwrap_or_else(body::empty))
            .build()
            .function()
            .signature()
            .build()
            .with_body(def.reply_body.unwrap_or_else(body::empty))
            .build()
            .function()
            .signature()
            .build()
            .with_body(def.signal_body.unwrap_or_else(body::empty))
            .build()
            .export()
            .field("init")
            .internal()
            .func(func_offset + OFFSET_INIT)
            .build()
            .export()
            .field("handle")
            .internal()
            .func(func_offset + OFFSET_HANDLE)
            .build()
            .export()
            .field("handle_reply")
            .internal()
            .func(func_offset + OFFSET_REPLY)
            .build()
            .export()
            .field("handle_signal")
            .internal()
            .func(func_offset + OFFSET_SIGNAL)
            .build();

        // If specified we add an additional internal function
        if let Some(body) = def.aux_body {
            let mut signature = program.function().signature();
            for _ in 0..def.aux_arg_num {
                signature = signature.with_param(ValueType::I64);
            }
            if let Some(res) = def.aux_res {
                signature = signature.with_result(res);
            }
            program = signature.build().with_body(body).build();
        }

        // Grant access to linear memory.
        if let Some(memory) = &def.memory {
            program = program
                .import()
                .module("env")
                .field("memory")
                .external()
                .memory(memory.min_pages.raw(), None)
                .build();
        }

        // Import supervisor functions. They start with idx 0.
        for name in def.imported_functions {
            let sign = name.signature();
            let sig = builder::signature()
                .with_params(sign.params.into_iter().map(Into::into))
                .with_results(sign.results.into_iter())
                .build_sig();
            let sig = program.push_signature(sig);
            program = program
                .import()
                .module("env")
                .field(name.to_str())
                .with_external(elements::External::Function(sig))
                .build();
        }

        // Initialize memory
        for data in def.data_segments {
            program = program
                .data()
                .offset(Instruction::I32Const(data.offset as i32))
                .value(data.value)
                .build()
        }

        // Add global variables
        if def.num_globals > 0 {
            use rand::{distributions::Standard, prelude::*};
            let rng = rand_pcg::Pcg32::seed_from_u64(3112244599778833558);
            for val in rng.sample_iter(Standard).take(def.num_globals as usize) {
                program = program
                    .global()
                    .value_type()
                    .i64()
                    .mutable()
                    .init_expr(Instruction::I64Const(val))
                    .build()
            }
        }

        // Add stack end export
        let stack_end = def.stack_end.unwrap_or(
            def.memory
                .as_ref()
                .map(|memory| memory.min_pages)
                .unwrap_or(0.into()),
        );
        program = program
            .global()
            .value_type()
            .i32()
            .init_expr(Instruction::I32Const(stack_end.offset() as i32))
            .build()
            .export()
            .field(STACK_END_EXPORT_NAME)
            .internal()
            .global(def.num_globals)
            .build();

        // Add function pointer table
        if let Some(table) = def.table {
            program = program
                .table()
                .with_min(table.num_elements)
                .with_max(Some(table.num_elements))
                .with_element(0, vec![table.function_index; table.num_elements as usize])
                .build();
        }

        // Add the dummy section
        if def.dummy_section > 0 {
            program = program.with_section(Section::Custom(CustomSection::new(
                "dummy".to_owned(),
                vec![42; def.dummy_section as usize],
            )));
        }

        let code = program.build();
        let code = code.into_bytes().unwrap();
        let hash = CodeId::generate(&code);
        Self {
            code: code.to_vec(),
            hash,
            memory: def.memory,
            _data: PhantomData,
        }
    }
}

impl<T: Config> WasmModule<T>
where
    T: Config,
    T::AccountId: Origin,
{
    /// Creates a wasm module with an empty `handle` and `init` function and nothing else.
    pub fn dummy() -> Self {
        ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            ..Default::default()
        }
        .into()
    }

    /// Creates a wasm module of `target_bytes` size. The generated module maximizes
    /// instrumentation runtime by nesting blocks as deeply as possible given the byte budget.
    /// `code_location`: Whether to place the code into `init` or `handle`.
    pub fn sized(target_bytes: u32, code_location: Location) -> Self {
        use self::elements::Instruction::{Drop, End, I32Const, I64Const, I64Eq, If, Return};
        // Base size of a program is 63 bytes and each expansion adds 20 bytes.
        // We do one expansion less to account for the code section and function body
        // size fields inside the binary wasm module representation which are leb128 encoded
        // and therefore grow in size when the contract grows. We are not allowed to overshoot
        // because of the maximum code size that is enforced by `instantiate_with_code`.
        let expansions = (target_bytes.saturating_sub(63) / 20).saturating_sub(1);
        const EXPANSION: &[Instruction] = &[
            I64Const(0),
            I64Const(1),
            I64Eq,
            If(BlockType::NoResult),
            Return,
            End,
            I32Const(0xffffff),
            Drop,
            I32Const(0),
            If(BlockType::NoResult),
            Return,
            End,
        ];
        let mut module = ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            ..Default::default()
        };
        let body = Some(body::repeated(expansions, EXPANSION));
        match code_location {
            Location::Init => module.init_body = body,
            Location::Handle => module.handle_body = body,
        }
        module.into()
    }

    /// Creates a memory instance for use in a sandbox with dimensions declared in this module
    /// and adds it to `env`. A reference to that memory is returned so that it can be used to
    /// access the memory contents from the supervisor.
    pub fn add_memory<S>(&self, env: &mut EnvironmentDefinitionBuilder<S>) -> Option<Memory> {
        let memory = if let Some(memory) = &self.memory {
            memory
        } else {
            return None;
        };
        let memory = Memory::new(memory.min_pages.raw(), None).unwrap();
        env.add_memory("env", "memory", memory.clone());
        Some(memory)
    }

    pub fn unary_instr_64(instr: Instruction, repeat: u32) -> Self {
        Self::unary_instr_for_bit_width(instr, BitWidth::X64, repeat)
    }

    pub fn unary_instr_32(instr: Instruction, repeat: u32) -> Self {
        Self::unary_instr_for_bit_width(instr, BitWidth::X86, repeat)
    }

    fn unary_instr_for_bit_width(instr: Instruction, bit_width: BitWidth, repeat: u32) -> Self {
        use body::DynInstr::Regular;
        ModuleDefinition {
            handle_body: Some(body::repeated_dyn(
                repeat,
                vec![
                    bit_width.random_repeated(1),
                    Regular(instr),
                    Regular(Instruction::Drop),
                ],
            )),
            ..Default::default()
        }
        .into()
    }

    pub fn binary_instr_64(instr: Instruction, repeat: u32) -> Self {
        Self::binary_instr_for_bit_width(instr, BitWidth::X64, repeat)
    }

    pub fn binary_instr_32(instr: Instruction, repeat: u32) -> Self {
        Self::binary_instr_for_bit_width(instr, BitWidth::X86, repeat)
    }

    fn binary_instr_for_bit_width(instr: Instruction, bit_width: BitWidth, repeat: u32) -> Self {
        use body::DynInstr::Regular;
        ModuleDefinition {
            handle_body: Some(body::repeated_dyn(
                repeat,
                vec![
                    bit_width.random_repeated(2),
                    Regular(instr),
                    Regular(Instruction::Drop),
                ],
            )),
            ..Default::default()
        }
        .into()
    }
}

/// Mechanisms to generate a function body that can be used inside a `ModuleDefinition`.
pub mod body {
    use gear_core::memory::{GearPage, PageU32Size, WasmPage};

    use super::*;

    /// When generating contract code by repeating a wasm sequence, it's sometimes necessary
    /// to change those instructions on each repetition. The variants of this enum describe
    /// various ways in which this can happen.
    #[derive(Debug, Clone)]
    pub enum DynInstr {
        /// Insert `i32.const (self.0 as i32)` operation
        InstrI32Const(u32),
        /// Insert `i64.const (self.0 as i64)` operation
        InstrI64Const(u64),
        /// Insert `call self.0` operation
        InstrCall(u32),
        /// Insert `i32.load align=self.0, offset=self.1` operation
        InstrI32Load(u32, u32),
        /// Insert the associated instruction.
        Regular(Instruction),
        /// Insert a I32Const with incrementing value for each insertion.
        /// (start_at, increment_by)
        Counter(u32, u32),
        /// Insert a I32Const with a random value in [low, high) not divisible by two.
        /// (low, high)
        RandomUnaligned(u32, u32),
        /// Insert a I32Const with a random value in [low, high).
        /// (low, high)
        RandomI32(i32, i32),
        /// Insert a I64Const with a random value in [low, high).
        /// (low, high)
        RandomI64(i64, i64),
        /// Insert the specified amount of I32Const with a random value.
        RandomI32Repeated(usize),
        /// Insert the specified amount of I64Const with a random value.
        RandomI64Repeated(usize),
        /// Insert a GetLocal with a random offset in [low, high).
        /// (low, high)
        RandomGetLocal(u32, u32),
        /// Insert a SetLocal with a random offset in [low, high).
        /// (low, high)
        RandomSetLocal(u32, u32),
        /// Insert a TeeLocal with a random offset in [low, high).
        /// (low, high)
        RandomTeeLocal(u32, u32),
        /// Insert a GetGlobal with a random offset in [low, high).
        /// (low, high)
        RandomGetGlobal(u32, u32),
        /// Insert a SetGlobal with a random offset in [low, high).
        /// (low, high)
        RandomSetGlobal(u32, u32),
    }

    pub fn write_access_all_pages_instrs(
        mem_size: WasmPage,
        mut head: Vec<Instruction>,
    ) -> Vec<Instruction> {
        for page in mem_size
            .iter_from_zero()
            .flat_map(|p| p.to_pages_iter::<GearPage>())
        {
            head.push(Instruction::I32Const(page.offset() as i32));
            head.push(Instruction::I32Const(42));
            head.push(Instruction::I32Store(2, 0));
        }
        head
    }

    pub fn read_access_all_pages_instrs(
        mem_size: WasmPage,
        mut head: Vec<Instruction>,
    ) -> Vec<Instruction> {
        for page in mem_size
            .iter_from_zero()
            .flat_map(|p| p.to_pages_iter::<GearPage>())
        {
            head.push(Instruction::I32Const(page.offset() as i32));
            head.push(Instruction::I32Load(2, 0));
            head.push(Instruction::Drop);
        }
        head
    }

    pub fn repeated_dyn_instr(
        repetitions: u32,
        mut instructions: Vec<DynInstr>,
        mut head: Vec<Instruction>,
    ) -> Vec<Instruction> {
        use rand::{distributions::Standard, prelude::*};

        // We do not need to be secure here.
        let mut rng = rand_pcg::Pcg32::seed_from_u64(8446744073709551615);

        // We need to iterate over indices because we cannot cycle over mutable references
        let instr_iter = (0..instructions.len())
            .cycle()
            .take(instructions.len() * usize::try_from(repetitions).unwrap())
            .flat_map(|idx| match &mut instructions[idx] {
                DynInstr::InstrI32Const(c) => vec![Instruction::I32Const(*c as i32)],
                DynInstr::InstrI64Const(c) => vec![Instruction::I64Const(*c as i64)],
                DynInstr::InstrCall(c) => vec![Instruction::Call(*c)],
                DynInstr::InstrI32Load(align, offset) => {
                    vec![Instruction::I32Load(*align, *offset)]
                }
                DynInstr::Regular(instruction) => vec![instruction.clone()],
                DynInstr::Counter(offset, increment_by) => {
                    let current = *offset;
                    *offset += *increment_by;
                    vec![Instruction::I32Const(current as i32)]
                }
                DynInstr::RandomUnaligned(low, high) => {
                    let unaligned = rng.gen_range(*low..*high) | 1;
                    vec![Instruction::I32Const(unaligned as i32)]
                }
                DynInstr::RandomI32(low, high) => {
                    vec![Instruction::I32Const(rng.gen_range(*low..*high))]
                }
                DynInstr::RandomI64(low, high) => {
                    vec![Instruction::I64Const(rng.gen_range(*low..*high))]
                }
                DynInstr::RandomI32Repeated(num) => (&mut rng)
                    .sample_iter(Standard)
                    .take(*num)
                    .map(Instruction::I32Const)
                    .collect(),
                DynInstr::RandomI64Repeated(num) => (&mut rng)
                    .sample_iter(Standard)
                    .take(*num)
                    .map(Instruction::I64Const)
                    .collect(),
                DynInstr::RandomGetLocal(low, high) => {
                    vec![Instruction::GetLocal(rng.gen_range(*low..*high))]
                }
                DynInstr::RandomSetLocal(low, high) => {
                    vec![Instruction::SetLocal(rng.gen_range(*low..*high))]
                }
                DynInstr::RandomTeeLocal(low, high) => {
                    vec![Instruction::TeeLocal(rng.gen_range(*low..*high))]
                }
                DynInstr::RandomGetGlobal(low, high) => {
                    vec![Instruction::GetGlobal(rng.gen_range(*low..*high))]
                }
                DynInstr::RandomSetGlobal(low, high) => {
                    vec![Instruction::SetGlobal(rng.gen_range(*low..*high))]
                }
            });
        head.extend(instr_iter);
        head
    }

    pub fn to_dyn(instructions: &[Instruction]) -> Vec<DynInstr> {
        instructions
            .iter()
            .cloned()
            .map(DynInstr::Regular)
            .collect()
    }

    pub fn with_result_check_dyn(res_offset: DynInstr, instructions: &[DynInstr]) -> Vec<DynInstr> {
        let mut res = vec![
            DynInstr::Regular(Instruction::Block(BlockType::NoResult)),
            res_offset,
            DynInstr::InstrI32Load(2, 0),
            DynInstr::Regular(Instruction::I32Eqz),
            DynInstr::Regular(Instruction::BrIf(0)),
            DynInstr::Regular(Instruction::Unreachable),
            DynInstr::Regular(Instruction::End),
        ];
        res.splice(1..1, instructions.iter().cloned());
        res
    }

    pub fn fallible_syscall_instr(
        repetitions: u32,
        call_index: u32,
        res_offset: DynInstr,
        params: &[DynInstr],
    ) -> Vec<Instruction> {
        let mut instructions = params.to_vec();
        instructions.extend([res_offset.clone(), DynInstr::InstrCall(call_index)]);
        if cfg!(feature = "runtime-benchmarks-checkers") {
            instructions = with_result_check_dyn(res_offset, &instructions);
        }
        repeated_dyn_instr(repetitions, instructions, vec![])
    }

    pub fn from_instructions(mut instructions: Vec<Instruction>) -> FuncBody {
        instructions.push(Instruction::End);
        FuncBody::new(vec![], Instructions::new(instructions))
    }

    pub fn empty() -> FuncBody {
        FuncBody::new(vec![], Instructions::empty())
    }

    pub fn repeated(repetitions: u32, instructions: &[Instruction]) -> FuncBody {
        let instructions = Instructions::new(
            instructions
                .iter()
                .cycle()
                .take(instructions.len() * usize::try_from(repetitions).unwrap())
                .cloned()
                .chain(sp_std::iter::once(Instruction::End))
                .collect(),
        );
        FuncBody::new(vec![], instructions)
    }

    pub fn repeated_dyn(repetitions: u32, instructions: Vec<DynInstr>) -> FuncBody {
        let mut body = repeated_dyn_instr(repetitions, instructions, vec![]);
        body.push(Instruction::End);
        FuncBody::new(vec![], Instructions::new(body))
    }

    pub fn fallible_syscall(repetitions: u32, res_offset: u32, params: &[DynInstr]) -> FuncBody {
        let mut instructions = params.to_vec();
        instructions.extend([DynInstr::InstrI32Const(res_offset), DynInstr::InstrCall(0)]);
        if cfg!(feature = "runtime-benchmarks-checkers") {
            instructions =
                with_result_check_dyn(DynInstr::InstrI32Const(res_offset), &instructions);
        }
        repeated_dyn(repetitions, instructions)
    }

    pub fn syscall(repetitions: u32, params: &[DynInstr]) -> FuncBody {
        let mut instructions = params.to_vec();
        instructions.push(DynInstr::InstrCall(0));
        repeated_dyn(repetitions, instructions)
    }

    pub fn prepend(body: &mut FuncBody, instructions: Vec<Instruction>) {
        body.code_mut()
            .elements_mut()
            .splice(0..0, instructions.iter().cloned());
    }

    /// Replace the locals of the supplied `body` with `num` i64 locals.
    pub fn inject_locals(body: &mut FuncBody, num: u32) {
        use self::elements::Local;
        *body.locals_mut() = vec![Local::new(num, ValueType::I64)];
    }
}

/// The maximum amount of pages any program is allowed to have according to the current `Schedule`.
pub fn max_pages<T: Config>() -> u16
where
    T: Config,
{
    T::Schedule::get().limits.memory_pages
}

// Used for producing different code based on instruction bit width: 32-bit or 64-bit.
enum BitWidth {
    X64,
    X86,
}

impl BitWidth {
    pub fn random_repeated(&self, count: usize) -> body::DynInstr {
        match self {
            BitWidth::X64 => body::DynInstr::RandomI64Repeated(count),
            BitWidth::X86 => body::DynInstr::RandomI32Repeated(count),
        }
    }
}
