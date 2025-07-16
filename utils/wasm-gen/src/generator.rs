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

//! Generators entities used to generate a valid gear wasm module.
//!
//! Generally, all generators have same work-patterns:
//! 1. Every generator has a disabled version of itself.
//! 2. Almost all disabled generators can be converted to [`ModuleWithCallIndexes`], from which the wasm module can be retrieved.
//! 3. Every generator has a "main" function, after finishing which a transition to another generator is available (either directly or through disabled
//!    version of the generator).
//! 4. Almost all generators are instantiated from the disabled generator worked on the previous generation step (see [`GearWasmGenerator::generate`]). This is how
//!    generator form a state-machine.
//!
//! Transitions paths:
//! ```text
//! # Zero generators nesting level
//! GearWasmGenerator--->DisabledGearWasmGenerator--->ModuleWithCallIndexes--->WasmModule
//!
//! # First generators nesting level
//! GearWasmGenerator--->MemoryGenerator--->DisabledMemoryGenerator--->ModuleWithCallIndexes--->WasmModule
//!
//! # Second generators nesting level
//! GearWasmGenerator--->MemoryGenerator--(DisabledMemoryGenerator, FrozenGearWasmGenerator)---\
//! |--->GearWasmGenerator--->EntryPointsGenerator--->DisabledEntryPointsGenerator--->ModuleWithCallIndexes--->
//!
//! # Third generators nesting level
//! GearWasmGenerator--->MemoryGenerator--(DisabledMemoryGenerator, FrozenGearWasmGenerator)---\
//! |--->GearWasmGenerator--->EntryPointsGenerator--->DisabledEntryPointsGenerator--(MemoryImportGenerationProof, GearEntryPointGenerationProof)-->(syscalls-module-state-machine)
//! ```
//!
//! State machine named `(syscalls-module-state-machine)` can be started only with having proof of work from `MemoryGenerator` and `EntryPointsGenerator`.
//! For more info about this state machine read docs of the [`syscalls`] mod.

mod entry_points;
mod memory;
pub mod syscalls;

pub use entry_points::*;
pub use memory::*;
pub use syscalls::*;

use crate::{GearWasmGeneratorConfig, WasmModule, utils};
use arbitrary::{Result, Unstructured};
use gear_wasm_instrument::Module;
use std::{collections::HashSet, ops::RangeInclusive};

/// Module and it's call indexes carrier.
///
/// # Rationale:
/// `WasmModule` and `CallIndexes` have an implicit relationship: newly added imports
/// and functions can be included to the wasm, but not in the call indexes (if we forgot to do that).
/// Although, adding call indexes is controlled in the generator, some other generators
/// can be instantiated with wasm  module and call indexes being unrelated to each other.
/// So this carrier is used to control wasm module and call indexes value flow, so related
/// values will always be delivered together.
pub struct ModuleWithCallIndexes {
    module: WasmModule,
    call_indexes: CallIndexes,
}

impl ModuleWithCallIndexes {
    /// Convert into inner wasm module
    pub fn into_wasm_module(self) -> WasmModule {
        self.module
    }
}

/// General gear wasm generator, which works as a mediator
/// controlling relations between various available in the
/// crate generators.
pub struct GearWasmGenerator<'a, 'b> {
    unstructured: &'b mut Unstructured<'a>,
    module: WasmModule,
    config: GearWasmGeneratorConfig,
    call_indexes: CallIndexes,
}

impl<'a, 'b> GearWasmGenerator<'a, 'b> {
    /// Create a new generator with a default config..
    pub fn new(module: WasmModule, unstructured: &'b mut Unstructured<'a>) -> Self {
        Self::new_with_config(module, unstructured, GearWasmGeneratorConfig::default())
    }

    /// Create a new generator with a defined config.
    pub fn new_with_config(
        module: WasmModule,
        unstructured: &'b mut Unstructured<'a>,
        config: GearWasmGeneratorConfig,
    ) -> Self {
        let call_indexes = CallIndexes::new(&module);

        Self {
            unstructured,
            module,
            config,
            call_indexes,
        }
    }

    /// Run all generators, while mediating between them.
    pub fn generate(self) -> Result<Module> {
        let (disabled_mem_gen, frozen_gear_wasm_gen, mem_imports_gen_proof) =
            self.generate_memory_export();

        let (disabled_ep_gen, frozen_gear_wasm_gen, ep_gen_proof) =
            Self::from((disabled_mem_gen, frozen_gear_wasm_gen))
                .generate_entry_points(mem_imports_gen_proof)?;

        let (disabled_syscalls_invocator, frozen_gear_wasm_gen) =
            Self::from((disabled_ep_gen, frozen_gear_wasm_gen)).generate_syscalls(ep_gen_proof)?;

        let config = frozen_gear_wasm_gen.melt();
        let module = ModuleWithCallIndexes::from(disabled_syscalls_invocator)
            .into_wasm_module()
            .into_inner();

        let module = if let Some(critical_gas_limit) = config.critical_gas_limit {
            log::trace!("Injecting critical gas limit");
            utils::inject_critical_gas_limit(module, critical_gas_limit)
        } else {
            log::trace!("Critical gas limit is not set");
            module
        };

        Ok(if config.remove_recursions {
            log::trace!("Removing recursions");
            utils::remove_recursion(module)
        } else {
            log::trace!("Leaving recursions");
            module
        })
    }

    /// Generate memory export using memory generator.
    pub fn generate_memory_export(
        self,
    ) -> (
        DisabledMemoryGenerator,
        FrozenGearWasmGenerator<'a, 'b>,
        MemoryImportGenerationProof,
    ) {
        let (mem_gen, frozen_gear_wasm_gen): (MemoryGenerator, FrozenGearWasmGenerator) =
            self.into();
        let (disabled_mem_gen, mem_import_gen_proof) = mem_gen.generate_memory();

        (disabled_mem_gen, frozen_gear_wasm_gen, mem_import_gen_proof)
    }

    /// Generate gear wasm gentry points using entry points generator.
    pub fn generate_entry_points(
        self,
        mem_import_gen_proof: MemoryImportGenerationProof,
    ) -> Result<(
        DisabledEntryPointsGenerator<'a, 'b>,
        FrozenGearWasmGenerator<'a, 'b>,
        GearEntryPointGenerationProof,
    )> {
        let entry_points_gen_instantiator =
            EntryPointsGeneratorInstantiator::from((self, mem_import_gen_proof));
        let (ep_gen, frozen_gear_wasm_gen): (EntryPointsGenerator, FrozenGearWasmGenerator) =
            entry_points_gen_instantiator.into();
        let (disabled_ep_gen, ep_gen_proof) = ep_gen.generate_entry_points()?;

        Ok((disabled_ep_gen, frozen_gear_wasm_gen, ep_gen_proof))
    }

    /// Generate syscalls using syscalls module generators.
    pub fn generate_syscalls(
        self,
        ep_gen_proof: GearEntryPointGenerationProof,
    ) -> Result<(DisabledSyscallsInvocator, FrozenGearWasmGenerator<'a, 'b>)> {
        let syscalls_imports_gen_instantiator =
            SyscallsImportsGeneratorInstantiator::from((self, ep_gen_proof));
        let (syscalls_imports_gen, frozen_gear_wasm_gen) = syscalls_imports_gen_instantiator.into();
        let syscalls_imports_gen_res = syscalls_imports_gen.generate()?;

        let ad_injector = AdditionalDataInjector::from(syscalls_imports_gen_res);
        let data_injection_res = ad_injector.inject();

        let syscalls_invocator = SyscallsInvocator::from(data_injection_res);
        let disabled_syscalls_invocator = syscalls_invocator.insert_invokes()?;

        Ok((disabled_syscalls_invocator, frozen_gear_wasm_gen))
    }

    /// Disable current generator.
    pub fn disable(self) -> DisabledGearWasmGenerator {
        DisabledGearWasmGenerator(self.module)
    }
}

/// Index in call indexes collection.
type CallIndexesHandle = usize;

/// Type used to manage (mainly, add and resolve) indexes
/// of the wasm module calls, which are, mostly, import functions
/// and internal functions.
struct CallIndexes {
    inner: Vec<FunctionIndex>,
    /// Indexes of wasm-module functions which were newly generated.
    ///
    /// These are indexes of functions which aren't generated from
    /// `wasm-smith` but from the current crate generators. All gear
    /// entry points ([`EntryPointsGenerator`]) and custom precise syscalls
    /// (generated in [`SyscallsImportsGenerator`]) are considered to be
    /// "custom" functions.
    ///
    /// Separating "pre-defined" functions from newly generated ones is important
    /// when syscalls invocator inserts calls of generated syscalls. For example,
    /// calls must not be inserted in the custom function which serves as a precise
    /// call to `gr_reservation_send` not to pollute it's internal instructions structure
    /// which is defined such that semantically correct `gr_reservation_send` call
    /// is executed.
    ///
    /// Same immutability is actual for gear exports to keep them as simple as possible.
    custom_funcs: HashSet<usize>,
}

impl CallIndexes {
    fn new(module: &WasmModule) -> Self {
        let import_funcs = module.count_import_funcs();
        let code_funcs = module.count_code_funcs();
        let mut inner = Vec::with_capacity(import_funcs + code_funcs);
        for i in 0..import_funcs {
            inner.push(FunctionIndex::Import(i as u32));
        }
        for i in 0..code_funcs {
            inner.push(FunctionIndex::Func(i as u32));
        }

        Self {
            inner,
            custom_funcs: HashSet::new(),
        }
    }

    pub(crate) fn get(&self, handle_idx: CallIndexesHandle) -> Option<FunctionIndex> {
        self.inner.get(handle_idx).copied()
    }

    fn predefined_funcs_indexes(&self) -> RangeInclusive<usize> {
        let last = if let Some(first_custom_func_idx) = self.custom_funcs.iter().min() {
            // Take last predefined func idx
            //
            // Subtraction is safe, because by config it's guaranteed
            // that there's at least one internal function from `wasm-smith`.
            // So, if there's only one predefined function, then first idx
            // of a custom function is 1.
            first_custom_func_idx - 1
        } else {
            self.inner
                .iter()
                .filter_map(FunctionIndex::internal_func_idx)
                .max()
                // Config min/max function params are `NonZero<usize>`.
                .expect("at least 1 func is generated by config definition") as usize
        };

        0..=last
    }

    fn len(&self) -> usize {
        self.inner.len()
    }

    fn add_func(&mut self, func_idx: usize) {
        log::trace!("Inserting function with func index {func_idx}");

        self.inner.push(FunctionIndex::Func(func_idx as u32));
        let is_new = self.custom_funcs.insert(func_idx);

        debug_assert!(is_new, "same inner index is used");
    }

    fn add_import(&mut self, import_idx: usize) {
        self.inner.push(FunctionIndex::Import(import_idx as u32));
    }
}

/// Index of the function/call in the wasm module.
///
/// Enum variants give information on the type of the function:
/// it's an import or internal function.
#[derive(Debug, Clone, Copy)]
enum FunctionIndex {
    Import(u32),
    Func(u32),
}

impl FunctionIndex {
    fn internal_func_idx(&self) -> Option<u32> {
        match self {
            FunctionIndex::Func(idx) => Some(*idx),
            _ => None,
        }
    }
}

/// Frozen gear wasm generator.
///
/// Instance of this generator signals, that some gear wasm generator
/// instance was converted to some other generator available in this crate.
/// This type serves as an access/ticket for converting some generator back
/// to the gear wasm generator. So it mainly controls state machine flow.
pub struct FrozenGearWasmGenerator<'a, 'b> {
    config: GearWasmGeneratorConfig,
    call_indexes: Option<CallIndexes>,
    unstructured: Option<&'b mut Unstructured<'a>>,
}

impl FrozenGearWasmGenerator<'_, '_> {
    /// Destroy the frozen generator and retrieve it's
    /// beneficial data.
    pub fn melt(self) -> GearWasmGeneratorConfig {
        self.config
    }
}

/// Disabled gear wasm generator.
///
/// Similar to [`FrozenGearWasmGenerator`], but this one signals that state
/// machine transitions are ended.
pub struct DisabledGearWasmGenerator(WasmModule);

impl DisabledGearWasmGenerator {
    /// Converts into inner wasm module.
    pub fn into_wasm_module(self) -> WasmModule {
        self.0
    }
}
