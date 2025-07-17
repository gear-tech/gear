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

//! Configuration for the syscalls imports generator, additional data injector
//! and syscalls invocations generator.

use crate::InvocableSyscall;
use gear_wasm_instrument::syscalls::SyscallName;
use std::num::NonZero;

mod injection;
mod param;
mod precise;
mod process_errors;

pub use injection::*;
pub use param::*;
pub use precise::*;
pub use process_errors::*;

/// Builder for [`SyscallsConfig`].
#[derive(Debug, Clone)]
pub struct SyscallsConfigBuilder(SyscallsConfig);

impl SyscallsConfigBuilder {
    /// Create a new builder with defined injection amounts for all syscalls.
    pub fn new(injection_types: SyscallsInjectionTypes) -> Self {
        Self(SyscallsConfig {
            injection_types,
            params_config: SyscallsParamsConfig::default(),
            precise_syscalls_config: PreciseSyscallsConfig::default(),
            error_processing_config: ErrorProcessingConfig::None,
            log_info: None,
            waiting_probability: None,
            keeping_insertion_order: false,
        })
    }

    /// Set config for syscalls params.
    pub fn with_params_config(mut self, params_config: SyscallsParamsConfig) -> Self {
        use PtrParamAllowedValues::*;
        for v in params_config.ptr.values() {
            if let ActorId(actor)
            | ActorIdWithValue {
                actor_kind: actor, ..
            } = v
                && actor.is_source()
            {
                self.0
                    .injection_types
                    .enable_syscall_import(InvocableSyscall::Loose(SyscallName::Source));
            }
        }

        self.0.params_config = params_config;

        self
    }

    /// Set config for precise syscalls.
    pub fn with_precise_syscalls_config(
        mut self,
        precise_syscalls_config: PreciseSyscallsConfig,
    ) -> Self {
        self.0.precise_syscalls_config = precise_syscalls_config;

        self
    }

    /// Set whether some externalities must be logged in the gear export (entry point)
    /// function.
    ///
    /// Choosing gear export to log data is done from best `init` to worse `handle`.
    pub fn with_log_info(mut self, log: String) -> Self {
        self.0.log_info = Some(log);
        self.0
            .injection_types
            .enable_syscall_import(InvocableSyscall::Loose(SyscallName::Debug));

        self
    }

    /// Set probability of wait syscalls.
    pub fn with_waiting_probability(mut self, waiting_probability: NonZero<u32>) -> Self {
        self.0.waiting_probability = Some(waiting_probability);

        self
    }

    /// Setup fallible syscalls error processing options.
    pub fn with_error_processing_config(mut self, config: ErrorProcessingConfig) -> Self {
        self.0.error_processing_config = config;

        self
    }

    /// Set whether to keep insertion order of syscalls.
    ///
    /// Useful for testing with given order of execution of syscalls.
    pub fn with_keeping_insertion_order(mut self, keeping_insertion_order: bool) -> Self {
        self.0.keeping_insertion_order = keeping_insertion_order;
        self
    }

    /// Build the [`SyscallsConfig`].
    pub fn build(self) -> SyscallsConfig {
        self.0
    }
}

/// United config for all entities in syscalls generator module.
#[derive(Debug, Clone, Default)]
pub struct SyscallsConfig {
    injection_types: SyscallsInjectionTypes,
    params_config: SyscallsParamsConfig,
    precise_syscalls_config: PreciseSyscallsConfig,
    error_processing_config: ErrorProcessingConfig,
    log_info: Option<String>,
    waiting_probability: Option<NonZero<u32>>,
    keeping_insertion_order: bool,
}

impl SyscallsConfig {
    /// Get injection types.
    pub fn injection_types(&self) -> &SyscallsInjectionTypes {
        &self.injection_types
    }

    /// Get possible number of times (range) the syscall can be injected in the wasm.
    pub fn injection_type(&self, name: InvocableSyscall) -> SyscallInjectionType {
        self.injection_types.get(name)
    }

    /// Get defined log info.
    ///
    /// For more info, read [`SyscallsConfigBuilder::with_log_info`].
    pub fn log_info(&self) -> Option<&String> {
        self.log_info.as_ref()
    }

    /// Get syscalls params config.
    pub fn params_config(&self) -> &SyscallsParamsConfig {
        &self.params_config
    }

    /// Get precise syscalls config.
    pub fn precise_syscalls_config(&self) -> &PreciseSyscallsConfig {
        &self.precise_syscalls_config
    }

    /// Get error processing config for fallible syscalls.
    pub fn error_processing_config(&self) -> &ErrorProcessingConfig {
        &self.error_processing_config
    }

    /// Get probability of wait syscalls.
    pub fn waiting_probability(&self) -> Option<NonZero<u32>> {
        self.waiting_probability
    }

    /// Get whether to keep insertion order of syscalls.
    pub fn keeping_insertion_order(&self) -> bool {
        self.keeping_insertion_order
    }
}
