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

//! Configs related to instantiation of gear wasm module generators.

use crate::SysCallsConfig;

/// Builder for [`GearWasmGeneratorConfig`].
pub struct GearWasmGeneratorConfigBuilder(GearWasmGeneratorConfig);

impl GearWasmGeneratorConfigBuilder {
    #[allow(clippy::new_without_default)]
    /// Create a new builder.
    pub fn new() -> Self {
        Self(GearWasmGeneratorConfig::default())
    }

    /// Defines memory pages confir for the gear wasm generator.
    pub fn with_memory_config(mut self, mem_config: MemoryPagesConfig) -> Self {
        self.0.memory_config = mem_config;

        self
    }

    /// Defines entry points (gear exports) config for the gear wasm generator.
    pub fn with_entry_points_config(mut self, ep_config: EntryPointsSet) -> Self {
        self.0.entry_points_config = ep_config;

        self
    }

    /// Defines sys-calls config for the gear wasm generator.
    pub fn with_sys_calls_config(mut self, sys_calls_config: SysCallsConfig) -> Self {
        self.0.sys_calls_config = sys_calls_config;

        self
    }

    /// Defines whether recursions must be removed from the resulting gear wasm.
    pub fn with_recursions_removed(mut self, remove_recursions: bool) -> Self {
        self.0.remove_recursions = remove_recursions;

        self
    }

    /// Build the gear wasm generator.
    pub fn build(self) -> GearWasmGeneratorConfig {
        self.0
    }
}

/// Gear wasm generator config.
///
/// This is a carrier for other configs, that can be used separately
/// in corresponding generators.
#[derive(Debug, Clone, Default)]
pub struct GearWasmGeneratorConfig {
    /// Memory pages config.
    pub memory_config: MemoryPagesConfig,
    /// Entry points config.
    pub entry_points_config: EntryPointsSet,
    /// Sys-calls generator module config.
    pub sys_calls_config: SysCallsConfig,
    /// Flag, signalizing whether recursions
    /// should be removed from resulting module.
    pub remove_recursions: bool,
}

/// Memory pages config used by [`crate::MemoryGenerator`].
#[derive(Debug, Clone, Copy)]
pub struct MemoryPagesConfig {
    /// Initial memory size.
    pub initial_size: u32,
    /// Optional memory maximum.
    pub upper_limit: Option<u32>,
    /// Optional stack end page.
    pub stack_end_page: Option<u32>,
}

impl Default for MemoryPagesConfig {
    fn default() -> Self {
        Self {
            initial_size: Self::MAX_VALUE / 2 + 5,
            upper_limit: Some(Self::MAX_VALUE),
            stack_end_page: Some(Self::MAX_VALUE / 2),
        }
    }
}

impl MemoryPagesConfig {
    /// Default maximum memory pages.
    pub const MAX_VALUE: u32 = 512;
}

/// Possible for current crate gear entry points
/// to be generated.
#[derive(Debug, Clone, Copy)]
pub enum EntryPointName {
    Init,
    Handle,
    HandleReply,
}

impl EntryPointName {
    /// Convert current entry point to str.
    pub fn to_str(&self) -> &'static str {
        match self {
            EntryPointName::Init => "init",
            EntryPointName::Handle => "handle",
            EntryPointName::HandleReply => "handle_reply",
        }
    }
}

/// Entry points config used by [`crate::EntryPointsGenerator`].
///
/// It's literally all possible combinations of gear entry points
/// to be generated in the wasm by [`crate::EntryPointsGenerator`].
#[derive(Debug, Clone, Copy, Default)]
pub enum EntryPointsSet {
    #[default]
    Init,
    InitHandle,
    InitHandleReply,
    InitHandleHandleReply,
    Handle,
    HandleHandleReply,
}

impl EntryPointsSet {
    /// Checks whether the set has ***init*** entry point.
    pub fn has_init(&self) -> bool {
        matches!(
            self,
            EntryPointsSet::Init
                | EntryPointsSet::InitHandle
                | EntryPointsSet::InitHandleReply
                | EntryPointsSet::InitHandleHandleReply
        )
    }

    /// Checks whether the set has ***handle*** entry point.
    pub fn has_handle(&self) -> bool {
        matches!(
            self,
            EntryPointsSet::InitHandle
                | EntryPointsSet::InitHandleHandleReply
                | EntryPointsSet::Handle
                | EntryPointsSet::HandleHandleReply
        )
    }

    /// Checks whether the set has ***handle_reply*** entry point.
    pub fn has_handle_reply(&self) -> bool {
        matches!(
            self,
            EntryPointsSet::InitHandleReply
                | EntryPointsSet::InitHandleHandleReply
                | EntryPointsSet::HandleHandleReply
        )
    }
}
