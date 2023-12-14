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

//! Configuration for the syscalls imports generator, additional data injector
//! and syscalls invocations generator.

mod injection;
mod param;
mod precise;
mod process_errors;

use gear_utils::NonEmpty;
use gear_wasm_instrument::syscalls::SyscallName;
use gsys::{Hash, HashWithValue};

pub use injection::*;
pub use param::*;
pub use precise::*;
pub use process_errors::*;

use crate::InvocableSyscall;

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
            syscall_destination: SyscallDestination::default(),
            error_processing_config: ErrorProcessingConfig::None,
            log_info: None,
        })
    }

    /// Set config for syscalls params.
    pub fn with_params_config(mut self, params_config: SyscallsParamsConfig) -> Self {
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

    /// Set whether syscalls with destination param (like `gr_*send*` or `gr_exit`) must use `gr_source` syscall result for a destination param.
    pub fn with_source_msg_dest(mut self) -> Self {
        self.0.syscall_destination = SyscallDestination::Source;
        self.0
            .injection_types
            .enable_syscall_import(InvocableSyscall::Loose(SyscallName::Source));

        self
    }

    /// Set whether syscalls with destination param (like `gr_*send*` or `gr_exit`) must use addresses from `addresses` collection
    /// for a destination param.
    pub fn with_addresses_msg_dest<T: Into<Hash>>(mut self, addresses: NonEmpty<T>) -> Self {
        let addresses = NonEmpty::collect(addresses.into_iter().map(|pid| HashWithValue {
            hash: pid.into(),
            value: 0,
        }))
        .expect("collected from non empty");
        self.0.syscall_destination = SyscallDestination::ExistingAddresses(addresses);

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

    /// Setup fallible syscalls error processing options.
    pub fn with_error_processing_config(mut self, config: ErrorProcessingConfig) -> Self {
        self.0.error_processing_config = config;

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
    syscall_destination: SyscallDestination,
    error_processing_config: ErrorProcessingConfig,
    log_info: Option<String>,
}

impl SyscallsConfig {
    /// Get possible number of times (range) the syscall can be injected in the wasm.
    pub fn injection_types(&self, name: InvocableSyscall) -> SyscallInjectionType {
        self.injection_types.get(name)
    }

    /// Get defined syscall destination for `gr_send*` and `gr_exit` syscalls.
    ///
    /// For more info, read [`SyscallDestination`].
    pub fn syscall_destination(&self) -> &SyscallDestination {
        &self.syscall_destination
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

    /// Error processing config for fallible syscalls.
    pub fn error_processing_config(&self) -> &ErrorProcessingConfig {
        &self.error_processing_config
    }
}

/// Syscall destination choice.
///
/// `gr_send*` and `gr_exit` syscalls generated from this crate can be sent
/// to different destination in accordance to the config.
/// It's either to the message source, to some existing known address,
/// or to some random, most probably non-existing, address.
#[derive(Debug, Clone, Default)]
pub enum SyscallDestination {
    Source,
    ExistingAddresses(NonEmpty<HashWithValue>),
    #[default]
    Random,
}

impl SyscallDestination {
    /// Check whether syscall destination is a result of `gr_source`.
    pub fn is_source(&self) -> bool {
        matches!(&self, SyscallDestination::Source)
    }

    /// Check whether syscall destination is defined randomly.
    pub fn is_random(&self) -> bool {
        matches!(&self, SyscallDestination::Random)
    }

    /// Check whether syscall destination is defined from a collection of existing addresses.
    pub fn is_existing_addresses(&self) -> bool {
        matches!(&self, SyscallDestination::ExistingAddresses(_))
    }
}
