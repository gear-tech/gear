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
use derive_more::{Display, Error};
use gear_wasm_instrument::parity_wasm::{
    builder,
    elements::{External, Instruction, Instructions, Module},
};

use crate::OS_PAGE_SIZE;

#[derive(Debug, Display, Error)]
pub enum InjectMemoryAccessesError {
    #[display(fmt = "No memory imports found")]
    NoMemoryImports,
    #[display(fmt = "No code section found")]
    NoCodeSection,
}

// TODO: different word size accesses
enum MemoryAccess {
    ReadI32,
    WriteI32,
    ReadWriteI32,
}

pub struct InjectMemoryAccesses<'a> {
    unstructured: Unstructured<'a>,
}

impl InjectMemoryAccesses<'_> {
    pub fn new(unstructured: Unstructured<'_>) -> InjectMemoryAccesses<'_> {
        InjectMemoryAccesses { unstructured }
    }

    fn generate_access_body(
        u: &mut Unstructured,
        target_addr: usize,
    ) -> Result<Vec<Instruction>, ()> {
        use MemoryAccess::*;
        let mut body = match u.choose(&[ReadI32, WriteI32, ReadWriteI32]).unwrap() {
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
            ReadWriteI32 => vec![
                Instruction::I32Const(target_addr as i32),
                Instruction::I32Const(target_addr as i32),
                Instruction::I32Load(0, 0),
                Instruction::I32Const(42),
                Instruction::I32Add,
                Instruction::I32Store(0, 0),
            ],
        };

        body.push(Instruction::End);
        Ok(body)
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

        let mut next_func_index = module.functions_space() as u32;
        let mut functions_instr = Vec::new();

        let code_section = module
            .code_section_mut()
            .ok_or(InjectMemoryAccessesError::NoCodeSection)?;

        // NOTE: ATM insert one access per function.
        for function in code_section.bodies_mut() {
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

            function
                .code_mut()
                .elements_mut()
                .insert(insert_at_pos, Instruction::Call(next_func_index));
            next_func_index += 1;

            let instructions =
                Self::generate_access_body(&mut self.unstructured, target_addr).unwrap();
            functions_instr.push(instructions)
        }

        let mut mbuilder = builder::from_module(module);

        for instructions in functions_instr.into_iter() {
            mbuilder.push_function(
                builder::function()
                    .signature()
                    .build()
                    .body()
                    .with_instructions(Instructions::new(instructions))
                    .build()
                    .build(),
            );
        }

        Ok((mbuilder.build(), self.unstructured))
    }
}
