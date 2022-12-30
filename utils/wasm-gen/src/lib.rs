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

use std::ops::RangeInclusive;

use arbitrary::Unstructured;
use gear_wasm_instrument::{
    parity_wasm::{
        self, builder,
        elements::{
            External, FunctionType, Instruction, Instructions, Internal, Module, Section, Type,
            ValueType,
        },
    },
    syscalls::SysCallName,
};
use wasm_smith::{InstructionKind::*, InstructionKinds, Module as ModuleSmith, SwarmConfig};

mod syscalls;
use syscalls::{sys_calls_table, SyscallsConfig};

#[cfg(test)]
mod test;

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
    fn from(p: (u32, u32)) -> Self {
        Self {
            numerator: p.0,
            denominator: p.1,
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
    pub restricted_ratio: Ratio,
}

impl Default for ParamRule {
    fn default() -> Self {
        Self {
            allowed_values: 0..=0,
            restricted_ratio: (100, 100).into(),
        }
    }
}

impl ParamRule {
    pub fn get_i32(&self, u: &mut Unstructured) -> i32 {
        if self.restricted_ratio.get(u) {
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
        if self.restricted_ratio.get(u) {
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
    pub skip_init_when_no_funcs: Ratio,
    pub init_export_is_any_func: Ratio,
    pub max_mem_size: u32,
    pub max_mem_delta: u32,
    pub has_mem_upper_bound: Ratio,
    pub upper_bound_can_be_less_then: Ratio,
    pub sys_call_freq: Ratio,
    pub sys_calls: SyscallsConfig,
    pub print_test_info: Option<String>,
    pub max_percentage_seed: u32,
}

impl GearConfig {
    pub fn new_normal() -> Self {
        let prob = (1, 100).into();
        Self {
            process_when_no_funcs: prob,
            skip_init: (1, 1000).into(),
            skip_handle: prob,
            skip_init_when_no_funcs: prob,
            init_export_is_any_func: prob,
            max_mem_size: 1024,
            max_mem_delta: 1024,
            has_mem_upper_bound: prob,
            upper_bound_can_be_less_then: prob,
            sys_call_freq: (1, 1000).into(),
            sys_calls: Default::default(),
            print_test_info: None,
            max_percentage_seed: 100,
        }
    }
    pub fn new_for_rare_cases() -> Self {
        let prob = (50, 100).into();
        Self {
            skip_init: prob,
            skip_handle: prob,
            skip_init_when_no_funcs: prob,
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
        }
    }
    pub fn new_valid() -> Self {
        let prob = (1, 100).into();
        let zero_prob = (0, 100).into();
        Self {
            process_when_no_funcs: prob,
            skip_init: zero_prob,
            skip_handle: zero_prob,
            skip_init_when_no_funcs: zero_prob,
            init_export_is_any_func: zero_prob,
            max_mem_size: 512,
            max_mem_delta: 256,
            has_mem_upper_bound: prob,
            upper_bound_can_be_less_then: zero_prob,
            sys_call_freq: (1, 1000).into(),
            sys_calls: Default::default(),
            print_test_info: None,
            max_percentage_seed: 100,
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

fn make_call_instructions_vec(
    u: &mut Unstructured,
    params: &[ValueType],
    results: &[ValueType],
    params_rules: &[ParamRule],
    func_no: u32,
) -> Vec<Instruction> {
    let mut code = Vec::new();
    for (index, val) in params.iter().enumerate() {
        let rule = params_rules.get(index).cloned().unwrap_or_default();
        let instr = match val {
            ValueType::I32 => Instruction::I32Const(rule.get_i32(u)),
            ValueType::I64 => Instruction::I64Const(rule.get_i64(u)),
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
    // ~1% of cases with invalid stask size that is biger than import memory
    // ~1% of cases stack size is not generated at all
    // all other cases should be valid
    fn get_gear_stack_end_seed(&mut self, min_memory_size_pages: u32) -> GearStackEndExportSeed {
        const NOT_GENERATE_SEED: u32 = 0;
        const NOT_WASM_PAGE_SEED: u32 = 1;
        const BIGGER_THAN_MEMORY_SEED: u32 = 2;

        const WASM_PAGE_SIZE_BYTES: u32 = 64 * 1024;

        let seed = self
            .u
            .int_in_range(0..=self.config.max_percentage_seed)
            .unwrap();
        match seed {
            NOT_GENERATE_SEED => GearStackEndExportSeed::NotGenerate,
            NOT_WASM_PAGE_SEED => {
                let max_size = min_memory_size_pages * WASM_PAGE_SIZE_BYTES;
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
                let value_bytes = (value_pages + 1) * WASM_PAGE_SIZE_BYTES;
                GearStackEndExportSeed::GenerateValue(value_bytes)
            }
            _ => {
                let correct_value_pages = self.u.int_in_range(0..=min_memory_size_pages).unwrap();
                // Make value a multiple of WASM_PAGE_SIZE_BYTES but less than min_memory_size
                let correct_value_bytes = correct_value_pages * WASM_PAGE_SIZE_BYTES;
                GearStackEndExportSeed::GenerateValue(correct_value_bytes)
            }
        }
    }

    pub fn gen_mem_export(&mut self, mut module: Module) -> Module {
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
            .field("memory")
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

            return builder::from_module(module)
                .export()
                .field("__gear_stack_end")
                .internal()
                .global(last_element_num.try_into().unwrap())
                .build()
                .build();
        }
        module
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
        let mut instructions = make_call_instructions_vec(
            self.u,
            func_type.params(),
            func_type.results(),
            Default::default(),
            func_no,
        );
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

    pub fn insert_sys_calls(&mut self, mut module: Module) -> Module {
        let code_size = if let Some(code) = module.code_section() {
            code.bodies()
                .iter()
                .fold(0, |sum, body| sum + body.code().elements().len())
        } else {
            return module;
        };

        let sys_calls_table = sys_calls_table(&self.config);

        for (name, info) in sys_calls_table {
            let sys_call_max_amount = info.frequency.mult(code_size);
            let sys_call_amount = self.u.int_in_range(0..=sys_call_max_amount).unwrap();
            if sys_call_amount == 0
                && !(name == SysCallName::Debug && self.config.print_test_info.is_some())
            {
                continue;
            }

            let types = module.type_section_mut().unwrap().types_mut();
            let type_no = types.len() as u32;
            types.push(Type::Function(info.func_type()));

            // make import
            module = builder::from_module(module)
                .import()
                .module("env")
                .external()
                .func(type_no)
                .field(name.to_str())
                .build()
                .build();

            let import_func_no = module.import_section().unwrap().functions() as u32 - 1;

            self.calls_indexes.push(FuncIdx::Import(import_func_no));

            let func_no = self.calls_indexes.len() as u32 - 1;

            // insert sys call anywhere in the code
            let instructions = make_call_instructions_vec(
                self.u,
                &info.params,
                &info.results,
                &info.param_rules,
                func_no,
            );

            for _ in 0..sys_call_amount {
                module = self.insert_instructions_in_random_place(module, &instructions);
            }
        }

        module
    }

    pub fn make_print_test_info(&mut self, mut module: Module) -> Module {
        let text = if let Some(text) = &self.config.print_test_info {
            text
        } else {
            return module;
        };

        if let External::Memory(mem_type) = module
            .import_section()
            .unwrap()
            .entries()
            .iter()
            .find(|section| section.field() == "memory")
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
            .offset(Instruction::I32Const(0))
            .value(bytes.to_vec())
            .build()
            .build();

        let gr_debug_import_no = module
            .import_section()
            .unwrap()
            .entries()
            .iter()
            .position(|import| import.field() == "gr_debug")
            .unwrap() as u32;
        let gr_debug_call_no = self
            .calls_indexes
            .iter()
            .position(|func_idx| {
                if let FuncIdx::Import(import_no) = func_idx {
                    *import_no == gr_debug_import_no - 1 // TODO: first is memory import, so need to do `- 1`.
                                                         // Make more common solution.
                } else {
                    false
                }
            })
            .unwrap() as u32;

        let init_code = module.code_section_mut().unwrap().bodies_mut()
            [init_func_no.unwrap() as usize]
            .code_mut()
            .elements_mut();
        let print_code = [
            Instruction::I32Const(0),
            Instruction::I32Const(bytes.len() as i32),
            Instruction::Call(gr_debug_call_no),
        ];

        init_code.splice(0..0, print_code);

        module
    }

    pub fn resolves_calls_indexes(self, mut module: Module) -> Module {
        if module.code_section().is_none() {
            return module;
        }

        let Self { calls_indexes, .. } = self;

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

        module
    }
}

pub fn gen_gear_program_module<'a>(u: &'a mut Unstructured<'a>, config: GearConfig) -> Module {
    let swarm_config = default_swarm_config(u, &config);

    let mut module = loop {
        let module = gen_wasm_smith_module(u, &swarm_config);
        let wasm_bytes = module.to_bytes();
        let module: Module = parity_wasm::deserialize_buffer(&wasm_bytes).unwrap();
        if module.function_section().is_some() || config.process_when_no_funcs.get(u) {
            break module;
        }
    };

    let mut gen = WasmGen::new(&module, u, config);
    module = gen.gen_mem_export(module);
    let (mut module, has_init) = gen.gen_init(module);
    if !has_init {
        return gen.resolves_calls_indexes(module);
    }
    module = gen.gen_handle(module).0;
    module = gen.insert_sys_calls(module);
    module = gen.make_print_test_info(module);

    gen.resolves_calls_indexes(module)
}

pub fn gen_gear_program_code<'a>(u: &'a mut Unstructured<'a>, config: GearConfig) -> Vec<u8> {
    let module = gen_gear_program_module(u, config);
    parity_wasm::serialize(module).unwrap()
}
