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

use arbitrary::Unstructured;
use derive_more::{Display, Error, From};
use gear_wasm_instrument::parity_wasm::elements::{External, Instruction, Module};

use crate::OS_PAGE_SIZE;

pub struct InjectMemoryAccessesConfig {
    pub max_accesses_per_func: usize,
}

impl Default for InjectMemoryAccessesConfig {
    fn default() -> Self {
        InjectMemoryAccessesConfig {
            max_accesses_per_func: 10,
        }
    }
}

#[derive(Debug, Display, Error, From)]
pub enum InjectMemoryAccessesError {
    #[display(fmt = "No memory imports found")]
    NoMemoryImports,
    #[display(fmt = "No code section found")]
    NoCodeSection,
    #[display(fmt = "")]
    Arbitrary(arbitrary::Error),
}

// TODO: different word size accesses
enum MemoryAccess {
    ReadI32,
    WriteI32,
}

pub struct InjectMemoryAccesses<'u> {
    unstructured: Unstructured<'u>,
    config: InjectMemoryAccessesConfig,
}

impl InjectMemoryAccesses<'_> {
    pub fn new(
        unstructured: Unstructured<'_>,
        config: InjectMemoryAccessesConfig,
    ) -> InjectMemoryAccesses<'_> {
        InjectMemoryAccesses {
            unstructured,
            config,
        }
    }

    fn generate_access_instructions(
        u: &mut Unstructured,
        target_addr: usize,
    ) -> Result<Vec<Instruction>, InjectMemoryAccessesError> {
        use MemoryAccess::*;
        Ok(match u.choose(&[ReadI32, WriteI32])? {
            ReadI32 => vec![
                Instruction::I32Const(target_addr as i32),
                Instruction::I32Load(0, 0),
                Instruction::Drop,
            ],
            WriteI32 => vec![
                Instruction::I32Const(target_addr as i32),
                Instruction::I32Const(0xB6B6B6B6u32 as i32),
                Instruction::I32Store(0, 0),
            ],
        })
    }

    pub fn inject<'this>(
        mut self,
        mut module: Module,
    ) -> Result<(Module, Unstructured<'this>), InjectMemoryAccessesError>
    where
        Self: 'this,
    {
        let import_section = module
            .import_section()
            .ok_or(InjectMemoryAccessesError::NoMemoryImports)?;
        let initial_memory_limit = import_section
            .entries()
            .iter()
            .filter_map(|import| {
                if let External::Memory(import) = import.external() {
                    Some(import.limits().initial())
                } else {
                    None
                }
            })
            .next()
            .ok_or(InjectMemoryAccessesError::NoMemoryImports)?;

        let code_section = module
            .code_section_mut()
            .ok_or(InjectMemoryAccessesError::NoCodeSection)?;

        for function in code_section.bodies_mut() {
            let access_count = self
                .unstructured
                .int_in_range(1..=self.config.max_accesses_per_func)?;

            for _ in 0..=access_count {
                let target_addr = self
                    .unstructured
                    .choose_index(initial_memory_limit as usize)
                    .unwrap()
                    .saturating_mul(OS_PAGE_SIZE);

                let code_len = function.code().elements().len();
                let insert_at_pos = self
                    .unstructured
                    .choose_index(code_len)
                    .ok()
                    .unwrap_or_default();

                let instrs =
                    Self::generate_access_instructions(&mut self.unstructured, target_addr)?;

                for instr in instrs.into_iter().rev() {
                    function
                        .code_mut()
                        .elements_mut()
                        .insert(insert_at_pos, instr);
                }
            }
        }

        Ok((module, self.unstructured))
    }
}
