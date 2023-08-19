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

//! Gear wasm entry points generator module.

use crate::{
    generator::{CallIndexes, FrozenGearWasmGenerator, GearWasmGenerator, ModuleWithCallIndexes},
    EntryPointsSet, WasmModule,
};
use arbitrary::{Result, Unstructured};
use gear_wasm_instrument::parity_wasm::{
    builder,
    elements::{FunctionType, Instruction, Instructions, Type, ValueType},
};

/// Gear wasm entry points generator.
///
/// Inserts gear wasm required export functions depending on the config.
pub struct EntryPointsGenerator<'a, 'b> {
    unstructured: &'b mut Unstructured<'a>,
    module: WasmModule,
    config: EntryPointsSet,
    call_indexes: CallIndexes,
}

impl<'a, 'b> From<GearWasmGenerator<'a, 'b>>
    for (
        EntryPointsGenerator<'a, 'b>,
        FrozenGearWasmGenerator<'a, 'b>,
    )
{
    fn from(generator: GearWasmGenerator<'a, 'b>) -> Self {
        let ep_generator = EntryPointsGenerator {
            unstructured: generator.unstructured,
            module: generator.module,
            config: generator.config.entry_points_config,
            call_indexes: generator.call_indexes,
        };
        let frozen = FrozenGearWasmGenerator {
            config: generator.config,
            call_indexes: None,
            unstructured: None,
        };

        (ep_generator, frozen)
    }
}

impl<'a, 'b> EntryPointsGenerator<'a, 'b> {
    /// Instantiate a new gear wasm entry points generator.
    pub fn new(
        module_with_indexes: ModuleWithCallIndexes,
        config: EntryPointsSet,
        unstructured: &'b mut Unstructured<'a>,
    ) -> Self {
        let ModuleWithCallIndexes {
            module,
            call_indexes,
        } = module_with_indexes;

        Self {
            unstructured,
            module,
            config,
            call_indexes,
        }
    }

    /// Disable current generator.
    pub fn disable(self) -> DisabledEntryPointsGenerator<'a, 'b> {
        log::trace!(
            "Random data when disabling gear entry points generator - {}",
            self.unstructured.len()
        );
        DisabledEntryPointsGenerator {
            unstructured: self.unstructured,
            module: self.module,
            call_indexes: self.call_indexes,
        }
    }

    /// Generates gear wasm entry points from the config, used to instantiate the generator.
    ///
    /// Returns disabled entry points generator and a proof that all entry points from config were generated.
    pub fn generate_entry_points(
        mut self,
    ) -> Result<(
        DisabledEntryPointsGenerator<'a, 'b>,
        GearEntryPointGenerationProof,
    )> {
        log::trace!("Generating gear entry points");

        if self.config.has_init() {
            self.generate_export("init")?;
        }

        if self.config.has_handle() {
            self.generate_export("handle")?;
        }

        if self.config.has_handle_reply() {
            self.generate_export("handle_reply")?;
        }

        Ok((self.disable(), GearEntryPointGenerationProof(())))
    }

    /// Generates an export function with a `name`.
    ///
    /// Actually generating a new export function doesn't mean generating it's body
    /// from scratch. This function chooses random internal function and calls it
    /// from the body of the newly generated export.
    ///
    /// # Note:
    /// 1. The method is intended to generate just exports, not only gear entry points.
    /// 2. If the generator was used to generate some export with a custom name (not gear entry point)
    /// and then disabled, that export index can be retrieved from [`DisabledEntryPointsGenerator`], by
    /// accessing the underlying `parity_wasm::module::Module` and iterating over it's export section.
    pub fn generate_export(&mut self, name: &str) -> Result<GearEntryPointGenerationProof> {
        log::trace!(
            "Random data before generating {name} export - {}",
            self.unstructured.len()
        );

        let last_func_idx = self.module.count_code_funcs() - 1;
        let export_body_call_idx = self.unstructured.int_in_range(0..=last_func_idx)?;

        // New export func index is the last from function section.
        let export_idx = last_func_idx + 1;

        // Get export body call signature
        let export_body_call_func_type = self.module.with(|module| {
            let func_type_ref = module
                .function_section()
                .expect("has at least one function by config")
                .entries()
                .get(export_body_call_idx)
                .expect("call index is received from module")
                .type_ref();

            let Type::Function(func_type) = module
                .type_section()
                .expect("")
                .types()
                .get(func_type_ref as usize)
                .cloned()
                .expect("func exists, so type does");

            (module, func_type)
        });

        let export_body_instructions =
            self.generate_export_body(export_body_call_idx, export_body_call_func_type)?;
        self.module.with(|module| {
            let module = builder::from_module(module)
                .function()
                .body()
                .with_instructions(Instructions::new(export_body_instructions))
                .build()
                .signature()
                .build()
                .build()
                .export()
                .field(name)
                .internal()
                .func(export_idx as u32)
                .build()
                .build();

            (module, ())
        });

        log::trace!("Generated export - {name}");
        self.call_indexes.add_func(export_idx);

        Ok(GearEntryPointGenerationProof(()))
    }

    /// Generates body of the export function.
    fn generate_export_body(
        &mut self,
        export_body_call_idx: usize,
        export_body_call_func_type: FunctionType,
    ) -> Result<Vec<Instruction>> {
        let params = export_body_call_func_type.params();
        let results = export_body_call_func_type.results();
        // +2 for End and Call instructions.
        let mut res = Vec::with_capacity(params.len() + results.len() + 2);
        for param in params {
            let instr = match param {
                ValueType::I32 => Instruction::I32Const(self.unstructured.arbitrary()?),
                ValueType::I64 => Instruction::I64Const(self.unstructured.arbitrary()?),
                _ => panic!("EntryPointsGenerator::get_call_instruction: can't handle f32/f64"),
            };
            res.push(instr);
        }
        res.push(Instruction::Call(export_body_call_idx as u32));
        res.extend(results.iter().map(|_| Instruction::Drop));
        res.push(Instruction::End);

        Ok(res)
    }
}

/// Proof that there was an instance of entry points generator and `EntryPointsGenerator::generate_export_entry_point` was called.
pub struct GearEntryPointGenerationProof(());

/// Disabled gear wasm entry points generator.
///
/// Instance of this types signals that there was once active gear wasm
/// entry points generator, but it ended up it's work.
pub struct DisabledEntryPointsGenerator<'a, 'b> {
    unstructured: &'b mut Unstructured<'a>,
    module: WasmModule,
    call_indexes: CallIndexes,
}

impl<'a, 'b> From<DisabledEntryPointsGenerator<'a, 'b>> for ModuleWithCallIndexes {
    fn from(ep_gen: DisabledEntryPointsGenerator<'a, 'b>) -> Self {
        ModuleWithCallIndexes {
            module: ep_gen.module,
            call_indexes: ep_gen.call_indexes,
        }
    }
}

impl<'a, 'b>
    From<(
        DisabledEntryPointsGenerator<'a, 'b>,
        FrozenGearWasmGenerator<'a, 'b>,
    )> for GearWasmGenerator<'a, 'b>
{
    fn from(
        (disabled_ep_gen, frozen_gear_wasm_gen): (
            DisabledEntryPointsGenerator<'a, 'b>,
            FrozenGearWasmGenerator<'a, 'b>,
        ),
    ) -> Self {
        GearWasmGenerator {
            unstructured: disabled_ep_gen.unstructured,
            module: disabled_ep_gen.module,
            config: frozen_gear_wasm_gen.config,
            call_indexes: disabled_ep_gen.call_indexes,
        }
    }
}
