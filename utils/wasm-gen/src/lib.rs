// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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

use std::{collections::BTreeMap, iter::Cycle, ops::RangeInclusive};

use arbitrary::Unstructured;
use gear_wasm_instrument::{
    parity_wasm::{
        self, builder,
        elements::{
            External, FunctionType, ImportCountType, Instruction, Instructions, Internal, Module,
            Section, Type, ValueType,
        },
    },
    syscalls::SysCallName,
    STACK_END_EXPORT_NAME,
};
pub use gsys;
use gsys::HashWithValue;
use wasm_smith::{InstructionKind::*, InstructionKinds, Module as ModuleSmith, SwarmConfig};

mod syscalls;
use syscalls::{sys_calls_table, Parameter, SysCallInfo, SyscallsConfig};

#[cfg(test)]
mod tests;

pub mod utils;
pub mod wasm;
use wasm::{PageCount as WasmPageCount, PAGE_SIZE as WASM_PAGE_SIZE};

pub mod memory;
use memory::ModuleBuilderWithData;

const MEMORY_VALUE_SIZE: u32 = 100;
const MEMORY_FIELD_NAME: &str = "memory";

#[derive(Clone, Copy, Debug)]
pub struct Ratio {
    numerator: u32,
    denominator: u32,
}

impl Ratio {
    pub fn get(&self, u: &mut Unstructured) -> bool {
        if self.numerator == 0 {
            false
        } else {
            u.ratio(self.numerator, self.denominator).unwrap()
        }
    }
}

impl From<(u32, u32)> for Ratio {
    fn from((numerator, denominator): (u32, u32)) -> Self {
        Self {
            numerator,
            denominator,
        }
    }
}

impl Ratio {
    pub fn mult<T: Into<usize>>(&self, x: T) -> usize {
        (T::into(x) * self.numerator as usize) / self.denominator as usize
    }
}

#[derive(Debug, Clone)]
pub struct ParamRule {
    pub allowed_values: RangeInclusive<i64>,
    pub unrestricted_ratio: Ratio,
}

impl Default for ParamRule {
    fn default() -> Self {
        Self {
            allowed_values: 0..=0,
            unrestricted_ratio: (100, 100).into(),
        }
    }
}

impl ParamRule {
    pub fn get_i32(&self, u: &mut Unstructured) -> i32 {
        if self.unrestricted_ratio.get(u) {
            u.arbitrary().unwrap()
        } else {
            let start = if *self.allowed_values.start() < i32::MIN as i64 {
                i32::MIN
            } else {
                *self.allowed_values.start() as i32
            };
            let end = if *self.allowed_values.end() > i32::MAX as i64 {
                i32::MAX
            } else {
                *self.allowed_values.end() as i32
            };
            u.int_in_range(start..=end).unwrap()
        }
    }
    pub fn get_i64(&self, u: &mut Unstructured) -> i64 {
        if self.unrestricted_ratio.get(u) {
            u.arbitrary().unwrap()
        } else {
            u.int_in_range(self.allowed_values.clone()).unwrap()
        }
    }
}

#[derive(Clone)]
pub struct GearConfig {
    pub process_when_no_funcs: Ratio,
    pub skip_init: Ratio,
    pub skip_handle: Ratio,
    pub skip_handle_reply: Ratio,
    pub skip_init_when_no_funcs: Ratio,
    pub remove_recursion: Ratio,
    pub init_export_is_any_func: Ratio,
    pub max_mem_size: u32,
    pub max_mem_delta: u32,
    pub has_mem_upper_bound: Ratio,
    pub upper_bound_can_be_less_then: Ratio,
    pub sys_call_freq: Ratio,
    pub sys_calls: SyscallsConfig,
    pub print_test_info: Option<String>,
    pub max_percentage_seed: u32,
    pub unchecked_memory_access: Ratio,
    pub use_message_source: Ratio,
    pub call_indirect_enabled: bool,
}

impl GearConfig {
    pub fn new_normal() -> Self {
        let prob = (1, 100).into();
        Self {
            process_when_no_funcs: prob,
            skip_init: (1, 1000).into(),
            skip_handle: prob,
            skip_handle_reply: prob,
            skip_init_when_no_funcs: prob,
            remove_recursion: (80, 100).into(),
            init_export_is_any_func: prob,
            max_mem_size: 1024,
            max_mem_delta: 1024,
            has_mem_upper_bound: prob,
            upper_bound_can_be_less_then: prob,
            sys_call_freq: (1, 1000).into(),
            sys_calls: Default::default(),
            print_test_info: None,
            max_percentage_seed: 100,
            unchecked_memory_access: prob,
            use_message_source: (50, 100).into(),
            call_indirect_enabled: true,
        }
    }
    pub fn new_for_rare_cases() -> Self {
        let prob = (50, 100).into();
        Self {
            skip_init: prob,
            skip_handle: prob,
            skip_handle_reply: prob,
            skip_init_when_no_funcs: prob,
            remove_recursion: prob,
            process_when_no_funcs: prob,
            init_export_is_any_func: prob,
            max_mem_size: 1024,
            max_mem_delta: 1024,
            has_mem_upper_bound: prob,
            upper_bound_can_be_less_then: prob,
            sys_call_freq: (1, 1000).into(),
            sys_calls: Default::default(),
            print_test_info: None,
            max_percentage_seed: 5,
            unchecked_memory_access: prob,
            use_message_source: prob,
            call_indirect_enabled: true,
        }
    }
    pub fn new_valid() -> Self {
        let prob = (1, 100).into();
        let zero_prob = (0, 100).into();
        Self {
            process_when_no_funcs: prob,
            skip_init: zero_prob,
            skip_handle: zero_prob,
            skip_handle_reply: zero_prob,
            skip_init_when_no_funcs: zero_prob,
            remove_recursion: zero_prob,
            init_export_is_any_func: zero_prob,
            max_mem_size: 512,
            max_mem_delta: 256,
            has_mem_upper_bound: prob,
            upper_bound_can_be_less_then: zero_prob,
            sys_call_freq: (1, 1000).into(),
            sys_calls: Default::default(),
            print_test_info: None,
            max_percentage_seed: 100,
            unchecked_memory_access: zero_prob,
            use_message_source: zero_prob,
            call_indirect_enabled: true,
        }
    }
}

// Module and an optional index of gr_debug syscall.
struct ModuleWithDebug {
    module: Module,
    debug_syscall_index: Option<u32>,
    last_offset: u32,
}

impl From<ModuleBuilderWithData> for ModuleWithDebug {
    fn from(data: ModuleBuilderWithData) -> Self {
        let module = data.module_builder.build();
        Self {
            module,
            debug_syscall_index: None,
            last_offset: data.last_offset,
        }
    }
}

impl From<(Module, Option<u32>, u32)> for ModuleWithDebug {
    fn from((module, debug_syscall_index, last_offset): (Module, Option<u32>, u32)) -> Self {
        Self {
            module,
            debug_syscall_index,
            last_offset,
        }
    }
}

pub fn default_swarm_config(u: &mut Unstructured, gear_config: &GearConfig) -> SwarmConfig {
    let mut cfg: SwarmConfig = u.arbitrary().unwrap();

    cfg.allowed_instructions = InstructionKinds::new(&[
        Numeric, Control, Parametric, Variable, Reference, Table, Memory,
    ]);

    cfg.sign_extension_enabled = false;
    cfg.saturating_float_to_int_enabled = false;
    cfg.reference_types_enabled = false;
    cfg.bulk_memory_enabled = false;
    cfg.simd_enabled = false;
    cfg.float_enabled = false;
    cfg.relaxed_simd_enabled = false;
    cfg.exceptions_enabled = false;
    cfg.memory64_enabled = false;
    cfg.allow_start_export = false;
    cfg.multi_value_enabled = false;
    cfg.memory_grow_enabled = false;
    cfg.call_indirect_enabled = gear_config.call_indirect_enabled;

    cfg.max_memories = 1;
    cfg.max_tables = 1;

    cfg.min_exports = 0;
    cfg.max_exports = 0;

    cfg.max_imports = 0;
    cfg.min_imports = 0;

    cfg.max_instructions = 100000;
    cfg.max_memory_pages = gear_config.max_mem_size as u64;
    cfg.max_funcs = 100;
    cfg.min_funcs = u.int_in_range(0..=30).unwrap();

    cfg
}

pub fn gen_wasm_smith_module(u: &mut Unstructured, config: &SwarmConfig) -> ModuleSmith {
    loop {
        if let Ok(module) = ModuleSmith::new(config.clone(), u) {
            return module;
        }
    }
}

fn build_checked_call(
    u: &mut Unstructured,
    results: &[ValueType],
    params_rules: &[Parameter],
    func_no: u32,
    memory_pages: WasmPageCount,
    unchecked_memory: Ratio,
) -> Vec<Instruction> {
    let unchecked = unchecked_memory.get(u);

    let mut code = Vec::with_capacity(params_rules.len() * 2 + 1 + results.len());
    for parameter in params_rules {
        match parameter {
            Parameter::Value { value_type, rule } => {
                let instr = match value_type {
                    ValueType::I32 => Instruction::I32Const(rule.get_i32(u)),
                    ValueType::I64 => Instruction::I64Const(rule.get_i64(u)),
                    _ => panic!("Cannot handle f32/f64"),
                };
                code.push(instr);
            }

            Parameter::MemoryArray => {
                if unchecked {
                    code.push(Instruction::I32Const(
                        u.arbitrary()
                            .expect("Unstructured::arbitrary failed for MemoryArray"),
                    ));
                    code.push(Instruction::I32Const(
                        u.arbitrary()
                            .expect("Unstructured::arbitrary failed for MemoryArray"),
                    ));
                } else {
                    let memory_size = memory_pages.memory_size();
                    let upper_limit = memory_size.saturating_sub(1);

                    let pointer_beyond = u
                        .int_in_range(0..=upper_limit)
                        .expect("Unstructured::int_in_range failed for MemoryArray");
                    let offset = u
                        .int_in_range(0..=pointer_beyond)
                        .expect("Unstructured::int_in_range failed for MemoryArray");

                    code.push(Instruction::I32Const(offset as i32));
                    code.push(Instruction::I32Const((pointer_beyond - offset) as i32));
                }
            }

            Parameter::MemoryValue => {
                if unchecked {
                    code.push(Instruction::I32Const(
                        u.arbitrary()
                            .expect("Unstructured::arbitrary failed for MemoryValue"),
                    ));
                } else {
                    let memory_size = memory_pages.memory_size();
                    // Subtract a bit more so entities from gsys fit.
                    let upper_limit = memory_size.saturating_sub(MEMORY_VALUE_SIZE);
                    let offset = u
                        .int_in_range(0..=upper_limit)
                        .expect("Unstructured::int_in_range failed for MemoryValue");

                    code.push(Instruction::I32Const(offset as i32));
                }
            }

            Parameter::Alloc => {
                if unchecked {
                    code.push(Instruction::I32Const(
                        u.arbitrary()
                            .expect("Unstructured::arbitrary failed for Alloc"),
                    ));
                } else {
                    let pages_to_alloc = u
                        .int_in_range(0..=memory_pages.raw().saturating_sub(1))
                        .expect("Unstructured::int_in_range failed for Alloc");

                    code.push(Instruction::I32Const(pages_to_alloc as i32));
                }
            }
        }
    }

    code.push(Instruction::Call(func_no));
    code.extend(results.iter().map(|_| Instruction::Drop));
    code
}

fn make_call_instructions_vec(
    u: &mut Unstructured,
    params: &[ValueType],
    results: &[ValueType],
    func_no: u32,
) -> Vec<Instruction> {
    let mut code = Vec::with_capacity(params.len() + 1 + results.len());
    for val in params {
        let instr = match val {
            ValueType::I32 => Instruction::I32Const(
                u.arbitrary()
                    .expect("Unstructured::arbitrary failed for I32"),
            ),
            ValueType::I64 => Instruction::I64Const(
                u.arbitrary()
                    .expect("Unstructured::arbitrary failed for I64"),
            ),
            _ => panic!("Cannot handle f32/f64"),
        };
        code.push(instr);
    }
    code.push(Instruction::Call(func_no));
    code.extend(results.iter().map(|_| Instruction::Drop));

    code
}

#[derive(Debug, Clone, Copy)]
enum FuncIdx {
    Import(u32),
    Func(u32),
}

fn get_func_type(module: &Module, func_idx: FuncIdx) -> FunctionType {
    match func_idx {
        FuncIdx::Import(idx) => {
            let type_no = if let External::Function(type_no) =
                module.import_section().unwrap().entries()[idx as usize].external()
            {
                *type_no as usize
            } else {
                panic!("Import func index must be for import function");
            };
            let Type::Function(func_type) = &module.type_section().unwrap().types()[type_no];
            func_type.clone()
        }
        FuncIdx::Func(idx) => {
            let func = module.function_section().unwrap().entries()[idx as usize];
            let Type::Function(func_type) =
                &module.type_section().unwrap().types()[func.type_ref() as usize];
            func_type.clone()
        }
    }
}

struct WasmGen<'a> {
    u: &'a mut Unstructured<'a>,
    config: GearConfig,
    calls_indexes: Vec<FuncIdx>,
}

enum GearStackEndExportSeed {
    NotGenerate,
    GenerateValue(u32),
}

struct SyscallData {
    info: SysCallInfo,
    sys_call_amount: usize,
    call_index: u32,
}

impl<'a> WasmGen<'a> {
    fn initial_calls_indexes(module: &Module) -> Vec<FuncIdx> {
        let mut calls_indexes = Vec::new();
        let import_funcs_num = module
            .import_section()
            .map(|imps| imps.functions() as u32)
            .unwrap_or(0);
        let code_funcs_num = module
            .function_section()
            .map(|funcs| funcs.entries().len() as u32)
            .unwrap_or(0);
        for i in 0..import_funcs_num {
            calls_indexes.push(FuncIdx::Import(i));
        }
        for i in 0..code_funcs_num {
            calls_indexes.push(FuncIdx::Func(i));
        }
        calls_indexes
    }

    pub fn new(module: &Module, u: &'a mut Unstructured<'a>, config: GearConfig) -> Self {
        let calls_indexes = Self::initial_calls_indexes(module);
        Self {
            u,
            config,
            calls_indexes,
        }
    }

    // ~1% of cases with invalid stack size not a multiple of the page size
    // ~1% of cases with invalid stack size that is bigger than import memory
    // ~1% of cases stack size is not generated at all
    // all other cases should be valid
    fn get_gear_stack_end_seed(&mut self, min_memory_size_pages: u32) -> GearStackEndExportSeed {
        const NOT_GENERATE_SEED: u32 = 0;
        const NOT_WASM_PAGE_SEED: u32 = 1;
        const BIGGER_THAN_MEMORY_SEED: u32 = 2;

        let seed = self
            .u
            .int_in_range(0..=self.config.max_percentage_seed)
            .unwrap();
        match seed {
            NOT_GENERATE_SEED => GearStackEndExportSeed::NotGenerate,
            NOT_WASM_PAGE_SEED => {
                let max_size = min_memory_size_pages * WASM_PAGE_SIZE;
                // More likely value is not multiple of WASM_PAGE_SIZE_BYTES
                let value = self.u.int_in_range(0..=max_size).unwrap();
                GearStackEndExportSeed::GenerateValue(value)
            }
            BIGGER_THAN_MEMORY_SEED => {
                let value_pages = self
                    .u
                    .int_in_range(min_memory_size_pages..=10 * min_memory_size_pages)
                    .unwrap();
                // Make value a multiple of WASM_PAGE_SIZE_BYTES but bigger than min_memory_size
                let value_bytes = (value_pages + 1) * WASM_PAGE_SIZE;
                GearStackEndExportSeed::GenerateValue(value_bytes)
            }
            _ => {
                let correct_value_pages = self.u.int_in_range(0..=min_memory_size_pages).unwrap();
                // Make value a multiple of WASM_PAGE_SIZE_BYTES but less than min_memory_size
                let correct_value_bytes = correct_value_pages * WASM_PAGE_SIZE;
                GearStackEndExportSeed::GenerateValue(correct_value_bytes)
            }
        }
    }

    pub fn gen_mem_export(&mut self, mut module: Module) -> (Module, WasmPageCount) {
        let mut mem_section_idx = None;
        for (idx, section) in module.sections().iter().enumerate() {
            if let Section::Memory(_) = section {
                mem_section_idx = Some(idx);
                break;
            }
        }
        mem_section_idx.map(|index| module.sections_mut().remove(index));

        let mem_size = self.u.int_in_range(0..=self.config.max_mem_size).unwrap();
        let mem_size_upper_bound = if self.config.has_mem_upper_bound.get(self.u) {
            Some(if self.config.upper_bound_can_be_less_then.get(self.u) {
                self.u
                    .int_in_range(0..=mem_size + self.config.max_mem_delta)
                    .unwrap()
            } else {
                self.u
                    .int_in_range(mem_size..=mem_size + self.config.max_mem_delta)
                    .unwrap()
            })
        } else {
            None
        };

        let module = builder::from_module(module)
            .import()
            .module("env")
            .field(MEMORY_FIELD_NAME)
            .external()
            .memory(mem_size, mem_size_upper_bound)
            .build()
            .build();

        let gear_stack_end_seed = self.get_gear_stack_end_seed(mem_size);
        if let GearStackEndExportSeed::GenerateValue(gear_stack_val) = gear_stack_end_seed {
            let mut module = builder::from_module(module)
                .global()
                .value_type()
                .i32()
                .init_expr(Instruction::I32Const(gear_stack_val as i32))
                .build()
                .build();

            let last_element_num = module.global_section_mut().unwrap().entries_mut().len() - 1;

            return (
                builder::from_module(module)
                    .export()
                    .field(STACK_END_EXPORT_NAME)
                    .internal()
                    .global(last_element_num.try_into().unwrap())
                    .build()
                    .build(),
                mem_size.into(),
            );
        }

        (module, mem_size.into())
    }

    fn insert_instructions_in_random_place(
        &mut self,
        mut module: Module,
        instructions: &[Instruction],
    ) -> Module {
        let funcs_num = module.code_section().unwrap().bodies().len();
        let insert_func_no = self.u.int_in_range(0..=funcs_num - 1).unwrap();
        let code = module.code_section_mut().unwrap().bodies_mut()[insert_func_no]
            .code_mut()
            .elements_mut();

        let pos = self.u.int_in_range(0..=code.len() - 1).unwrap();
        code.splice(pos..pos, instructions.iter().cloned());
        module
    }

    pub fn gen_export_func_which_call_func_no(
        &mut self,
        mut module: Module,
        name: &str,
        func_no: u32,
    ) -> Module {
        let funcs_len = module
            .function_section()
            .map_or(0, |funcs| funcs.entries().len() as u32);
        let func_type = get_func_type(&module, FuncIdx::Func(func_no));

        let mut instructions =
            make_call_instructions_vec(self.u, func_type.params(), func_type.results(), func_no);
        instructions.push(Instruction::End);

        module = builder::from_module(module)
            .function()
            .body()
            .with_instructions(Instructions::new(instructions))
            .build()
            .signature()
            .build()
            .build()
            .export()
            .field(name)
            .internal()
            .func(funcs_len)
            .build()
            .build();

        let init_function_no = module.function_section().unwrap().entries().len() as u32 - 1;
        self.calls_indexes.push(FuncIdx::Func(init_function_no));

        module
    }

    pub fn gen_handle(&mut self, module: Module) -> (Module, bool) {
        if self.config.skip_handle.get(self.u) {
            return (module, false);
        }

        let funcs_len = module
            .function_section()
            .map_or(0, |funcs| funcs.entries().len() as u32);

        if funcs_len == 0 {
            return (module, false);
        }

        let func_no = self.u.int_in_range(0..=funcs_len - 1).unwrap();
        (
            self.gen_export_func_which_call_func_no(module, "handle", func_no),
            true,
        )
    }

    pub fn gen_handle_reply(&mut self, module: Module) -> (Module, bool) {
        if self.config.skip_handle_reply.get(self.u) {
            return (module, false);
        }

        let funcs_len = module
            .function_section()
            .map_or(0, |funcs| funcs.entries().len() as u32);

        if funcs_len == 0 {
            return (module, false);
        }

        let func_no = self.u.int_in_range(0..=funcs_len - 1).unwrap();
        (
            self.gen_export_func_which_call_func_no(module, "handle_reply", func_no),
            true,
        )
    }

    pub fn gen_init(&mut self, module: Module) -> (Module, bool) {
        if self.config.skip_init.get(self.u) {
            return (module, false);
        }

        let funcs_len = module
            .function_section()
            .map_or(0, |funcs| funcs.entries().len() as u32);

        if funcs_len == 0 && self.config.skip_init_when_no_funcs.get(self.u) {
            return (module, false);
        }

        if funcs_len == 0 {
            self.calls_indexes.push(FuncIdx::Func(funcs_len));
            return (
                builder::from_module(module)
                    .function()
                    .signature()
                    .build()
                    .build()
                    .export()
                    .field("init")
                    .internal()
                    .func(funcs_len)
                    .build()
                    .build(),
                true,
            );
        }

        let func_no = self.u.int_in_range(0..=funcs_len - 1).unwrap();

        if self.config.init_export_is_any_func.get(self.u) {
            return (
                builder::from_module(module)
                    .export()
                    .field("init")
                    .internal()
                    .func(func_no)
                    .build()
                    .build(),
                true,
            );
        }

        (
            self.gen_export_func_which_call_func_no(module, "init", func_no),
            true,
        )
    }

    pub fn insert_sys_calls(
        &mut self,
        builder: ModuleBuilderWithData,
        memory_pages: WasmPageCount,
    ) -> ModuleWithDebug {
        if builder.code_size == 0 {
            return builder.into();
        }

        let ModuleBuilderWithData {
            module_builder: mut builder,
            offsets,
            last_offset,
            import_count,
            code_size,
        } = builder;

        let mut source_call_index = None;
        let mut debug_call_index = None;

        // generate corresponding import entries for syscalls
        let mut syscall_data = BTreeMap::default();
        let sys_calls_table = sys_calls_table(&self.config);
        for (i, (name, info, sys_call_amount)) in sys_calls_table
            .into_iter()
            .filter_map(|(name, info)| {
                let sys_call_max_amount = info.frequency.mult(code_size);
                let sys_call_amount = self.u.int_in_range(0..=sys_call_max_amount).unwrap();
                if sys_call_amount == 0
                    && !(name == SysCallName::Debug && self.config.print_test_info.is_some())
                {
                    None
                } else {
                    Some((name, info, sys_call_amount))
                }
            })
            .enumerate()
        {
            let signature_index = {
                let func_type = info.func_type();
                let mut signature_builder = builder::signature();
                for parameter in func_type.params() {
                    signature_builder = signature_builder.with_param(*parameter);
                }

                for result in func_type.results() {
                    signature_builder = signature_builder.with_result(*result);
                }

                builder.push_signature(signature_builder.build_sig())
            };

            // make import
            builder.push_import(
                builder::import()
                    .module("env")
                    .external()
                    .func(signature_index)
                    .field(name.to_str())
                    .build(),
            );

            let call_index = self.calls_indexes.len() as u32;
            if name == SysCallName::Source {
                source_call_index = Some(call_index);
            }

            if name == SysCallName::Debug {
                debug_call_index = Some(call_index);
            }

            self.calls_indexes
                .push(FuncIdx::Import((import_count + i) as u32));
            syscall_data.insert(
                name,
                SyscallData {
                    info,
                    sys_call_amount,
                    call_index,
                },
            );
        }

        let mut module = builder.build();
        let mut offsets = offsets.into_iter().cycle();

        // generate call instructions for syscalls and insert them somewhere into the code
        for (name, data) in syscall_data {
            let instructions = self.build_call_instructions(
                name,
                &data,
                memory_pages,
                source_call_index,
                &mut offsets,
            );
            for _ in 0..data.sys_call_amount {
                module = self.insert_instructions_in_random_place(module, &instructions);
            }
        }

        (module, debug_call_index, last_offset).into()
    }

    fn build_call_instructions<I: Clone + Iterator<Item = u32>>(
        &mut self,
        name: SysCallName,
        data: &SyscallData,
        memory_pages: WasmPageCount,
        source_call_index: Option<u32>,
        offsets: &mut Cycle<I>,
    ) -> Vec<Instruction> {
        let info = &data.info;
        // TODO #2206: send also using reserved gas
        if ![
            SysCallName::Send,
            SysCallName::SendWGas,
            SysCallName::SendInput,
            SysCallName::SendInputWGas,
        ]
        .contains(&name)
        {
            return build_checked_call(
                self.u,
                &info.results,
                &info.parameter_rules,
                data.call_index,
                memory_pages,
                self.config.unchecked_memory_access,
            );
        }

        let mut remaining_instructions = build_checked_call(
            self.u,
            &info.results,
            &info.parameter_rules[1..],
            data.call_index,
            memory_pages,
            self.config.unchecked_memory_access,
        );

        if let Some(source_call_index) = source_call_index {
            if self.config.use_message_source.get(self.u) {
                let mut instructions = Vec::with_capacity(3 + remaining_instructions.len());

                let memory_size = memory_pages.memory_size();
                let upper_limit = memory_size.saturating_sub(MEMORY_VALUE_SIZE);
                let offset = self
                    .u
                    .int_in_range(0..=upper_limit)
                    .expect("build_call_instructions: Unstructured::int_in_range failed");

                // call msg::source (gr_source) with a memory offset
                instructions.push(Instruction::I32Const(offset as i32));
                instructions.push(Instruction::Call(source_call_index));
                // pass the offset as the first argument to the send-call
                instructions.push(Instruction::I32Const(offset as i32));
                instructions.append(&mut remaining_instructions);

                return instructions;
            }
        }

        let address_offset = offsets.next().unwrap_or_else(|| {
            self.u
                .arbitrary()
                .expect("build_call_instructions: Unstructured::arbitrary failed")
        }) as i32;
        let mut instructions = Vec::with_capacity(1 + remaining_instructions.len());
        instructions.push(Instruction::I32Const(address_offset));
        instructions.append(&mut remaining_instructions);

        instructions
    }

    pub fn make_print_test_info(&mut self, result: ModuleWithDebug) -> Module {
        let Some(text) = &self.config.print_test_info else {
            return result.module;
        };

        let ModuleWithDebug {
            mut module,
            debug_syscall_index,
            last_offset,
        } = result;

        if let External::Memory(mem_type) = module
            .import_section()
            .unwrap()
            .entries()
            .iter()
            .find(|section| section.field() == MEMORY_FIELD_NAME)
            .unwrap()
            .external()
        {
            if mem_type.limits().initial() == 0 {
                return module;
            }
        }

        let mut init_func_no = None;
        if let Some(export_section) = module.export_section() {
            for export in export_section.entries().iter() {
                if export.field() == "init" {
                    init_func_no = if let Internal::Function(func_no) = export.internal() {
                        Some(*func_no)
                    } else {
                        panic!("init export is not a func, very strange -_-");
                    }
                }
            }
        }
        if init_func_no.is_none() {
            return module;
        }

        let bytes = text.as_bytes();
        module = builder::from_module(module)
            .data()
            .offset(Instruction::I32Const(last_offset as i32))
            .value(bytes.to_vec())
            .build()
            .build();

        let init_code = module.code_section_mut().unwrap().bodies_mut()
            [init_func_no.unwrap() as usize]
            .code_mut()
            .elements_mut();
        let print_code = [
            Instruction::I32Const(last_offset as i32),
            Instruction::I32Const(bytes.len() as i32),
            Instruction::Call(debug_syscall_index.expect("debug data specified so do the call")),
        ];

        init_code.splice(0..0, print_code);

        module
    }

    pub fn resolves_calls_indexes(self, mut module: Module) -> Module {
        if module.code_section().is_none() {
            return module;
        }

        let Self {
            calls_indexes,
            u,
            config,
        } = self;

        let import_funcs_num = module
            .import_section()
            .map(|imp| imp.functions() as u32)
            .unwrap_or(0);

        for instr in module
            .code_section_mut()
            .unwrap()
            .bodies_mut()
            .iter_mut()
            .flat_map(|body| body.code_mut().elements_mut().iter_mut())
        {
            if let Instruction::Call(func_no) = instr {
                let index = calls_indexes[*func_no as usize];
                match index {
                    FuncIdx::Func(no) => *func_no = no + import_funcs_num,
                    FuncIdx::Import(no) => *func_no = no,
                }
            }
        }

        let mut empty_export_section = Default::default();
        for func_no in module
            .export_section_mut()
            .unwrap_or(&mut empty_export_section)
            .entries_mut()
            .iter_mut()
            .filter_map(|export| {
                if let Internal::Function(func_no) = export.internal_mut() {
                    Some(func_no)
                } else {
                    None
                }
            })
        {
            if let FuncIdx::Func(code_func_no) = calls_indexes[*func_no as usize] {
                *func_no = import_funcs_num + code_func_no;
            } else {
                // TODO: export can be to the import function by WASM specification,
                // but we currently do not support this in wasm-gen.
                panic!("Export cannot be to the import function");
            }
        }

        match config.remove_recursion.get(u) {
            true => utils::remove_recursion(module),
            false => module,
        }
    }
}

pub fn gen_gear_program_module<'a>(
    u: &'a mut Unstructured<'a>,
    config: GearConfig,
    addresses: &[HashWithValue],
) -> Module {
    let swarm_config = default_swarm_config(u, &config);

    let module = loop {
        let module = gen_wasm_smith_module(u, &swarm_config);
        let wasm_bytes = module.to_bytes();
        let module: Module = parity_wasm::deserialize_buffer(&wasm_bytes).unwrap();
        if module.function_section().is_some() || config.process_when_no_funcs.get(u) {
            break module;
        }
    };

    let mut gen = WasmGen::new(&module, u, config);

    let (module, memory_pages) = gen.gen_mem_export(module);
    let (module, has_init) = gen.gen_init(module);
    if !has_init {
        return gen.resolves_calls_indexes(module);
    }

    let (module, _has_handle) = gen.gen_handle(module);
    let (module, _has_handle_reply) = gen.gen_handle_reply(module);

    let builder = ModuleBuilderWithData::new(addresses, module, memory_pages);
    let module = gen.insert_sys_calls(builder, memory_pages);
    let module = gen.make_print_test_info(module);

    gen.resolves_calls_indexes(module)
}

pub fn gen_gear_program_code<'a>(
    u: &'a mut Unstructured<'a>,
    config: GearConfig,
    addresses: &[HashWithValue],
) -> Vec<u8> {
    let module = gen_gear_program_module(u, config, addresses);
    parity_wasm::serialize(module).unwrap()
}
