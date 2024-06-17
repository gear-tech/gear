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

pub struct InjectGlobals<'a> {
    unstructured: Unstructured<'a>,
    config: InjectGlobalsConfig,
}

impl InjectGlobals<'_> {
    pub fn new(unstructured: Unstructured<'_>, config: InjectGlobalsConfig) -> InjectGlobals<'_> {
        InjectGlobals {
            unstructured,
            config,
        }
    }

    pub fn inject<'this>(mut self, mut module: Module) -> Result<(Module, Unstructured<'this>)>
    where
        Self: 'this,
    {
        let globals: Vec<_> = ('a'..='z')
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
            for _ in 0..count_per_func {
                let array_idx = self.unstructured.choose_index(globals.len())? as u32;
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
        for global in globals.iter() {
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
                    .init_expr(Instruction::I64Const(0))
                    .build(),
            );

            next_global_idx += 1;
        }

        Ok((builder.build(), self.unstructured))
    }
}
