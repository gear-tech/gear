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
//! Or, for example, gear was generators config:
//! ```rust
//! use gear_wasm_gen::*;
//! let memory_pages_config = MemoryPagesConfig {
//!     initial_size: 128,
//!     upper_limit: None,
//!     stack_end: Some(64),
//! };
//! let entry_points_set = EntryPointsSet::InitHandle;
//! let use_random_memory_access_ptrs = true;
//! let sys_calls_config = SysCallsConfigBuilder::new(SysCallsAmountRanges::all_once(), use_random_memory_access_ptrs)
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

pub mod generator;
pub mod module;
pub mod syscalls;

pub use generator::*;
pub use module::*;
pub use syscalls::*;

/// United config for using the crate.
///
/// Uses [`SelectableParams`] instead of the [`WasmModuleConfig`], because
/// the former one provides all required from the crate user configurations
/// and all the other configurations are generated internally.
#[derive(Debug, Clone, Default)]
pub struct WasmGenConfig {
    pub generator_config: GearWasmGeneratorConfig,
    pub selectables_config: SelectableParams,
}
