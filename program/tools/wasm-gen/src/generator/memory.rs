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

//! Memory import generator module.

use crate::{
    MemoryPagesConfig, WasmModule,
    generator::{CallIndexes, FrozenGearWasmGenerator, GearWasmGenerator, ModuleWithCallIndexes},
};
use gear_core::pages::WasmPage;
use gear_wasm_instrument::{Export, Global, Import, ModuleBuilder, STACK_END_EXPORT_NAME};

/// Memory import generator.
///
/// The generator is used to insert into wasm module new
/// valid (from gear runtime perspective) memory import definition
/// from the provided config.
pub struct MemoryGenerator {
    config: MemoryPagesConfig,
    module: WasmModule,
}

impl<'a, 'b> From<GearWasmGenerator<'a, 'b>>
    for (MemoryGenerator, FrozenGearWasmGenerator<'a, 'b>)
{
    fn from(generator: GearWasmGenerator<'a, 'b>) -> Self {
        let mem_generator = MemoryGenerator {
            config: generator.config.memory_config,
            module: generator.module,
        };
        let frozen = FrozenGearWasmGenerator {
            config: generator.config,
            unstructured: Some(generator.unstructured),
            call_indexes: Some(generator.call_indexes),
        };

        (mem_generator, frozen)
    }
}

impl MemoryGenerator {
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
        log::trace!("Generating memory section");

        self.remove_mem_section();

        let MemoryGenerator {
            mut module,
            config:
                MemoryPagesConfig {
                    initial_size,
                    upper_limit,
                    stack_end_page,
                },
        } = self;

        log::trace!("Initial pages num - {initial_size}");

        // Define memory import in the module
        module.with(|module| {
            let mut builder = ModuleBuilder::from_module(module);
            builder.push_import(Import::memory(initial_size, upper_limit));

            // Define optional stack-end
            if let Some(stack_end_page) = stack_end_page {
                log::trace!("Stack end offset - {stack_end_page:?}");

                let stack_end = stack_end_page * WasmPage::SIZE;
                let stack_end_index = builder.push_global(Global::i32_value(stack_end as i32));

                builder.push_export(Export::global(STACK_END_EXPORT_NAME, stack_end_index));
            }

            (builder.build(), ())
        });

        (
            DisabledMemoryGenerator(module),
            MemoryImportGenerationProof(()),
        )
    }

    // Remove current memory section.
    fn remove_mem_section(&mut self) {
        self.module.with(|mut module| {
            module.memory_section = None;
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
pub struct DisabledMemoryGenerator(WasmModule);

impl From<DisabledMemoryGenerator> for ModuleWithCallIndexes {
    fn from(DisabledMemoryGenerator(module): DisabledMemoryGenerator) -> Self {
        let call_indexes = CallIndexes::new(&module);

        ModuleWithCallIndexes {
            module,
            call_indexes,
        }
    }
}

impl<'a, 'b> From<(DisabledMemoryGenerator, FrozenGearWasmGenerator<'a, 'b>)>
    for GearWasmGenerator<'a, 'b>
{
    fn from(
        (disabled_mem_gen, frozen_gear_wasm_gen): (
            DisabledMemoryGenerator,
            FrozenGearWasmGenerator<'a, 'b>,
        ),
    ) -> Self {
        GearWasmGenerator {
            module: disabled_mem_gen.0,
            config: frozen_gear_wasm_gen.config,
            unstructured: frozen_gear_wasm_gen
                .unstructured
                .expect("internal error: counterfeit frozen gear wasm gen is used"),
            call_indexes: frozen_gear_wasm_gen
                .call_indexes
                .expect("internal error: counterfeit frozen gear wasm gen is used"),
        }
    }
}
