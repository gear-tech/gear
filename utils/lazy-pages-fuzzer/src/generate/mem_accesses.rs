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
    #[display(fmt = "No memory imports found")]
    NoMemoryImports,
    #[display(fmt = "No code section found")]
    NoCodeSection,
    #[display(fmt = "")]
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
                Instruction::I32Load(0, 0),
                Instruction::Drop,
            ],
            WriteI32 => vec![
                Instruction::I32Const(target_addr as i32),
                Instruction::I32Const(DUMMY_VALUE as i32),
                Instruction::I32Store(0, 0),
            ],
        })
    }

    pub fn inject(
        mut self,
        mut module: Module,
    ) -> Result<(Module, Unstructured<'u>), InjectMemoryAccessesError> {
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
                    .choose_index(initial_memory_limit as usize)?
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

#[cfg(test)]
mod tests {
    use std::hash::{DefaultHasher, Hash, Hasher};

    use super::*;

    const TEST_PROGRAM_WAT: &str = r#"
        (module
            (import "env" "memory" (memory 1))
            (func (export "main") (result i32)
                i32.const 42
            )
        )
    "#;

    struct Resolver {
        memory: sandbox_wasmi::MemoryRef,
    }

    impl sandbox_wasmi::ModuleImportResolver for Resolver {
        fn resolve_memory(
            &self,
            _field_name: &str,
            _memory_type: &sandbox_wasmi::MemoryDescriptor,
        ) -> Result<sandbox_wasmi::MemoryRef, sandbox_wasmi::Error> {
            Ok(self.memory.clone())
        }
    }

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
        let module = Module::from_bytes(wasm).unwrap();

        let (module, _) = InjectMemoryAccesses::new(unstructured, config)
            .inject(module)
            .unwrap();

        let memory =
            sandbox_wasmi::MemoryInstance::alloc(sandbox_wasmi::memory_units::Pages(1), None)
                .unwrap();

        let original_mem_hash = {
            let mem_slice = memory.direct_access();
            calculate_slice_hash(mem_slice.as_ref())
        };

        let resolver = Resolver { memory };
        let imports = sandbox_wasmi::ImportsBuilder::new().with_resolver("env", &resolver);

        let module = sandbox_wasmi::Module::from_buffer(module.into_bytes().unwrap()).unwrap();
        let instance = sandbox_wasmi::ModuleInstance::new(&module, &imports)
            .unwrap()
            .assert_no_start();
        let _ = instance
            .invoke_export("main", &[], &mut sandbox_wasmi::NopExternals)
            .unwrap();

        let mem_hash = {
            let mem_slice = resolver.memory.direct_access();
            calculate_slice_hash(mem_slice.as_ref())
        };

        assert_ne!(original_mem_hash, mem_hash);
    }
}
