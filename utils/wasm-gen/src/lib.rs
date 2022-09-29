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
use parity_wasm::{
    builder,
    elements::{
        External, FunctionType, Instruction, Instructions, Internal, Module, Section, Type,
        ValueType,
    },
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
    // pub param_type: ValueType,
    pub allowed_values: RangeInclusive<i64>,
    pub restricted_ratio: Ratio,
}

#[derive(Clone)]
pub struct GearConfig {
    pub process_when_no_funcs: Ratio,
    pub skip_init: Ratio,
    pub skip_init_when_no_funcs: Ratio,
    pub init_export_is_any_func: Ratio,
    pub max_mem_size: u32,
    pub max_mem_delta: u32,
    pub has_mem_upper_bound: Ratio,
    pub upper_bound_can_be_less_then: Ratio,
    pub sys_call_freq: Ratio,
    pub sys_calls: SyscallsConfig,
}

impl Default for GearConfig {
    fn default() -> Self {
        let prob = (1, 100).into();
        Self {
            process_when_no_funcs: prob,
            skip_init: prob,
            skip_init_when_no_funcs: prob,
            init_export_is_any_func: prob,
            max_mem_size: 1024,
            max_mem_delta: 1024,
            has_mem_upper_bound: prob,
            upper_bound_can_be_less_then: prob,
            sys_call_freq: prob,
            sys_calls: Default::default(),
        }
    }
}

impl GearConfig {
    pub fn new_for_rare_cases() -> Self {
        let prob = (50, 100).into();
        Self {
            skip_init: prob,
            skip_init_when_no_funcs: prob,
            process_when_no_funcs: prob,
            init_export_is_any_func: prob,
            max_mem_size: 1024,
            max_mem_delta: 1024,
            has_mem_upper_bound: prob,
            upper_bound_can_be_less_then: prob,
            sys_call_freq: (1, 100).into(),
            sys_calls: Default::default(),
        }
    }
    pub fn new_valid() -> Self {
        let prob = (1, 100).into();
        Self {
            process_when_no_funcs: prob,
            skip_init: (0, 100).into(),
            skip_init_when_no_funcs: (0, 100).into(),
            init_export_is_any_func: (0, 100).into(),
            max_mem_size: 512,
            max_mem_delta: 256,
            has_mem_upper_bound: prob,
            upper_bound_can_be_less_then: (0, 100).into(),
            sys_call_freq: prob,
            sys_calls: Default::default(),
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

    cfg.max_instructions = 10000000;
    cfg.max_memory_pages = gear_config.max_mem_size as u64;
    cfg.max_funcs = 1000;

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
    let gen_const_instr = |u: &mut Unstructured, param| match param {
        ValueType::I32 => Instruction::I32Const(u.arbitrary().unwrap()),
        ValueType::I64 => Instruction::I64Const(u.arbitrary().unwrap()),
        _ => panic!("Cannot handle f32/f64"),
    };

    let mut code = Vec::new();
    for (index, val) in params.iter().enumerate() {
        let instr = if let Some(rule) = params_rules.get(index) {
            if rule.restricted_ratio.get(u) {
                gen_const_instr(u, *val)
            } else {
                let c = u.int_in_range(rule.allowed_values.clone()).unwrap();
                match val {
                    ValueType::I32 => Instruction::I32Const(c as i32),
                    ValueType::I64 => Instruction::I64Const(c),
                    _ => panic!("Cannot handle f32/f64"),
                }
            }
        } else {
            gen_const_instr(u, *val)
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

        builder::from_module(module)
            .import()
            .module("env")
            .field("memory")
            .external()
            .memory(mem_size, mem_size_upper_bound)
            .build()
            .build()
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

    pub fn gen_init(&mut self, mut module: Module) -> (Module, bool) {
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
            .field("init")
            .internal()
            .func(funcs_len)
            .build()
            .build();

        let init_function_no = module.function_section().unwrap().entries().len() as u32 - 1;
        self.calls_indexes.push(FuncIdx::Func(init_function_no));

        (module, true)
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
            let sys_call_max_number = info.frequency.mult(code_size);
            let sys_call_number = self.u.int_in_range(0..=sys_call_max_number).unwrap();
            if sys_call_number == 0 {
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
                .field(name)
                .build()
                .build();

            let import_func_no = module.import_section().unwrap().functions() as u32 - 1;

            self.calls_indexes.push(FuncIdx::Import(import_func_no));

            let func_no = self.calls_indexes.len() as u32 - 1;

            // insert sys call any where in the code
            let instructions = make_call_instructions_vec(
                self.u,
                &info.params,
                &info.results,
                &info.param_rules,
                func_no,
            );
            module = self.insert_instructions_in_random_place(module, &instructions);
        }

        module
    }

    pub fn resolves_calls_indexes(self, mut module: Module) -> Module {
        if module.code_section().is_none() {
            return module;
        }

        let Self { calls_indexes, .. } = self;
        // println!("{calls_indexes:?}");

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
            // println!("func_no = {func_no:?}");
            if let FuncIdx::Func(code_func_no) = calls_indexes[*func_no as usize] {
                *func_no = import_funcs_num + code_func_no;
            } else {
                // TODO: check that case
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
        let module: Module = wasm_instrument::parity_wasm::deserialize_buffer(&wasm_bytes).unwrap();
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
    module = gen.insert_sys_calls(module);
    gen.resolves_calls_indexes(module)
}

pub fn gen_gear_program_code<'a>(u: &'a mut Unstructured<'a>, config: GearConfig) -> Vec<u8> {
    let module = gen_gear_program_module(u, config);
    parity_wasm::serialize(module).unwrap()
}
