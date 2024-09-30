// This file is part of Gear.

// Copyright (C) 2021-2024 Gear Technologies Inc.
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

use anyhow::Result;
use arbitrary::{Arbitrary, Unstructured};

use gear_wasm_instrument::parity_wasm::{
    builder,
    elements::{Instruction, Module},
};

pub const GLOBAL_NAME_PREFIX: &str = "gear_fuzz_";
pub const INITIAL_GLOBAL_VALUE: i64 = 0;

#[derive(Debug, Clone)]
pub struct InjectGlobalsConfig {
    pub max_global_number: usize,
    pub max_access_per_func: usize,
}

impl Default for InjectGlobalsConfig {
    fn default() -> Self {
        InjectGlobalsConfig {
            max_global_number: 10,
            max_access_per_func: 10,
        }
    }
}

pub struct InjectGlobals<'u> {
    unstructured: Unstructured<'u>,
    config: InjectGlobalsConfig,
}

impl<'u> InjectGlobals<'u> {
    pub fn new(unstructured: Unstructured<'_>, config: InjectGlobalsConfig) -> InjectGlobals<'_> {
        InjectGlobals {
            unstructured,
            config,
        }
    }

    pub fn inject(mut self, mut module: Module) -> Result<(Module, Unstructured<'u>)> {
        let global_names: Vec<_> = ('a'..='z')
            .take(self.config.max_global_number)
            .map(|ch| format!("{GLOBAL_NAME_PREFIX}{ch}"))
            .collect();

        let mut next_global_idx = module.globals_space() as u32;

        let code_section = module
            .code_section_mut()
            .ok_or_else(|| anyhow::Error::msg("No code section found"))?;

        // Insert global access instructions
        for function in code_section.bodies_mut() {
            let count_per_func = self
                .unstructured
                .int_in_range(1..=self.config.max_access_per_func)?;

            for _ in 0..=count_per_func {
                let array_idx = self.unstructured.choose_index(global_names.len())? as u32;
                let global_idx = next_global_idx + array_idx;

                let insert_at_pos = self
                    .unstructured
                    .choose_index(function.code().elements().len())?;
                let is_set = bool::arbitrary(&mut self.unstructured)?;

                let instructions = if is_set {
                    [
                        Instruction::I64Const(self.unstructured.int_in_range(0..=i64::MAX)?),
                        Instruction::SetGlobal(global_idx),
                    ]
                } else {
                    [Instruction::GetGlobal(global_idx), Instruction::Drop]
                };

                for instr in instructions.into_iter().rev() {
                    function
                        .code_mut()
                        .elements_mut()
                        .insert(insert_at_pos, instr.clone());
                }
            }
        }

        // Add global exports
        let mut builder = builder::from_module(module);
        for global in global_names.iter() {
            builder.push_export(
                builder::export()
                    .field(global)
                    .internal()
                    .global(next_global_idx)
                    .build(),
            );
            builder.push_global(
                builder::global()
                    .mutable()
                    .value_type()
                    .i64()
                    .init_expr(Instruction::I64Const(INITIAL_GLOBAL_VALUE))
                    .build(),
            );

            next_global_idx += 1;
        }

        Ok((builder.build(), self.unstructured))
    }
}

#[cfg(test)]
mod tests {
    use gear_wasm_instrument::parity_wasm::elements::Internal;

    use super::*;

    const TEST_PROGRAM_WAT: &str = r#"
        (module
            (func (export "main") (result i32)
                i32.const 42
            )
        )
    "#;

    #[test]
    fn test_inject_globals() {
        let unstructured = Unstructured::new(&[0u8; 32]);
        let config = InjectGlobalsConfig {
            max_global_number: 3,
            max_access_per_func: 3,
        };
        let globals = InjectGlobals::new(unstructured, config);

        let wasm = wat::parse_str(TEST_PROGRAM_WAT).unwrap();
        let module = Module::from_bytes(wasm).unwrap();
        let (module, _) = globals.inject(module).unwrap();

        assert_eq!(module.globals_space(), 3);
        assert_eq!(
            module
                .export_section()
                .unwrap()
                .entries()
                .iter()
                .filter(|export| { matches!(export.internal(), Internal::Global(_)) })
                .count(),
            3
        );
    }

    #[test]
    fn test_globals_modified() {
        // Precomputed value of the global after running the program
        const EXPECTED_GLOBAL_VALUE: i64 = 217020518514230019;

        let unstructured = Unstructured::new(&[3u8; 32]);
        let config = InjectGlobalsConfig {
            max_global_number: 3,
            max_access_per_func: 3,
        };
        let globals = InjectGlobals::new(unstructured, config);

        let wasm = wat::parse_str(TEST_PROGRAM_WAT).unwrap();
        let module = Module::from_bytes(wasm).unwrap();
        let (module, _) = globals.inject(module).unwrap();

        let engine = sandbox_wasmi::Engine::default();
        let mut store = sandbox_wasmi::Store::new(&engine, ());

        let module = sandbox_wasmi::Module::new(&engine, &module.into_bytes().unwrap()).unwrap();
        let instance = sandbox_wasmi::Instance::new(&mut store, &module, &[]).unwrap();

        let gear_fuzz_a = instance
            .get_global(&store, "gear_fuzz_a")
            .unwrap()
            .get(&store)
            .i64()
            .unwrap();
        assert_eq!(gear_fuzz_a, INITIAL_GLOBAL_VALUE);

        let func = instance.get_func(&store, "main").unwrap();
        func.call(&mut store, &[], &mut [sandbox_wasmi::Val::I64(0)])
            .unwrap();

        // Assert that global was modified (initially 0)
        let gear_fuzz_a = instance
            .get_global(&store, "gear_fuzz_a")
            .unwrap()
            .get(&store)
            .i64()
            .unwrap();
        assert_eq!(gear_fuzz_a, EXPECTED_GLOBAL_VALUE);
    }
}
