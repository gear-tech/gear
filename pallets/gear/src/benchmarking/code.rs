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
    ids::{CodeId, prelude::*},
    pages::{WasmPage, WasmPagesAmount},
};
use gear_sandbox::{
    SandboxEnvironmentBuilder, SandboxMemory,
    default_executor::{EnvironmentDefinitionBuilder, Memory, Store},
};
use gear_wasm_instrument::{
    BlockType, Data, Element, Export, FuncType, Function, Global, Import, Instruction,
    ModuleBuilder, STACK_END_EXPORT_NAME, Table, ValType, syscalls::SyscallName,
};
use sp_std::{convert::TryFrom, marker::PhantomData, prelude::*};

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
    /// Make i64 globals init expr occupy 9 bytes.
    pub full_length_globals: bool,
    /// List of syscalls that the module should import. They start with index 0.
    pub imported_functions: Vec<SyscallName>,
    /// Function body of the exported `init` function. Body is empty if `None`.
    /// Its index is `imported_functions.len()`.
    pub init_body: Option<Function>,
    /// Function body of the exported `handle` function. Body is empty if `None`.
    /// Its index is `imported_functions.len() + 1`.
    pub handle_body: Option<Function>,
    /// Function body of the exported `handle_reply` function. Body is empty if `None`.
    /// Its index is `imported_functions.len() + 2`.
    pub reply_body: Option<Function>,
    /// Function body of the exported `handle_signal` function. Body is empty if `None`.
    /// Its index is `imported_functions.len() + 3`.
    pub signal_body: Option<Function>,
    /// Function body of a non-exported function with index `imported_functions.len() + 4`.
    pub aux_body: Option<Function>,
    /// The amount of I64 arguments the aux function should have.
    pub aux_arg_num: u32,
    pub aux_res: Option<ValType>,
    /// Create a table containing function pointers.
    pub table: Option<TableSegment>,
    /// Create a type section with the specified amount of types.
    pub types: Option<TypeSegment>,
    /// Create a section named "dummy" of the specified size. This is useful in order to
    /// benchmark the overhead of loading and storing codes of specified sizes. The dummy
    /// section only contributes to the size of the program but does not affect execution.
    pub dummy_section: u32,
    /// Create global export [STACK_END_GLOBAL_NAME] with the given page offset addr.
    /// If None, then all memory supposed to be stack memory, so stack end will be equal to memory size.
    pub stack_end: Option<WasmPage>,
}

#[derive(Default)]
pub enum InitElements {
    NoInit,
    Number(u32),
    #[default]
    All,
}

pub struct TableSegment {
    /// How many elements should be created inside the table.
    pub num_elements: u32,
    /// The function index with which all table elements should be initialized.
    pub function_index: u32,
    /// Generate element segment which initialize the table.
    pub init_elements: InitElements,
}

pub struct TypeSegment {
    pub num_elements: u32,
}

pub struct DataSegment {
    pub offset: u32,
    pub value: Vec<u8>,
}

#[derive(Clone)]
pub struct ImportedMemory {
    pub min_pages: WasmPagesAmount,
}

impl ImportedMemory {
    pub fn max<T: Config>() -> Self {
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
    pub params: Vec<ValType>,
    pub return_type: Option<ValType>,
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
        let mut program = ModuleBuilder::default();
        // init function (first internal function)
        program.add_func(
            FuncType::new([], []),
            def.init_body.unwrap_or_else(body::empty),
        );
        // handle function (second internal function)
        program.add_func(
            FuncType::new([], []),
            def.handle_body.unwrap_or_else(body::empty),
        );

        program.add_func(
            FuncType::new([], []),
            def.reply_body.unwrap_or_else(body::empty),
        );

        program.add_func(
            FuncType::new([], []),
            def.signal_body.unwrap_or_else(body::empty),
        );

        program.push_export(Export::func("init", func_offset + OFFSET_INIT));
        program.push_export(Export::func("handle", func_offset + OFFSET_HANDLE));
        program.push_export(Export::func("handle_reply", func_offset + OFFSET_REPLY));
        program.push_export(Export::func("handle_signal", func_offset + OFFSET_SIGNAL));

        // If specified we add an additional internal function
        if let Some(body) = def.aux_body {
            let ty = FuncType::new(
                vec![ValType::I64; def.aux_arg_num as usize],
                def.aux_res.into_iter(),
            );
            program.add_func(ty, body);
        }

        // Grant access to linear memory.
        if let Some(memory) = &def.memory {
            program.push_import(Import::memory(memory.min_pages.into(), None));
        }

        // Import supervisor functions. They start with idx 0.
        for name in def.imported_functions {
            let sign = name.signature();
            let sig = program.push_type(sign.func_type());
            program.push_import(Import::func("env", name.to_str(), sig));
        }

        // Initialize memory
        for data in def.data_segments {
            program.push_data(Data::with_offset(data.value, data.offset));
        }

        // Add global variables
        if def.num_globals > 0 {
            use rand::{distributions::Standard, prelude::*};
            let rng = rand_pcg::Pcg32::seed_from_u64(3112244599778833558);
            for mut value in rng.sample_iter(Standard).take(def.num_globals as usize) {
                // Make i64 const init expr use full length
                if def.full_length_globals {
                    value |= 1 << 63;
                }

                program.push_global(Global::i64_value_mut(value));
            }
        }

        // Add stack end export
        let stack_end = def.stack_end.unwrap_or(
            // Set all static memory as stack
            def.memory
                .as_ref()
                .map(|memory| {
                    memory
                        .min_pages
                        .to_page_number()
                        .expect("memory size is too big")
                })
                .unwrap_or(0.into()),
        );

        program.push_global(Global::i32_value(stack_end.offset() as i32));
        program.push_export(Export::global(STACK_END_EXPORT_NAME, def.num_globals));

        // Add function pointer table
        if let Some(table) = def.table {
            let functions = match table.init_elements {
                InitElements::NoInit => {
                    vec![]
                }
                InitElements::Number(num) => {
                    vec![table.function_index; num as usize]
                }
                InitElements::All => {
                    vec![table.function_index; table.num_elements as usize]
                }
            };

            program.set_table(Table::funcref(table.num_elements, Some(table.num_elements)));
            program.push_element(Element::functions(functions));
        }

        // Add the dummy section
        if def.dummy_section > 0 {
            program.push_custom_section("dummy", vec![42; def.dummy_section as usize]);
        }

        // Add dummy type section
        if let Some(types) = def.types {
            for proto in generate_uniq_prototypes(types.num_elements as usize).into_iter() {
                program.push_type(proto);
            }
        }

        let code = program.build().serialize().unwrap();
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
        use Instruction::{Drop, End, I32Const, I64Const, I64Eq, If, Return};
        // Base size of a program is 63 bytes and each expansion adds 20 bytes.
        // We do one expansion less to account for the code section and function body
        // size fields inside the binary wasm module representation which are leb128 encoded
        // and therefore grow in size when the program grows. We are not allowed to overshoot
        // because of the maximum code size that is enforced by `instantiate_with_code`.
        let expansions = (target_bytes.saturating_sub(63) / 20).saturating_sub(1);
        const EXPANSION: &[Instruction] = &[
            I64Const(0),
            I64Const(1),
            I64Eq,
            If(BlockType::Empty),
            Return,
            End,
            I32Const(0xffffff),
            Drop,
            I32Const(0),
            If(BlockType::Empty),
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

    /// Creates a WebAssembly module with a data section of size `data_section_bytes`.
    /// The generated module contains `data_segment_num` data segments with an overall size of `data_section_bytes`.
    /// If `data_segment_num` is 0, no data segments are added.
    /// If the result of dividing `data_section_bytes` by `data_segment_num` is 0, zero-length data segments are added.
    pub fn sized_data_section(data_section_bytes: u32, data_segment_num: u32) -> Self {
        let mut module = ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            ..Default::default()
        };

        if data_segment_num != 0 {
            let (data_segment_size, residual_bytes) = (
                data_section_bytes / data_segment_num,
                data_section_bytes % data_segment_num,
            );

            for seg_idx in 0..data_segment_num {
                module.data_segments.push(DataSegment {
                    offset: seg_idx * data_segment_size,
                    value: vec![0xA5; data_segment_size as usize],
                });
            }

            // Add residual bytes to the last data segment
            if residual_bytes != 0
                && let Some(last) = module.data_segments.last_mut()
            {
                last.value
                    .resize(data_segment_size as usize + residual_bytes as usize, 0xA5)
            }
        }

        module.into()
    }

    /// Creates a wasm module of `target_bytes` size.
    /// The generated module generates wasm module containing only global section.
    pub fn sized_global_section(target_bytes: u32) -> Self {
        let mut module = ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            ..Default::default()
        };

        // Maximum size of encoded i64 global is 14 bytes.
        module.num_globals = target_bytes / 14;
        module.full_length_globals = true;

        module.into()
    }

    /// Creates a WebAssembly module with a table size of `num_elements` bytes.
    /// Each element in the table points to function index `0` and occupies 1 byte.
    pub fn sized_table_section(num_elements: u32, init_elements: Option<u32>) -> Self {
        let mut module = ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            ..Default::default()
        };

        module.init_body = Some(body::empty());

        // 1 element with function index value `0` occupies 1 byte
        module.table = Some(TableSegment {
            num_elements,
            function_index: 0,
            init_elements: match init_elements {
                Some(num) => InitElements::Number(num),
                None => InitElements::NoInit,
            },
        });

        module.into()
    }

    /// Creates a WebAssembly module with a type section of size `target_bytes` bytes.
    pub fn sized_type_section(target_bytes: u32) -> Self {
        let mut module = ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            ..Default::default()
        };

        // Dummy type takes on average 10.5 bytes.
        module.types = Some(TypeSegment {
            num_elements: target_bytes * 2 / 21,
        });

        module.into()
    }

    /// Creates a memory instance for use in a sandbox with dimensions declared in this module
    /// and adds it to `env`. A reference to that memory is returned so that it can be used to
    /// access the memory contents from the supervisor.
    pub fn add_memory<S>(
        &self,
        store: &mut Store<()>,
        env: &mut EnvironmentDefinitionBuilder<S>,
    ) -> Option<Memory> {
        let memory = if let Some(memory) = &self.memory {
            memory
        } else {
            return None;
        };
        let memory = Memory::new(store, memory.min_pages.into(), None).unwrap();
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
    use super::*;
    use gear_core::pages::{GearPage, WasmPage, numerated::iterators::IntervalIterator};
    use gear_wasm_instrument::{BlockType, MemArg};

    /// When generating program code by repeating a wasm sequence, it's sometimes necessary
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
        InstrI32Load(u8, u32),
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
        /// Insert the specified amount of Drop.
        DropRepeated(usize),
    }

    pub fn write_access_all_pages_instrs(
        end_page: WasmPage,
        mut head: Vec<Instruction>,
    ) -> Vec<Instruction> {
        IntervalIterator::from(..end_page)
            .flat_map(|p: WasmPage| p.to_iter())
            .for_each(|page: GearPage| {
                head.push(Instruction::I32Const(page.offset() as i32));
                head.push(Instruction::I32Const(42));
                head.push(Instruction::I32Store(MemArg::i32()));
            });
        head
    }

    pub fn read_access_all_pages_instrs(
        end_page: WasmPage,
        mut head: Vec<Instruction>,
    ) -> Vec<Instruction> {
        IntervalIterator::from(..end_page)
            .flat_map(|p: WasmPage| p.to_iter())
            .for_each(|page: GearPage| {
                head.push(Instruction::I32Const(page.offset() as i32));
                head.push(Instruction::I32Load(MemArg::i32()));
                head.push(Instruction::Drop);
            });
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
                    vec![Instruction::I32Load(MemArg {
                        align: *align,
                        offset: *offset,
                    })]
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
                    vec![Instruction::LocalGet(rng.gen_range(*low..*high))]
                }
                DynInstr::RandomSetLocal(low, high) => {
                    vec![Instruction::LocalSet(rng.gen_range(*low..*high))]
                }
                DynInstr::RandomTeeLocal(low, high) => {
                    vec![Instruction::LocalTee(rng.gen_range(*low..*high))]
                }
                DynInstr::RandomGetGlobal(low, high) => {
                    vec![Instruction::GlobalGet(rng.gen_range(*low..*high))]
                }
                DynInstr::RandomSetGlobal(low, high) => {
                    vec![Instruction::GlobalSet(rng.gen_range(*low..*high))]
                }
                DynInstr::DropRepeated(num) => vec![Instruction::Drop; *num],
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
            DynInstr::Regular(Instruction::Block(BlockType::Empty)),
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

    pub fn from_instructions(mut instructions: Vec<Instruction>) -> Function {
        instructions.push(Instruction::End);
        Function::from_instructions(instructions)
    }

    pub fn empty() -> Function {
        Function::default()
    }

    pub fn repeated(repetitions: u32, instructions: &[Instruction]) -> Function {
        let instructions = instructions
            .iter()
            .cycle()
            .take(instructions.len() * usize::try_from(repetitions).unwrap())
            .cloned()
            .collect();
        from_instructions(instructions)
    }

    pub fn repeated_dyn(repetitions: u32, instructions: Vec<DynInstr>) -> Function {
        let instructions = repeated_dyn_instr(repetitions, instructions, vec![]);
        from_instructions(instructions)
    }

    pub fn fallible_syscall(repetitions: u32, res_offset: u32, params: &[DynInstr]) -> Function {
        let mut instructions = params.to_vec();
        instructions.extend([DynInstr::InstrI32Const(res_offset), DynInstr::InstrCall(0)]);
        if cfg!(feature = "runtime-benchmarks-checkers") {
            instructions =
                with_result_check_dyn(DynInstr::InstrI32Const(res_offset), &instructions);
        }
        repeated_dyn(repetitions, instructions)
    }

    pub fn syscall(repetitions: u32, params: &[DynInstr]) -> Function {
        let mut instructions = params.to_vec();
        instructions.push(DynInstr::InstrCall(0));
        repeated_dyn(repetitions, instructions)
    }

    pub fn prepend(body: &mut Function, instructions: Vec<Instruction>) {
        body.instructions.splice(0..0, instructions.iter().cloned());
    }

    /// Replace the locals of the supplied `body` with `num` i64 locals.
    pub fn inject_locals(body: &mut Function, num: u32) {
        body.locals = vec![(num, ValType::I64)];
    }

    pub fn unreachable_condition_i32(
        instructions: &mut Vec<Instruction>,
        flag: Instruction,
        compare_with: i32,
    ) {
        let additional = vec![
            Instruction::I32Const(compare_with),
            flag,
            Instruction::If(BlockType::Empty),
            Instruction::Unreachable,
            Instruction::End,
        ];

        instructions.extend(additional)
    }
}

/// The maximum amount of pages any program is allowed to have according to the current `Schedule`.
pub fn max_pages<T>() -> u16
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

// Generate `number` unique WASM function prototypes
fn generate_uniq_prototypes(number: usize) -> Vec<FuncType> {
    // NOTE: types `F32`, `F64`, `V128` are not supported and only used for
    // dummy type section generation.
    const ALPHABET: [ValType; 5] = [
        ValType::I32,
        ValType::I64,
        ValType::F32,
        ValType::F64,
        ValType::V128,
    ];

    let mut out = Vec::with_capacity(number);
    if number == 0 {
        return out;
    }

    let k = ALPHABET.len();
    let mut params_len: usize = 0;

    'fill: loop {
        let patterns = k.pow(params_len as u32);

        for idx in 0..patterns {
            let params = index_to_types(params_len, idx, &ALPHABET, k);
            out.push(FuncType::new(params, [ValType::I64; 1]));

            if out.len() == number {
                break 'fill;
            }
        }

        params_len += 1;
    }

    out
}

// Map an index in base-`k` to a sequence of `len` ValTypes.
// Least-significant “digit” becomes the first element.
fn index_to_types(len: usize, mut idx: usize, alphabet: &[ValType], k: usize) -> Vec<ValType> {
    let mut v = Vec::with_capacity(len);
    for _ in 0..len {
        let d = idx % k;
        v.push(alphabet[d]);
        idx /= k;
    }
    v
}
