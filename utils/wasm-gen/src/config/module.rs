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

//! Config entities related to generating plain wasm module using `wasm-smith`.
//!
//! We don't give access to wasm_smith::`SwarmConfig` direct;y, but with several adaptors,
//! because valid wasm module is not always valid gear module. So, some configurational variables
//! can be arbitrary, but some must be constantly set. That's implemented with [`ArbitraryParams`]
//! and [`ConstantParams`].

use arbitrary::{Arbitrary, Result, Unstructured};
pub use wasm_smith::InstructionKind;
use wasm_smith::{InstructionKind::*, InstructionKinds, SwarmConfig};

/// Wasm module generation config.
///
/// This config wraps the `wasm_smith::SwarmConfig`. That's to make it
/// easy creating a configuration, which is custom, from one side, and,
/// from another side, results in generating valid gear wasm modules.
#[derive(Debug, Clone)]
pub struct WasmModuleConfig(SwarmConfig);

impl WasmModuleConfig {
    /// Unwrap the inner `wasm-smith` config.
    pub fn into_inner(self) -> SwarmConfig {
        self.0
    }
}

impl Arbitrary<'_> for WasmModuleConfig {
    fn arbitrary(u: &mut Unstructured<'_>) -> Result<Self> {
        let selectable_params = SelectableParams::default();
        let arbitrary_params = u.arbitrary::<ArbitraryParams>()?;

        Ok((selectable_params, arbitrary_params).into())
    }
}

impl From<(SelectableParams, ArbitraryParams)> for WasmModuleConfig {
    fn from((selectable_params, arbitrary_params): (SelectableParams, ArbitraryParams)) -> Self {
        let ConstantParams {
            allow_start_export,
            bulk_memory_enabled,
            exceptions_enabled,
            max_exports,
            max_imports,
            max_memories,
            min_memories,
            max_tables,
            memory64_enabled,
            min_exports,
            max_funcs,
            min_imports,
            multi_value_enabled,
            reference_types_enabled,
            relaxed_simd_enabled,
            saturating_float_to_int_enabled,
            sign_extension_enabled,
            simd_enabled,
            float_enabled,
            memory_grow_enabled,
            min_funcs,
            max_data_segments,
            min_data_segments,
            max_types,
            min_types,
        } = ConstantParams::default();

        let SelectableParams {
            call_indirect_enabled,
            allowed_instructions,
            max_instructions,
        } = selectable_params;

        let ArbitraryParams {
            available_imports,
            canonicalize_nans,
            export_everything,
            max_aliases,
            max_components,
            max_element_segments,
            max_elements,
            max_globals,
            max_instances,
            max_modules,
            max_nesting_depth,
            max_tags,
            max_type_size,
            max_values,
            memory_max_size_required,
            memory_offset_choices,
            min_element_segments,
            min_elements,
            min_globals,
            min_tables,
            min_tags,
            min_uleb_size,
            threads_enabled,
            max_table_elements,
            table_max_size_required,
            max_memory_pages,
        } = arbitrary_params;

        let allowed_instructions = InstructionKinds::new(&allowed_instructions);

        Self(SwarmConfig {
            allow_start_export,
            available_imports,
            bulk_memory_enabled,
            canonicalize_nans,
            exceptions_enabled,
            export_everything,
            max_aliases,
            max_components,
            max_data_segments,
            max_element_segments,
            max_elements,
            max_exports,
            max_funcs,
            max_globals,
            max_imports,
            max_instances,
            max_instructions,
            max_memories,
            max_memory_pages,
            max_modules,
            max_nesting_depth,
            max_tables,
            max_tags,
            max_type_size,
            max_types,
            max_values,
            memory64_enabled,
            memory_max_size_required,
            memory_offset_choices,
            min_data_segments,
            min_element_segments,
            min_elements,
            min_exports,
            min_funcs,
            min_globals,
            min_imports,
            min_memories,
            min_tables,
            min_tags,
            min_types,
            min_uleb_size,
            multi_value_enabled,
            reference_types_enabled,
            relaxed_simd_enabled,
            saturating_float_to_int_enabled,
            sign_extension_enabled,
            simd_enabled,
            float_enabled,
            threads_enabled,
            allowed_instructions,
            max_table_elements,
            table_max_size_required,
            memory_grow_enabled,
            call_indirect_enabled,
        })
    }
}

/// Arbitrary wasm module generation params.
///
/// These are params that are allowed to be randomly set.
/// All of them are later used to instantiate `wasm_smith::SwarmConfig`.
#[derive(Debug, Clone)]
pub struct ArbitraryParams {
    available_imports: Option<Vec<u8>>,
    canonicalize_nans: bool,
    export_everything: bool,
    max_aliases: usize,
    max_components: usize,
    max_element_segments: usize,
    max_elements: usize,
    max_globals: usize,
    max_instances: usize,
    max_modules: usize,
    max_nesting_depth: usize,
    max_tags: usize,
    max_type_size: u32,
    max_values: usize,
    memory_max_size_required: bool,
    memory_offset_choices: (u32, u32, u32),
    min_element_segments: usize,
    min_elements: usize,
    min_globals: usize,
    min_tables: u32,
    min_tags: usize,
    min_uleb_size: u8,
    threads_enabled: bool,
    max_table_elements: u32,
    table_max_size_required: bool,
    max_memory_pages: u64,
}

impl Arbitrary<'_> for ArbitraryParams {
    fn arbitrary(u: &mut Unstructured<'_>) -> Result<Self> {
        let random_swarm = u.arbitrary()?;
        let SwarmConfig {
            available_imports,
            canonicalize_nans,
            export_everything,
            max_aliases,
            max_components,
            max_element_segments,
            max_elements,
            max_globals,
            max_instances,
            max_modules,
            max_nesting_depth,
            max_tags,
            max_type_size,
            max_values,
            memory_max_size_required,
            memory_offset_choices,
            min_element_segments,
            min_elements,
            min_globals,
            min_tables,
            min_tags,
            min_uleb_size,
            threads_enabled,
            max_table_elements,
            table_max_size_required,
            max_memory_pages,
            ..
        } = random_swarm;

        Ok(ArbitraryParams {
            available_imports,
            canonicalize_nans,
            export_everything,
            max_aliases,
            max_components,
            max_element_segments,
            max_elements,
            max_globals,
            max_instances,
            max_modules,
            max_nesting_depth,
            max_tags,
            max_type_size,
            max_values,
            memory_max_size_required,
            memory_offset_choices,
            min_element_segments,
            min_elements,
            min_globals,
            min_tables,
            min_tags,
            min_uleb_size,
            threads_enabled,
            max_table_elements,
            table_max_size_required,
            max_memory_pages,
        })
    }
}

/// Constant wasm module generation params.
///
/// Wraps params, which are used to create `wasm_smith::SwarmConfig`, but they
/// must have pre-defined values to make `wasm-smith` generate valid gear modules.
pub struct ConstantParams {
    allow_start_export: bool,
    bulk_memory_enabled: bool,
    exceptions_enabled: bool,
    max_data_segments: usize,
    max_exports: usize,
    max_imports: usize,
    max_types: usize,
    max_memories: usize,
    min_memories: u32,
    max_tables: usize,
    memory64_enabled: bool,
    min_exports: usize,
    min_data_segments: usize,
    max_funcs: usize,
    min_imports: usize,
    multi_value_enabled: bool,
    reference_types_enabled: bool,
    relaxed_simd_enabled: bool,
    saturating_float_to_int_enabled: bool,
    sign_extension_enabled: bool,
    simd_enabled: bool,
    float_enabled: bool,
    memory_grow_enabled: bool,
    min_funcs: usize,
    min_types: usize,
}

impl Default for ConstantParams {
    fn default() -> Self {
        ConstantParams {
            bulk_memory_enabled: false,
            sign_extension_enabled: false,
            saturating_float_to_int_enabled: false,
            reference_types_enabled: false,
            // This is related to reference_types_enabled.
            max_tables: 1,
            simd_enabled: false,
            float_enabled: false,
            relaxed_simd_enabled: false,
            exceptions_enabled: false,
            memory64_enabled: false,
            allow_start_export: false,
            multi_value_enabled: false,
            memory_grow_enabled: false,
            min_memories: 0,
            max_memories: 1,
            min_exports: 0,
            max_exports: 0,
            min_imports: 0,
            max_imports: 0,
            min_funcs: 15,
            max_funcs: 30,
            max_data_segments: 0,
            min_data_segments: 0,
            max_types: 100,
            min_types: 5,
        }
    }
}

/// Selectable wasm module generation params.
#[derive(Debug, Clone)]
pub struct SelectableParams {
    /// Flag signalizing whether `call_indirect` instruction
    /// must be used or not.
    pub call_indirect_enabled: bool,
    /// Set of [`InstructionKind`], that are allowed to
    /// be generated by `wasm-gen`.
    pub allowed_instructions: Vec<InstructionKind>,
    /// Maximum amount of instructions that `wasm-gen`
    /// can generate before inserting syscalls.
    pub max_instructions: usize,
}

impl Default for SelectableParams {
    fn default() -> Self {
        Self {
            call_indirect_enabled: true,
            allowed_instructions: vec![Numeric, Reference, Parametric, Variable, Table, Memory],
            max_instructions: 100_000,
        }
    }
}
