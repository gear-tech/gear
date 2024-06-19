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

//! Configs used to generate main entities of the crate.
//!
//! Configs, possibly, can be instantiated 3 different ways:
//! 1. From scratch by settings fields to corresponding values sometimes using
//! related to these fields builders. For example, wasm module configs:
//! ```rust
//! # use std::num::NonZeroUsize;
//! use gear_wasm_gen::*;
//! use arbitrary::{Arbitrary, Result, Unstructured};
//!
//! fn my_config<'a>(u: &'a mut Unstructured<'a>) -> Result<WasmModuleConfig> {
//!     let selectable_params = SelectableParams {
//!         allowed_instructions: vec![
//!             InstructionKind::Numeric,
//!             InstructionKind::Reference,
//!             InstructionKind::Parametric,
//!             InstructionKind::Variable,
//!             InstructionKind::Table,
//!             InstructionKind::Memory,
//!             InstructionKind::Control,
//!         ],
//!         max_instructions: 100_000,
//!         min_funcs: NonZeroUsize::new(15).unwrap(),
//!         max_funcs: NonZeroUsize::new(30).unwrap(),
//!     };
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
//! let syscalls_config = SyscallsConfigBuilder::new(SyscallsInjectionTypes::all_once())
//!     .with_log_info("I'm from wasm-gen".into())
//!     .build();
//!
//! let wasm_gen_config = GearWasmGeneratorConfig {
//!     memory_config: memory_pages_config,
//!     entry_points_config: entry_points_set,
//!     remove_recursions: false,
//!     critical_gas_limit: Some(1_000_000),
//!     syscalls_config,
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
//! These types of configs instantiations are helpful if you want to call generators
//! manually with some special (maybe not) generators state transition flow. However,
//! for the simplest usage with crate's main generation functions (like
//! [`crate::generate_gear_program_code`] or [`crate::generate_gear_program_module`])
//! you'd need a configs bundle - type which implements [`ConfigsBundle`].
//!
//! There's a pre-defined one - [`StandardGearWasmConfigsBundle`], usage of which will result
//! in generation of valid (always) gear-wasm module.

use std::num::{NonZeroU32, NonZeroUsize};

mod generator;
mod module;
mod syscalls;

use arbitrary::Unstructured;
pub use generator::*;
pub use module::*;
pub use syscalls::*;

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

/// The fully controllable implementation of ConfigsBundle.
impl ConfigsBundle for (GearWasmGeneratorConfig, SelectableParams) {
    fn into_parts(self) -> (GearWasmGeneratorConfig, SelectableParams) {
        self
    }
}

/// Standard set of configurational data which is used to generate always
/// valid gear-wasm using generators of the current crate.
#[derive(Debug, Clone)]
pub struct StandardGearWasmConfigsBundle {
    /// Externalities to be logged.
    pub log_info: Option<String>,
    /// Probability of wait syscalls.
    ///
    /// For example, if this parameter is 4, wait syscalls will be invoked
    /// with probability 1/4.
    pub waiting_probability: Option<NonZeroU32>,
    /// Flag which signals whether recursions must be removed.
    pub remove_recursion: bool,
    /// If the limit is set to `Some(_)`, programs will try to stop execution
    /// after reaching a critical gas limit, which can be useful to exit from
    /// heavy loops and recursions that waste all gas.
    ///
    /// The `gr_gas_available` syscall is called at the beginning of each
    /// function and for each control instruction (blocks, loops, conditions).
    pub critical_gas_limit: Option<u64>,
    /// Injection type for each syscall.
    pub injection_types: SyscallsInjectionTypes,
    /// Config of gear wasm call entry-points (exports).
    pub entry_points_set: EntryPointsSet,
    /// Initial wasm memory pages.
    pub initial_pages: u32,
    /// Optional stack end pages.
    pub stack_end_page: Option<u32>,
    /// Syscalls params config
    pub params_config: SyscallsParamsConfig,
}

impl Default for StandardGearWasmConfigsBundle {
    fn default() -> Self {
        Self {
            log_info: Some("StandardGearWasmConfigsBundle".into()),
            waiting_probability: NonZeroU32::new(4),
            remove_recursion: false,
            critical_gas_limit: Some(1_000_000),
            injection_types: SyscallsInjectionTypes::all_once(),
            entry_points_set: Default::default(),
            initial_pages: DEFAULT_INITIAL_SIZE,
            stack_end_page: None,
            params_config: SyscallsParamsConfig::default(),
        }
    }
}

impl ConfigsBundle for StandardGearWasmConfigsBundle {
    fn into_parts(self) -> (GearWasmGeneratorConfig, SelectableParams) {
        let StandardGearWasmConfigsBundle {
            log_info,
            waiting_probability,
            remove_recursion,
            critical_gas_limit,
            injection_types,
            entry_points_set,
            initial_pages,
            stack_end_page,
            params_config,
        } = self;

        let selectable_params = SelectableParams::default();

        let mut syscalls_config_builder = SyscallsConfigBuilder::new(injection_types);
        if let Some(log_info) = log_info {
            syscalls_config_builder = syscalls_config_builder.with_log_info(log_info);
        }
        if let Some(waiting_probability) = waiting_probability {
            syscalls_config_builder =
                syscalls_config_builder.with_waiting_probability(waiting_probability);
        }
        syscalls_config_builder = syscalls_config_builder.with_params_config(params_config);

        let memory_pages_config = MemoryPagesConfig {
            initial_size: initial_pages,
            stack_end_page,
            upper_limit: None,
        };
        let gear_wasm_generator_config = GearWasmGeneratorConfigBuilder::new()
            .with_critical_gas_limit(critical_gas_limit)
            .with_recursions_removed(remove_recursion)
            .with_syscalls_config(syscalls_config_builder.build())
            .with_entry_points_config(entry_points_set)
            .with_memory_config(memory_pages_config)
            .build();

        (gear_wasm_generator_config, selectable_params)
    }
}

pub struct RandomizedGearWasmConfigBundle {
    pub max_instructions: usize,
    pub no_control: bool,
    pub min_funcs: usize,
    pub max_funcs: usize,
    pub standard_gear_wasm_config_bundle: StandardGearWasmConfigsBundle,
}

impl ConfigsBundle for RandomizedGearWasmConfigBundle {
    fn into_parts(self) -> (GearWasmGeneratorConfig, SelectableParams) {
        let RandomizedGearWasmConfigBundle {
            max_funcs,
            max_instructions,
            min_funcs,
            no_control,
            standard_gear_wasm_config_bundle:
                StandardGearWasmConfigsBundle {
                    critical_gas_limit,
                    entry_points_set,
                    initial_pages,
                    injection_types,
                    log_info,
                    params_config,
                    remove_recursion,
                    stack_end_page,
                    waiting_probability,
                },
        } = self;

        let mut selectable_params = SelectableParams::default();

        selectable_params.max_instructions = max_instructions;
        selectable_params.max_funcs = NonZeroUsize::new(max_funcs).unwrap();
        selectable_params.min_funcs = NonZeroUsize::new(min_funcs).unwrap();
        if no_control {
            let index = selectable_params
                .allowed_instructions
                .iter()
                .position(|x| x == &InstructionKind::Control)
                .unwrap();
            selectable_params.allowed_instructions.remove(index);
        }

        let mut syscalls_config_builder = SyscallsConfigBuilder::new(injection_types)
            .with_log_info("RandomizedGearWasmConfigBuilder".to_owned())
            .with_waiting_probability(waiting_probability.unwrap());

        syscalls_config_builder = syscalls_config_builder.with_params_config(params_config);

        let memory_pages_config = MemoryPagesConfig {
            initial_size: initial_pages,
            stack_end_page: stack_end_page,
            upper_limit: None,
        };
        let gear_wasm_generator_config = GearWasmGeneratorConfigBuilder::new()
            .with_critical_gas_limit(critical_gas_limit)
            .with_recursions_removed(remove_recursion)
            .with_syscalls_config(syscalls_config_builder.build())
            .with_entry_points_config(entry_points_set)
            .with_memory_config(memory_pages_config)
            .build();

        (gear_wasm_generator_config, selectable_params)
    }
}
