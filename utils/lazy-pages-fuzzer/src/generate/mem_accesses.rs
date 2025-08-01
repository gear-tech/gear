// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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

use crate::OS_PAGE_SIZE;
use arbitrary::Unstructured;
use derive_more::{Display, Error, From};
use gear_wasm_instrument::{Instruction, MemArg, Module, TypeRef};

#[derive(Debug, Clone)]
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
    #[display("No memory imports found")]
    NoMemoryImports,
    #[display("No code section found")]
    NoCodeSection,
    #[display("")]
    Arbitrary(arbitrary::Error),
}

// TODO: different word size accesses (#4042)
enum MemoryAccess {
    ReadI32,
    WriteI32,
}

pub struct InjectMemoryAccesses<'u> {
    unstructured: Unstructured<'u>,
    config: InjectMemoryAccessesConfig,
}

impl<'u> InjectMemoryAccesses<'u> {
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
        // Dummy value to write to memory
        const DUMMY_VALUE: u32 = 0xB6B6B6B6;

        Ok(match u.choose(&[ReadI32, WriteI32])? {
            ReadI32 => vec![
                Instruction::I32Const(target_addr as i32),
                Instruction::I32Load(MemArg::zero()),
                Instruction::Drop,
            ],
            WriteI32 => vec![
                Instruction::I32Const(target_addr as i32),
                Instruction::I32Const(DUMMY_VALUE as i32),
                Instruction::I32Store(MemArg::zero()),
            ],
        })
    }

    pub fn inject(
        mut self,
        mut module: Module,
    ) -> Result<(Module, Unstructured<'u>), InjectMemoryAccessesError> {
        let import_section = module
            .import_section
            .as_ref()
            .ok_or(InjectMemoryAccessesError::NoMemoryImports)?;
        let initial_memory_limit = import_section
            .iter()
            .filter_map(|import| {
                if let TypeRef::Memory(import) = import.ty {
                    Some(import.initial)
                } else {
                    None
                }
            })
            .next()
            .ok_or(InjectMemoryAccessesError::NoMemoryImports)?;

        let code_section = module
            .code_section
            .as_mut()
            .ok_or(InjectMemoryAccessesError::NoCodeSection)?;

        for function in code_section {
            let access_count = self
                .unstructured
                .int_in_range(1..=self.config.max_accesses_per_func)?;

            for _ in 0..=access_count {
                let target_addr = self
                    .unstructured
                    .choose_index(initial_memory_limit as usize)?
                    .saturating_mul(OS_PAGE_SIZE);

                let code_len = function.instructions.len();
                let insert_at_pos = self
                    .unstructured
                    .choose_index(code_len)
                    .ok()
                    .unwrap_or_default();

                let instrs =
                    Self::generate_access_instructions(&mut self.unstructured, target_addr)?;

                for instr in instrs.into_iter().rev() {
                    function.instructions.insert(insert_at_pos, instr);
                }
            }
        }

        Ok((module, self.unstructured))
    }
}

#[cfg(test)]
mod tests {
    use std::hash::{DefaultHasher, Hash, Hasher};

    use crate::MODULE_ENV;

    use super::*;

    const TEST_PROGRAM_WAT: &str = r#"
        (module
            (import "env" "memory" (memory 1))
            (func (export "main") (result i32)
                i32.const 42
            )
        )
    "#;

    fn calculate_slice_hash(slice: &[u8]) -> u64 {
        let mut s = DefaultHasher::new();
        for b in slice {
            b.hash(&mut s);
        }
        s.finish()
    }

    #[test]
    fn test_memory_accesses() {
        let unstructured = Unstructured::new(&[1u8; 32]);
        let config = InjectMemoryAccessesConfig {
            max_accesses_per_func: 10,
        };

        let wasm = wat::parse_str(TEST_PROGRAM_WAT).unwrap();
        let module = Module::new(&wasm).unwrap();

        let (module, _) = InjectMemoryAccesses::new(unstructured, config)
            .inject(module)
            .unwrap();

        let engine = wasmi::Engine::default();
        let mut store = wasmi::Store::new(&engine, ());
        let module = wasmi::Module::new(&engine, &module.serialize().unwrap()).unwrap();

        let ty = wasmi::MemoryType::new(1, None).unwrap();
        let memory = wasmi::Memory::new(&mut store, ty).unwrap();

        let original_mem_hash = {
            let mem_slice = memory.data(&store);
            calculate_slice_hash(mem_slice)
        };

        let mut linker = <wasmi::Linker<()>>::new(&engine);
        linker.define(MODULE_ENV, "memory", memory).unwrap();

        let instance = linker
            .instantiate(&mut store, &module)
            .unwrap()
            .ensure_no_start(&mut store)
            .unwrap();
        let func = instance.get_func(&store, "main").unwrap();

        func.call(&mut store, &[], &mut [wasmi::Val::I32(0)])
            .unwrap();

        let mem_hash = {
            let mem_slice = memory.data(&store);
            calculate_slice_hash(mem_slice)
        };

        assert_ne!(original_mem_hash, mem_hash);
    }
}
