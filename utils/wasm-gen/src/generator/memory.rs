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

//! Memory import generator module.

use crate::{
    generator::{FrozenGearWasmGenerator, GearWasmGenerator},
    MemoryPagesConfig, WasmModule,
};
use gear_wasm_instrument::{
    parity_wasm::{
        builder,
        elements::{Instruction, Section},
    },
    STACK_END_EXPORT_NAME,
};

/// Memory import generator.
///
/// The generator is used to insert into wasm module new
/// valid (from gear runtime perspective) memory import definition
/// from the provided config.
pub struct MemoryGenerator {
    config: MemoryPagesConfig,
    module: WasmModule,
}

impl<'a> From<GearWasmGenerator<'a>> for (MemoryGenerator, FrozenGearWasmGenerator<'a>) {
    fn from(generator: GearWasmGenerator<'a>) -> Self {
        let mem_generator = MemoryGenerator {
            config: generator.config.memory_config,
            module: generator.module,
        };
        let frozen = FrozenGearWasmGenerator {
            config: generator.config,
            unstructured: Some(generator.unstructured),
        };

        (mem_generator, frozen)
    }
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

impl<'a> From<(DisabledMemoryGenerator, FrozenGearWasmGenerator<'a>)> for GearWasmGenerator<'a> {
    fn from(
        (disabled_mem_gen, frozen_gear_wasm_gen): (
            DisabledMemoryGenerator,
            FrozenGearWasmGenerator<'a>,
        ),
    ) -> Self {
        GearWasmGenerator {
            module: disabled_mem_gen.0,
            config: frozen_gear_wasm_gen.config,
            unstructured: frozen_gear_wasm_gen
                .unstructured
                .expect("internal error: counterfeit frozen gear wasm gen is used"),
        }
    }
}
