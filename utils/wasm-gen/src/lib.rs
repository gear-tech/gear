// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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

use arbitrary::Unstructured;
use parity_wasm::{
    builder,
    elements::{Instruction, Instructions, Module as PModule, Section, Type},
};
use wasm_smith::{InstructionKind::*, InstructionKinds, Module, SwarmConfig};

#[cfg(test)]
mod test;

#[derive(Clone, Copy, Debug)]
pub struct Ratio {
    numerator: u32,
    denominator: u32,
}

impl Ratio {
    pub fn get(&self, u: &mut Unstructured) -> bool {
        u.ratio(self.numerator, self.denominator).unwrap()
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

pub struct GearConfig {
    pub process_when_no_funcs: Ratio,
    pub skip_init: Ratio,
    pub skip_init_when_no_funcs: Ratio,
    pub init_export_is_any_func: Ratio,
    pub max_mem_size: u32,
    pub max_mem_delta: u32,
    pub has_mem_upper_bound: Ratio,
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

pub fn gen_wasm_smith_module(u: &mut Unstructured, config: &SwarmConfig) -> Module {
    loop {
        if let Ok(module) = Module::new(config.clone(), u) {
            return module;
        }
    }
}

pub enum GenConfigMod {
    Default,
    Rare,
    Special(GearConfig),
}

fn gen_mem_export(mut module: PModule, u: &mut Unstructured, config: &GearConfig) -> PModule {
    let mut mem_section_idx = None;
    for i in 0..module.sections().len() {
        match module.sections()[i] {
            Section::Memory(_) => {
                mem_section_idx = Some(i);
                break;
            }
            _ => {}
        }
    }
    mem_section_idx.map(|index| module.sections_mut().remove(index));

    let mem_size = u.int_in_range(0..=config.max_mem_size).unwrap();
    let mem_size_upper_bound = if config.has_mem_upper_bound.get(u) {
        Some(u.int_in_range(0..=mem_size + config.max_mem_delta).unwrap())
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

fn gen_init(module: PModule, u: &mut Unstructured, config: &GearConfig) -> (PModule, bool) {
    if config.skip_init.get(u) {
        return (module, false);
    }

    let funcs_len = module
        .function_section()
        .map_or(0, |funcs| funcs.entries().len() as u32);

    if funcs_len == 0 && config.skip_init_when_no_funcs.get(u) {
        return (module, false);
    }

    if funcs_len == 0 {
        let module = builder::from_module(module).function().build().build();
        return (module, true);
    }

    let index = u.int_in_range(0..=funcs_len - 1).unwrap();

    if config.init_export_is_any_func.get(u) {
        let module = builder::from_module(module)
            .export()
            .field("init")
            .internal()
            .func(index)
            .build()
            .build();
        return (module, true);
    }

    let func = module.function_section().unwrap().entries()[index as usize];
    let Type::Function(func_type) =
        &module.type_section().unwrap().types()[func.type_ref() as usize];
    let mut code = Vec::new();
    code.extend(
        func_type
            .params()
            .iter()
            .map(|param_type| match param_type {
                parity_wasm::elements::ValueType::I32 => {
                    Instruction::I32Const(u.arbitrary().unwrap())
                }
                parity_wasm::elements::ValueType::I64 => {
                    Instruction::I64Const(u.arbitrary().unwrap())
                }
                _ => panic!("Cannot handle f32/f64"),
            }),
    );
    code.push(Instruction::Call(index));
    code.extend(func_type.results().iter().map(|_| Instruction::Drop));
    code.push(Instruction::End);

    let module = builder::from_module(module)
        .function()
        .body()
        .with_instructions(Instructions::new(code))
        .build()
        .build()
        .export()
        .field("init")
        .internal()
        .func(funcs_len)
        .build()
        .build();

    return (module, true);
}

pub fn gen_gear_program_module(u: &mut Unstructured, config: GearConfig) -> PModule {
    let swarm_config = default_swarm_config(u, &config);

    let module = loop {
        let module = gen_wasm_smith_module(u, &swarm_config);
        let wasm_bytes = module.to_bytes();
        let module: PModule =
            wasm_instrument::parity_wasm::deserialize_buffer(&wasm_bytes).unwrap();
        if module.function_section().is_some() || config.process_when_no_funcs.get(u) {
            break module;
        }
    };

    let module = gen_mem_export(module, u, &config);

    let (module, _) = gen_init(module, u, &config);

    // println!("funcs num = {}", module.function_section().map(|f| f.entries().len()).unwrap_or(0));

    return module;
}

pub fn gen_gear_program_code(u: &mut Unstructured, config: GearConfig) -> Vec<u8> {
    let module = gen_gear_program_module(u, config);
    parity_wasm::serialize(module).unwrap()
}
