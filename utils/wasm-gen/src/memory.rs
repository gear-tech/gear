// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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

use super::*;
use gear_wasm_instrument::{
    parity_wasm::{
        builder::{self, ModuleBuilder},
        elements::{Instruction, Section},
    },
    STACK_END_EXPORT_NAME,
};
use gsys::HashWithValue;
use std::{mem, slice};

pub struct ModuleBuilderWithData {
    pub module_builder: ModuleBuilder,
    pub offsets: Vec<u32>,
    pub last_offset: u32,
}

impl ModuleBuilderWithData {
    pub fn new(addresses: &[HashWithValue], module: Module, memory_pages: WasmPageCount) -> Self {
        let module_builder = builder::from_module(module);
        if memory_pages == 0.into() {
            return Self {
                module_builder,
                offsets: vec![],
                last_offset: 0,
            };
        };

        let (module_builder, offsets, last_offset) =
            Self::inject_addresses(addresses, module_builder);
        Self {
            module_builder,
            offsets,
            last_offset,
        }
    }

    fn inject_addresses(
        addresses: &[HashWithValue],
        module_builder: ModuleBuilder,
    ) -> (ModuleBuilder, Vec<u32>, u32) {
        let size = mem::size_of::<HashWithValue>();
        addresses.iter().fold(
            (module_builder, Vec::with_capacity(addresses.len()), 0u32),
            |(module_builder, mut offsets, last_offset), address| {
                offsets.push(last_offset);
                let slice = unsafe {
                    slice::from_raw_parts(address as *const HashWithValue as *const u8, size)
                };
                let len = slice.len();
                let module_builder = module_builder
                    .data()
                    .offset(Instruction::I32Const(last_offset as i32))
                    .value(slice.to_vec())
                    .build();

                (module_builder, offsets, last_offset + len as u32)
            },
        )
    }
}

/// Memory import generator.
///
/// The generator is used to insert into wasm module new
/// valid (from gear runtime perspective) memory import definition
/// from the provided config.
pub struct MemoryGenerator {
    config: MemoryPagesConfig,
    module: WasmModule,
}

impl MemoryGenerator {
    pub(crate) const MEMORY_FIELD_NAME: &str = "memory";

    /// Instantiate the memory generator from wasm module and memory pages config.
    pub fn new(module: WasmModule, config: MemoryPagesConfig) -> Self {
        Self { config, module }
    }

    /// Disable current generator.
    pub fn disable(self) -> DisabledMemoryGenerator {
        DisabledMemoryGenerator(self.module)
    }

    /// Generate a new memory section from the config, used to instantiate the generator.
    ///
    /// Returns disabled memory generation and a proof that memory imports generation has happened.
    pub fn generate_memory(mut self) -> (DisabledMemoryGenerator, MemoryImportGenerationProof) {
        self.remove_mem_section();

        let MemoryGenerator {
            mut module,
            config:
                MemoryPagesConfig {
                    initial_size,
                    upper_limit,
                    stack_end,
                },
        } = self;

        // Define memory import in the module
        module.with(|module| {
            let mut module = builder::from_module(module)
                .import()
                .module("env")
                .field(Self::MEMORY_FIELD_NAME)
                .external()
                .memory(initial_size, upper_limit)
                .build()
                .build();

            // Define optional stack-end
            if let Some(stack_end) = stack_end {
                module = builder::from_module(module)
                    .global()
                    .value_type()
                    .i32()
                    .init_expr(Instruction::I32Const(stack_end as i32))
                    .build()
                    .build();

                let stack_end_index = module
                    .global_section()
                    .expect("has at least stack end global")
                    .entries()
                    .len()
                    - 1;

                module = builder::from_module(module)
                    .export()
                    .field(STACK_END_EXPORT_NAME)
                    .internal()
                    .global(stack_end_index as u32)
                    .build()
                    .build();
            }

            (module, ())
        });

        (
            DisabledMemoryGenerator(module),
            MemoryImportGenerationProof(()),
        )
    }

    // Remove current memory section.
    fn remove_mem_section(&mut self) {
        self.module.with(|mut module| {
            // Find memory section index.
            let mem_section_idx = module
                .sections()
                .iter()
                .enumerate()
                .find_map(|(idx, section)| matches!(section, Section::Memory(_)).then_some(idx));

            // Remove it.
            if let Some(mem_section_idx) = mem_section_idx {
                module.sections_mut().remove(mem_section_idx);
            }

            (module, ())
        });
    }
}

/// Proof that there was an instance of memory generator and `MemoryGenerator::generate_memory` was called.
pub struct MemoryImportGenerationProof(());

/// Disabled wasm memory generator.
///
/// Instance of this types signals that there was once active memory generator,
/// but it ended up it's work.
pub struct DisabledMemoryGenerator(pub WasmModule);
