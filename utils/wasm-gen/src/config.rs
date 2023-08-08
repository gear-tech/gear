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

//! Configs used to generate main entities of the crate.
//!
//! Configs, possibly, can be instantiated 3 different ways:
//! 1. From scratch by settings fields to corresponding values sometimes using
//! related to these fields builders. For example, wasm module configs:
//! ```rust
//! use gear_wasm_gen::*;
//! use arbitrary::{Arbitrary, Result, Unstructured};
//!
//! fn my_config<'a>(u: &'a mut Unstructured<'a>) -> Result<WasmModuleConfig> {
//!     let selectable_params = SelectableParams { call_indirect_enabled: false };
//!     let arbitrary = ArbitraryParams::arbitrary(u)?;
//!     Ok((selectable_params, arbitrary).into())
//! }
//! ```
//! Or, for example, gear wasm generators config:
//! ```rust
//! use gear_wasm_gen::*;
//! let memory_pages_config = MemoryPagesConfig {
//!     initial_size: 128,
//!     upper_limit: None,
//!     stack_end_page: Some(64),
//! };
//! let entry_points_set = EntryPointsSet::InitHandle;
//! let sys_calls_config = SysCallsConfigBuilder::new(SysCallsInjectionAmounts::all_once())
//!     .with_source_msg_dest()
//!     .with_log_info("I'm from wasm-gen".into())
//!     .build();
//!
//! let wasm_gen_config = GearWasmGeneratorConfig {
//!     memory_config: memory_pages_config,
//!     entry_points_config: entry_points_set,
//!     remove_recursions: true,
//!     sys_calls_config,
//! };
//! ```
//!
//! 2. By using `Default` trait.
//! For example:
//! ```rust
//! use gear_wasm_gen::*;
//! let wasm_gen_config = GearWasmGeneratorConfig::default();
//! ```
//!
//! 3. With `arbitrary::Unstructured`. For example:
//! ```rust
//! use gear_wasm_gen::*;
//! use arbitrary::{Result, Arbitrary, Unstructured};
//!
//! fn my_config<'a>(u: &'a mut Unstructured<'a>) -> Result<WasmModuleConfig> {
//!     WasmModuleConfig::arbitrary(u)
//! }
//! ```
//!
//! These kinds of config instatiations are helpful if you want to call generators
//! manually with some special (maybe not) generators state transition flow. However,
//! for the simplest usage with crate's entry point generation functions (like
//! [`crate::generate_gear_program_code`] or [`crate::generate_gear_program_module`])
//! you'd need a configs bundle - type which implement [`ConfigsBundle`].
//!
//! There's a pre-defined one - [`ValidGearWasmConfigsBundle`], using which will result
//! in generation of always valid gear-wasm module.

mod generator;
mod module;
mod syscalls;

pub use generator::*;
pub use module::*;
pub use syscalls::*;

use gear_utils::NonEmpty;
use gsys::Hash;

/// Trait which describes a type that stores and manages data for generating
/// [`GearWasmGeneratorConfig`] and [`SelectableParams`], which are both used
/// by [`crate::generate_gear_program_code`] and [`crate::generate_gear_program_module`]
/// to generate a gear wasm.
pub trait ConfigsBundle {
    /// Convert a "bundle" type into configs required for gear wasm creation
    /// from [`crate::generate_gear_program_code`] and [`crate::generate_gear_program_module`].
    fn into_parts(self) -> (GearWasmGeneratorConfig, SelectableParams);
}

/// Mock implementation.
impl ConfigsBundle for () {
    fn into_parts(self) -> (GearWasmGeneratorConfig, SelectableParams) {
        unimplemented!("Mock")
    }
}

/// Set of configuration data which is used to generate always
/// valid gear-wasm using generators of the current crate.
#[derive(Debug, Clone)]
pub struct ValidGearWasmConfigsBundle<T = [u8; 32]> {
    /// Externalities to be logged.
    pub log_info: Option<String>,
    /// Set of existing addresses, which will be used as message destinations.
    ///
    /// If is `None`, then `gr_source` result will be used as a message destination.
    pub existing_addresses: Option<NonEmpty<T>>,
    /// Flag which signals whether recursions must be removed.
    pub remove_recursion: bool,
    /// Flag which signals whether `call_indirect` instruction not be used
    /// during wasm generation.
    pub call_indirect_enabled: bool,
    /// Injection amount ranges for each sys-call.
    pub injection_amounts: SysCallsInjectionAmounts,
    /// Config of gear wasm call entry-points (exports).
    pub entry_points_set: EntryPointsSet,
}

impl<T> Default for ValidGearWasmConfigsBundle<T> {
    fn default() -> Self {
        Self {
            log_info: Some("Valid config".into()),
            existing_addresses: None,
            remove_recursion: false,
            call_indirect_enabled: true,
            injection_amounts: SysCallsInjectionAmounts::all_once(),
            entry_points_set: Default::default(),
        }
    }
}

impl<T: Into<Hash>> ConfigsBundle for ValidGearWasmConfigsBundle<T> {
    fn into_parts(self) -> (GearWasmGeneratorConfig, SelectableParams) {
        let ValidGearWasmConfigsBundle {
            log_info,
            existing_addresses,
            remove_recursion,
            call_indirect_enabled,
            injection_amounts,
            entry_points_set,
        } = self;

        let selectable_params = SelectableParams {
            call_indirect_enabled,
        };

        let mut sys_calls_config_builder = SysCallsConfigBuilder::new(injection_amounts);
        if let Some(log_info) = log_info {
            sys_calls_config_builder = sys_calls_config_builder.with_log_info(log_info);
        }
        if let Some(addresses) = existing_addresses {
            sys_calls_config_builder =
                sys_calls_config_builder.with_data_offset_msg_dest(addresses);
        } else {
            sys_calls_config_builder = sys_calls_config_builder.with_source_msg_dest();
        }
        let gear_wasm_generator_config = GearWasmGeneratorConfigBuilder::new()
            .with_recursions_removed(remove_recursion)
            .with_sys_calls_config(sys_calls_config_builder.build())
            .with_entry_points_config(entry_points_set)
            .build();

        (gear_wasm_generator_config, selectable_params)
    }
}
