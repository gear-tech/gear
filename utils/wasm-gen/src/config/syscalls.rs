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

//! Configuration for the sys-calls imports generator, additional data injector
//! and sys-calls invocations generator.

mod injection;
mod param;
mod precise;

use gear_utils::NonEmpty;
use gear_wasm_instrument::syscalls::SysCallName;
use gsys::{Hash, HashWithValue};
use std::collections::HashSet;

pub use injection::*;
pub use param::*;
pub use precise::*;

use crate::InvocableSysCall;

/// Builder for [`SysCallsConfig`].
#[derive(Debug, Clone)]
pub struct SysCallsConfigBuilder(SysCallsConfig);

impl SysCallsConfigBuilder {
    /// Create a new builder with defined injection amounts for all sys-calls.
    pub fn new(injection_types: SysCallsInjectionTypes) -> Self {
        Self(SysCallsConfig {
            injection_types,
            params_config: SysCallsParamsConfig::default(),
            precise_syscalls_config: PreciseSysCallsConfig::default(),
            sys_call_destination: SysCallDestination::default(),
            error_processing_config: ErrorProcessingConfig::None,
            log_info: None,
        })
    }

    /// Set config for sys-calls params.
    pub fn with_params_config(mut self, params_config: SysCallsParamsConfig) -> Self {
        self.0.params_config = params_config;

        self
    }

    /// Set config for precise sys-calls.
    pub fn with_precise_syscalls_config(
        mut self,
        precise_syscalls_config: PreciseSysCallsConfig,
    ) -> Self {
        self.0.precise_syscalls_config = precise_syscalls_config;

        self
    }

    /// Set whether `gr_send*` and `gr_exit` sys-calls must use `gr_source` result for sys-call destination.
    pub fn with_source_msg_dest(mut self) -> Self {
        self.0.sys_call_destination = SysCallDestination::Source;
        self.0
            .injection_types
            .enable_sys_call(InvocableSysCall::Loose(SysCallName::Source));

        self
    }

    /// Set whether `gr_send*` and `gr_exit` sys-calls must use some address from `addresses` collection
    /// as a sys-call destination.
    pub fn with_data_offset_msg_dest<T: Into<Hash>>(mut self, addresses: NonEmpty<T>) -> Self {
        let addresses = NonEmpty::collect(addresses.into_iter().map(|pid| HashWithValue {
            hash: pid.into(),
            value: 0,
        }))
        .expect("collected from non empty");
        self.0.sys_call_destination = SysCallDestination::ExistingAddresses(addresses);

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
            .enable_sys_call(InvocableSysCall::Loose(SysCallName::Debug));

        self
    }

    /// Setup fallible syscalls error processing options.
    pub fn set_error_processing_config(mut self, config: ErrorProcessingConfig) -> Self {
        self.0.error_processing_config = config;

        self
    }

    /// Build the [`SysCallsConfig`].
    pub fn build(self) -> SysCallsConfig {
        self.0
    }
}

#[derive(Debug, Clone, Default)]
pub enum ErrorProcessingConfig {
    /// Process errors on all the fallible syscalls.
    All,
    /// Process only errors on provided syscalls.
    Whitelist(HashSet<InvocableSysCall>),
    /// Process errors on all the syscalls excluding provided.
    Blacklist(HashSet<InvocableSysCall>),
    /// Don't process syscall errors at all.
    #[default]
    None,
}

impl ErrorProcessingConfig {
    pub fn error_should_be_processed(&self, syscall: &InvocableSysCall) -> bool {
        match self {
            Self::All => true,
            Self::Whitelist(wl) => wl.contains(syscall),
            Self::Blacklist(bl) => !bl.contains(syscall),
            Self::None => false,
        }
    }
}

/// United config for all entities in sys-calls generator module.
#[derive(Debug, Clone, Default)]
pub struct SysCallsConfig {
    injection_types: SysCallsInjectionTypes,
    params_config: SysCallsParamsConfig,
    precise_syscalls_config: PreciseSysCallsConfig,
    sys_call_destination: SysCallDestination,
    error_processing_config: ErrorProcessingConfig,
    log_info: Option<String>,
}

impl SysCallsConfig {
    /// Get possible number of times (range) the sys-call can be injected in the wasm.
    pub fn injection_types(&self, name: InvocableSysCall) -> SysCallInjectionType {
        self.injection_types.get(name)
    }

    /// Get defined sys-call destination for `gr_send*` and `gr_exit` sys-calls.
    ///
    /// For more info, read [`SysCallDestination`].
    pub fn sys_call_destination(&self) -> &SysCallDestination {
        &self.sys_call_destination
    }

    /// Get defined log info.
    ///
    /// For more info, read [`SysCallsConfigBuilder::with_log_info`].
    pub fn log_info(&self) -> Option<&String> {
        self.log_info.as_ref()
    }

    /// Get sys-calls params config.
    pub fn params_config(&self) -> &SysCallsParamsConfig {
        &self.params_config
    }

    /// Get precise sys-calls config.
    pub fn precise_syscalls_config(&self) -> &PreciseSysCallsConfig {
        &self.precise_syscalls_config
    }

    /// Error processing config for fallible syscalls.
    pub fn error_processing_config(&self) -> &ErrorProcessingConfig {
        &self.error_processing_config
    }
}

/// Sys-call destination choice.
///
/// `gr_send*` and `gr_exit` sys-calls generated from this crate can be sent
/// to different destination in accordance to the config.
/// It's either to the message source, to some existing known address,
/// or to some random, most probably non-existing, address.
#[derive(Debug, Clone, Default)]
pub enum SysCallDestination {
    Source,
    ExistingAddresses(NonEmpty<HashWithValue>),
    #[default]
    Random,
}

impl SysCallDestination {
    /// Check whether sys-call destination is a result of `gr_source`.
    pub fn is_source(&self) -> bool {
        matches!(&self, SysCallDestination::Source)
    }

    /// Check whether sys-call destination is defined randomly.
    pub fn is_random(&self) -> bool {
        matches!(&self, SysCallDestination::Random)
    }

    /// Check whether sys-call destination is defined from a collection of existing addresses.
    pub fn is_existing_addresses(&self) -> bool {
        matches!(&self, SysCallDestination::ExistingAddresses(_))
    }
}
