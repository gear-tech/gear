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

use arbitrary::{Arbitrary, Unstructured};
use gear_wasm_instrument::parity_wasm::{
    builder,
    elements::{External, Instruction, Instructions, Module},
};

use crate::OS_PAGE_SIZE;

#[derive(Debug)]
pub enum InjectMemoryAccessesError {
    NoMemoryImports,
}

pub struct InjectMemoryAccesses<'a> {
    unstructured: Unstructured<'a>,
    module: Module,
}

impl InjectMemoryAccesses<'_> {
    pub fn new(unstructured: Unstructured<'_>, module: Module) -> InjectMemoryAccesses<'_> {
        InjectMemoryAccesses {
            unstructured,
            module,
        }
    }

    pub fn inject(mut self) -> Result<Module, InjectMemoryAccessesError> {
        let import_section = self
            .module
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

        let mut next_func_index = self.module.functions_space() as u32;
        let mut functions_instr = Vec::new();

        let code_section = self.module.code_section_mut().expect("no code section");

        for function in code_section.bodies_mut() {
            let target_addr = self
                .unstructured
                .choose_index(initial_memory_limit as usize)
                .unwrap()
                .saturating_mul(OS_PAGE_SIZE);
            let insert_at_start = bool::arbitrary(&mut self.unstructured).unwrap();

            let code_len = function.code().elements().len();
            function.code_mut().elements_mut().insert(
                if insert_at_start { 0 } else { code_len },
                Instruction::Call(next_func_index),
            );
            next_func_index += 1;

            let instructions = [
                Instruction::I32Const(target_addr as i32),
                Instruction::I32Load(0, 0),
                Instruction::Drop,
            ]
            .to_vec();
            functions_instr.push(instructions)
        }

        let mut mbuilder = builder::from_module(self.module);

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

        Ok(mbuilder.build())
    }
}
