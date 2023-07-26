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

//! Generators entities used to generate a valid gear wasm module.

use crate::{GearWasmGeneratorConfig, WasmModule};
use arbitrary::{Result, Unstructured};
use gear_wasm_instrument::parity_wasm::elements::Module;

mod memory;

pub use memory::*;

/// General gear wasm generator, which works as a mediator
/// controlling relations between various available in the
/// crate generators.
pub struct GearWasmGenerator<'a> {
    unstructured: &'a mut Unstructured<'a>,
    module: WasmModule,
    config: GearWasmGeneratorConfig,
}

impl<'a> GearWasmGenerator<'a> {
    /// Create a new generator with a default config..
    pub fn new(module: WasmModule, unstructured: &'a mut Unstructured<'a>) -> Self {
        Self::new_with_config(module, unstructured, GearWasmGeneratorConfig::default())
    }

    /// Create a new generator with a defined config.
    pub fn new_with_config(
        module: WasmModule,
        unstructured: &'a mut Unstructured<'a>,
        config: GearWasmGeneratorConfig,
    ) -> Self {
        Self {
            unstructured,
            module,
            config,
        }
    }

    /// Run all generators, while mediating between them.
    pub fn generate(self) -> Result<Module> {
        let (disabled_mem_gen, frozen_gear_wasm_gen, _mem_imports_gen_proof) =
            self.generate_memory_export();

        Ok(Self::from((disabled_mem_gen, frozen_gear_wasm_gen))
            .module
            .into_inner())
    }

    /// Generate memory export using memory generator.
    pub fn generate_memory_export(
        self,
    ) -> (
        DisabledMemoryGenerator,
        FrozenGearWasmGenerator<'a>,
        MemoryImportGenerationProof,
    ) {
        let (mem_gen, frozen_gear_wasm_gen): (MemoryGenerator, FrozenGearWasmGenerator) =
            self.into();
        let (disabled_mem_gen, mem_import_gen_proof) = mem_gen.generate_memory();

        (disabled_mem_gen, frozen_gear_wasm_gen, mem_import_gen_proof)
    }

    /// Disable current generator.
    pub fn disable(self) -> DisabledGearWasmGenerator {
        DisabledGearWasmGenerator(self.module)
    }
}

/// Frozen gear wasm generator.
///
/// Instantce of this generator signals. that the gear wasm generator
/// instance was converted to some other generator available in this crate.
/// This type serves as an access/ticket for converting some generator back
/// to the gear wasm generator. So it mainly controls state machine flow.
pub struct FrozenGearWasmGenerator<'a> {
    config: GearWasmGeneratorConfig,
    unstructured: Option<&'a mut Unstructured<'a>>,
}

impl<'a> FrozenGearWasmGenerator<'a> {
    /// Destroy the frozen generator and retrieve it's
    /// beneficial data.
    pub fn melt(self) -> GearWasmGeneratorConfig {
        self.config
    }
}

/// Disabled gear wasm generator.
///
/// It differs from the frozen gear wasm generator in the way, that
/// the latter one can be used to instantiate the gear wasm generator
/// again, but this signals that state machine transitions are ended.
pub struct DisabledGearWasmGenerator(WasmModule);

impl DisabledGearWasmGenerator {
    /// Converts into inner wasm module.
    pub fn into_wasm_module(self) -> WasmModule {
        self.0
    }
}
