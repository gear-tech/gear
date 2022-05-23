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
//! we define this simple definition of a program that can be passed to `submit_code` that
//! compiles it down into a `WasmModule` that can be used as a program's code.

use crate::Config;
use common::Origin;
use frame_support::traits::Get;
use gear_core::ids::CodeId;
use sp_sandbox::{
    default_executor::{EnvironmentDefinitionBuilder, Memory},
    SandboxEnvironmentBuilder, SandboxMemory,
};
use sp_std::{borrow::ToOwned, convert::TryFrom, marker::PhantomData, prelude::*};
use wasm_instrument::parity_wasm::{
    builder,
    elements::{
        self, BlockType, CustomSection, FuncBody, Instruction, Instructions, Module, Section,
        ValueType,
    },
};

/// The location where to put the generated code.
pub enum Location {
    /// Generate all code into the `init` exported function.
    Init,
    /// Generate all code into the `handle` exported function.
    Handle,
}

/// Pass to `submit_code` in order to create a compiled `WasmModule`.
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
    /// List of functions that the module should import. They start with index 0.
    pub imported_functions: Vec<ImportedFunction>,
    /// Function body of the exported `init` function. Body is empty if `None`.
    /// Its index is `imported_functions.len()`.
    pub init_body: Option<FuncBody>,
    /// Function body of the exported `handle` function. Body is empty if `None`.
    /// Its index is `imported_functions.len() + 1`.
    pub handle_body: Option<FuncBody>,
    /// Function body of a non-exported function with index `imported_functions.len() + 2`.
    pub aux_body: Option<FuncBody>,
    /// The amount of I64 arguments the aux function should have.
    pub aux_arg_num: u32,
    /// If set to true the stack height limiter is injected into the the module. This is
    /// needed for instruction debugging because the cost of executing the stack height
    /// instrumentation should be included in the costs for the individual instructions
    /// that cause more metering code (only call).
    pub inject_stack_metering: bool,
    /// Create a table containing function pointers.
    pub table: Option<TableSegment>,
    /// Create a section named "dummy" of the specified size. This is useful in order to
    /// benchmark the overhead of loading and storing codes of specified sizes. The dummy
    /// section only contributes to the size of the program but does not affect execution.
    pub dummy_section: u32,
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
    pub min_pages: u32,
}

impl ImportedMemory {
    pub fn max<T: Config>() -> Self
    where
        T: Config,
    {
        let pages = max_pages::<T>();
        Self { min_pages: pages }
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
            .with_body(
                def.init_body
                    .unwrap_or_else(|| FuncBody::new(Vec::new(), Instructions::empty())),
            )
            .build()
            // handle function (second internal function)
            .function()
            .signature()
            .build()
            .with_body(
                def.handle_body
                    .unwrap_or_else(|| FuncBody::new(Vec::new(), Instructions::empty())),
            )
            .build()
            .export()
            .field("init")
            .internal()
            .func(func_offset)
            .build()
            .export()
            .field("handle")
            .internal()
            .func(func_offset + 1)
            .build();

        // If specified we add an additional internal function
        if let Some(body) = def.aux_body {
            let mut signature = program.function().signature();
            for _ in 0..def.aux_arg_num {
                signature = signature.with_param(ValueType::I64);
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
                .memory(memory.min_pages, None)
                .build();
        }

        // Import supervisor functions. They start with idx 0.
        for func in def.imported_functions {
            let sig = builder::signature()
                .with_params(func.params)
                .with_results(func.return_type.into_iter().collect())
                .build_sig();
            let sig = program.push_signature(sig);
            program = program
                .import()
                .module(func.module)
                .field(func.name)
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

        let mut code = program.build();

        if def.inject_stack_metering {
            code = inject_stack_metering::<T>(code);
        }

        let code = code.to_bytes().unwrap();
        let hash = CodeId::generate(&code);
        Self {
            code,
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
        use self::elements::Instruction::{End, I32Const, If, Return};
        // Base size of a program is 63 bytes and each expansion adds 6 bytes.
        // We do one expansion less to account for the code section and function body
        // size fields inside the binary wasm module representation which are leb128 encoded
        // and therefore grow in size when the contract grows. We are not allowed to overshoot
        // because of the maximum code size that is enforced by `instantiate_with_code`.
        let expansions = (target_bytes.saturating_sub(63) / 6).saturating_sub(1);
        const EXPANSION: [Instruction; 4] = [I32Const(0), If(BlockType::NoResult), Return, End];
        let mut module = ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            ..Default::default()
        };
        let body = Some(body::repeated(expansions, &EXPANSION));
        match code_location {
            Location::Init => module.init_body = body,
            Location::Handle => module.handle_body = body,
        }
        module.into()
    }

    /// Creates a wasm module that calls the imported function named `getter_name` `repeat`
    /// times. The imported function is expected to have the "getter signature" of
    /// (out_ptr: u32) -> ().
    pub fn getter(module_name: &'static str, getter_name: &'static str, repeat: u32) -> Self {
        let pages = max_pages::<T>();
        ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![ImportedFunction {
                module: module_name,
                name: getter_name,
                params: vec![ValueType::I32],
                return_type: None,
            }],
            // Write the output buffer size. The output size will be overwritten by the
            // supervisor with the real size when calling the getter. Since this size does not
            // change between calls it suffices to start with an initial value and then just
            // leave as whatever value was written there.
            data_segments: vec![DataSegment {
                offset: 0,
                value: (pages * 64 * 1024 - 4).to_le_bytes().to_vec(),
            }],
            handle_body: Some(body::repeated(
                repeat,
                &[
                    Instruction::I32Const(4), // ptr where to store output
                    Instruction::Call(0),     // call the imported function
                ],
            )),
            ..Default::default()
        }
        .into()
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
        let memory = Memory::new(memory.min_pages, None).unwrap();
        env.add_memory("env", "memory", memory.clone());
        Some(memory)
    }

    pub fn unary_instr(instr: Instruction, repeat: u32) -> Self {
        use body::DynInstr::{RandomI64Repeated, Regular};
        ModuleDefinition {
            handle_body: Some(body::repeated_dyn(
                repeat,
                vec![
                    RandomI64Repeated(1),
                    Regular(instr),
                    Regular(Instruction::Drop),
                ],
            )),
            ..Default::default()
        }
        .into()
    }

    pub fn binary_instr(instr: Instruction, repeat: u32) -> Self {
        use body::DynInstr::{RandomI64Repeated, Regular};
        ModuleDefinition {
            handle_body: Some(body::repeated_dyn(
                repeat,
                vec![
                    RandomI64Repeated(2),
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

    /// When generating contract code by repeating a wasm sequence, it's sometimes necessary
    /// to change those instructions on each repetition. The variants of this enum describe
    /// various ways in which this can happen.
    pub enum DynInstr {
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

    pub fn plain(instructions: Vec<Instruction>) -> FuncBody {
        FuncBody::new(Vec::new(), Instructions::new(instructions))
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
        FuncBody::new(Vec::new(), instructions)
    }

    pub fn repeated_dyn(repetitions: u32, mut instructions: Vec<DynInstr>) -> FuncBody {
        use rand::{distributions::Standard, prelude::*};

        // We do not need to be secure here.
        let mut rng = rand_pcg::Pcg32::seed_from_u64(8446744073709551615);

        // We need to iterate over indices because we cannot cycle over mutable references
        let body = (0..instructions.len())
            .cycle()
            .take(instructions.len() * usize::try_from(repetitions).unwrap())
            .flat_map(|idx| match &mut instructions[idx] {
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
            })
            .chain(sp_std::iter::once(Instruction::End))
            .collect();
        FuncBody::new(Vec::new(), Instructions::new(body))
    }

    /// Replace the locals of the supplied `body` with `num` i64 locals.
    pub fn inject_locals(body: &mut FuncBody, num: u32) {
        use self::elements::Local;
        *body.locals_mut() = vec![Local::new(num, ValueType::I64)];
    }
}

/// The maximum amount of pages any program is allowed to have according to the current `Schedule`.
pub fn max_pages<T: Config>() -> u32
where
    T: Config,
{
    T::Schedule::get().limits.memory_pages
}

fn inject_stack_metering<T: Config>(module: Module) -> Module {
    let height = T::Schedule::get().limits.stack_height;
    wasm_instrument::inject_stack_limiter(module, height).unwrap()
}
