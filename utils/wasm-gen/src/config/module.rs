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

//! Config entities related to generating plain wasm module using `wasm-smith`.
//!
//! We don't give access to [`wasm_smith::Config`] directly, but with several adaptors,
//! because valid wasm module is not always valid gear module. So, some configurational variables
//! can be arbitrary, but some must be constantly set. That's implemented with [`ArbitraryParams`]
//! and [`ConstantParams`].

pub use wasm_smith::InstructionKind;

use crate::MemoryLayout;
use arbitrary::{Arbitrary, Result, Unstructured};
use std::num::NonZero;
use wasm_smith::{Config, InstructionKind::*, InstructionKinds, MemoryOffsetChoices};

/// Wasm module generation config.
///
/// This config wraps the [`wasm_smith::Config`]. That's to make it
/// easy creating a configuration, which is custom, from one side, and,
/// from another side, results in generating valid gear wasm modules.
#[derive(Debug, Clone)]
pub struct WasmModuleConfig(Config);

impl WasmModuleConfig {
    /// Unwrap the inner `wasm-smith` config.
    pub fn into_inner(self) -> Config {
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
            disallow_traps,
            exceptions_enabled,
            max_exports,
            max_imports,
            max_memories,
            min_memories,
            max_tables,
            memory64_enabled,
            max_memory64_bytes,
            min_exports,
            min_imports,
            multi_value_enabled,
            reference_types_enabled,
            tail_call_enabled,
            relaxed_simd_enabled,
            saturating_float_to_int_enabled,
            sign_extension_ops_enabled,
            simd_enabled,
            allow_floats,
            max_data_segments,
            min_data_segments,
            max_types,
            min_types,
            memory_offset_choices,
            reserved_memory_size,
            gc_enabled,
            custom_page_sizes_enabled,
            generate_custom_sections,
            exports,
            memory_grow_enabled,
            shared_everything_threads_enabled,
            allow_invalid_funcs,
            wide_arithmetic_enabled,
            extended_const_enabled,
        } = ConstantParams::default();

        let SelectableParams {
            allowed_instructions,
            max_instructions,
            min_funcs,
            max_funcs,
        } = selectable_params;

        let min_funcs = min_funcs.get();
        let max_funcs = max_funcs.get();

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
            max_memory32_bytes,
            max_nesting_depth,
            max_tags,
            max_type_size,
            max_values,
            memory_max_size_required,
            min_element_segments,
            min_elements,
            min_globals,
            min_tables,
            min_tags,
            min_uleb_size,
            threads_enabled,
            max_table_elements,
            table_max_size_required,
        } = arbitrary_params;

        let allowed_instructions = InstructionKinds::new(&allowed_instructions);

        Self(Config {
            allow_start_export,
            available_imports,
            bulk_memory_enabled,
            canonicalize_nans,
            disallow_traps,
            exceptions_enabled,
            export_everything,
            gc_enabled,
            custom_page_sizes_enabled,
            generate_custom_sections,
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
            max_memory32_bytes,
            max_memory64_bytes,
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
            reserved_memory_size,
            memory_grow_enabled,
            tail_call_enabled,
            relaxed_simd_enabled,
            saturating_float_to_int_enabled,
            sign_extension_ops_enabled,
            shared_everything_threads_enabled,
            simd_enabled,
            threads_enabled,
            allow_invalid_funcs,
            wide_arithmetic_enabled,
            allowed_instructions,
            max_table_elements,
            table_max_size_required,
            exports,
            allow_floats,
            extended_const_enabled,
        })
    }
}

/// Arbitrary wasm module generation params.
///
/// These are params that are allowed to be randomly set.
/// All of them are later used to instantiate `wasm_smith::Config`.
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
    min_element_segments: usize,
    min_elements: usize,
    min_globals: usize,
    min_tables: u32,
    min_tags: usize,
    min_uleb_size: u8,
    threads_enabled: bool,
    max_table_elements: u64,
    table_max_size_required: bool,
    max_memory32_bytes: u64,
}

impl Arbitrary<'_> for ArbitraryParams {
    fn arbitrary(u: &mut Unstructured<'_>) -> Result<Self> {
        let random_config = u.arbitrary()?;
        let Config {
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
            min_element_segments,
            min_elements,
            min_globals,
            min_tables,
            min_tags,
            min_uleb_size,
            threads_enabled,
            max_table_elements,
            table_max_size_required,
            max_memory32_bytes,
            ..
        } = random_config;

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
            min_element_segments,
            min_elements,
            min_globals,
            min_tables,
            min_tags,
            min_uleb_size,
            threads_enabled,
            max_table_elements,
            table_max_size_required,
            max_memory32_bytes,
        })
    }
}

/// Constant wasm module generation params.
///
/// Wraps params, which are used to create `wasm_smith::Config`, but they
/// must have pre-defined values to make `wasm-smith` generate valid gear modules.
pub struct ConstantParams {
    allow_start_export: bool,
    bulk_memory_enabled: bool,
    disallow_traps: bool,
    exceptions_enabled: bool,
    max_data_segments: usize,
    max_exports: usize,
    max_imports: usize,
    max_types: usize,
    max_memories: usize,
    min_memories: u32,
    max_tables: usize,
    memory64_enabled: bool,
    max_memory64_bytes: u128,
    min_exports: usize,
    min_data_segments: usize,
    min_imports: usize,
    multi_value_enabled: bool,
    reference_types_enabled: bool,
    tail_call_enabled: bool,
    relaxed_simd_enabled: bool,
    saturating_float_to_int_enabled: bool,
    sign_extension_ops_enabled: bool,
    simd_enabled: bool,
    allow_floats: bool,
    min_types: usize,
    memory_offset_choices: MemoryOffsetChoices,
    gc_enabled: bool,
    custom_page_sizes_enabled: bool,
    generate_custom_sections: bool,
    // pass empty module to not export anything to pass our checks
    exports: Option<Vec<u8>>,
    shared_everything_threads_enabled: bool,
    allow_invalid_funcs: bool,
    wide_arithmetic_enabled: bool,
    extended_const_enabled: bool,
    // our patches
    reserved_memory_size: Option<u64>,
    memory_grow_enabled: bool,
}

impl Default for ConstantParams {
    fn default() -> Self {
        ConstantParams {
            bulk_memory_enabled: false,
            sign_extension_ops_enabled: false,
            saturating_float_to_int_enabled: false,
            reference_types_enabled: false,
            tail_call_enabled: false,
            // This is related to reference_types_enabled.
            max_tables: 1,
            simd_enabled: false,
            allow_floats: false,
            relaxed_simd_enabled: false,
            exceptions_enabled: false,
            // we don't support 64-bit WASM
            memory64_enabled: false,
            max_memory64_bytes: 0,
            disallow_traps: true,
            allow_start_export: false,
            multi_value_enabled: false,
            min_memories: 0,
            max_memories: 1,
            min_exports: 0,
            max_exports: 0,
            min_imports: 0,
            max_imports: 0,
            max_data_segments: 0,
            min_data_segments: 0,
            max_types: 100,
            min_types: 5,
            memory_offset_choices: MemoryOffsetChoices(75, 25, 0),
            gc_enabled: false,
            custom_page_sizes_enabled: false,
            generate_custom_sections: false,
            exports: Some(b"\0asm\x01\0\0\0".to_vec()),
            shared_everything_threads_enabled: false,
            allow_invalid_funcs: false,
            wide_arithmetic_enabled: false,
            extended_const_enabled: false,
            reserved_memory_size: Some(MemoryLayout::RESERVED_MEMORY_SIZE as u64),
            memory_grow_enabled: false,
        }
    }
}

/// Selectable wasm module generation params.
#[derive(Debug, Clone)]
pub struct SelectableParams {
    /// Set of [`InstructionKind`], that are allowed to
    /// be generated by `wasm-gen`.
    pub allowed_instructions: Vec<InstructionKind>,
    /// Maximum amount of instructions that `wasm-gen`
    /// can generate before inserting syscalls.
    pub max_instructions: usize,
    /// Minimum amount of functions `wasm-gen` will insert
    /// into generated wasm.
    pub min_funcs: NonZero<usize>,
    /// Maximum amount of functions `wasm-gen` will insert
    /// into generated wasm.
    pub max_funcs: NonZero<usize>,
}

impl Default for SelectableParams {
    fn default() -> Self {
        Self {
            allowed_instructions: vec![
                Numeric, Reference, Parametric, Variable, Table, Memory, Control,
            ],
            max_instructions: 500,
            min_funcs: NonZero::<usize>::new(3).expect("from non zero value; qed."),
            max_funcs: NonZero::<usize>::new(5).expect("from non zero value; qed."),
        }
    }
}
