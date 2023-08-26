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

mod amount;
mod param;

use gear_utils::NonEmpty;
use gear_wasm_instrument::syscalls::SysCallName;
use gsys::{Hash, HashWithValue};
use std::ops::RangeInclusive;

pub use amount::*;
pub use param::*;

/// Builder for [`SysCallsConfig`].
pub struct SysCallsConfigBuilder(SysCallsConfig);

impl SysCallsConfigBuilder {
    /// Create a new builder with defined injection amounts for all sys-calls.
    pub fn new(injection_amounts: SysCallsInjectionAmounts) -> Self {
        Self(SysCallsConfig {
            injection_amounts,
            params_config: SysCallsParamsConfig::default(),
            sending_message_destination: MessageDestination::default(),
            log_info: None,
        })
    }

    /// Set config for sys-calls params.
    pub fn with_params_config(mut self, params_config: SysCallsParamsConfig) -> Self {
        self.0.params_config = params_config;

        self
    }

    /// Set whether `gr_send*` sys-calls must use `gr_source` result for message destination.
    pub fn with_source_msg_dest(mut self) -> Self {
        self.0.sending_message_destination = MessageDestination::Source;
        self.enable_sys_call(SysCallName::Source);

        self
    }

    /// Set whether `gr_send*` sys-calls must use some address from `addresses` collection
    /// as a message destination.
    pub fn with_data_offset_msg_dest<T: Into<Hash>>(mut self, addresses: NonEmpty<T>) -> Self {
        let addresses = NonEmpty::collect(addresses.into_iter().map(|pid| HashWithValue {
            hash: pid.into(),
            value: 0,
        }))
        .expect("collected from non empty");
        self.0.sending_message_destination = MessageDestination::ExistingAddresses(addresses);

        self
    }

    /// Set whether some externalities must be logged in the gear export (entry point)
    /// function.
    ///
    /// Choosing gear export to log data is done from best `init` to worse `handle`.
    pub fn with_log_info(mut self, log: String) -> Self {
        self.0.log_info = Some(log);
        self.enable_sys_call(SysCallName::Debug);

        self
    }

    fn enable_sys_call(&mut self, name: SysCallName) {
        let range = self.0.injection_amounts.get(name);

        let range_start = *range.start();
        if range_start == 0 {
            let max = *range.end().max(&1);
            self.0.injection_amounts.set(name, 1, max);
        }
    }

    /// Build the [`SysCallsConfig`].
    pub fn build(self) -> SysCallsConfig {
        self.0
    }
}

/// United config for all entities in sys-calls generator module.
#[derive(Debug, Clone, Default)]
pub struct SysCallsConfig {
    injection_amounts: SysCallsInjectionAmounts,
    params_config: SysCallsParamsConfig,
    sending_message_destination: MessageDestination,
    log_info: Option<String>,
}

impl SysCallsConfig {
    /// Get possible number of times (range) the sys-call can be injected in the wasm.
    pub fn injection_amounts(&self, name: SysCallName) -> RangeInclusive<u32> {
        self.injection_amounts.get(name)
    }

    /// Get defined message destination for `gr_send*` sys-calls.
    ///
    /// For more info, read [`MessageDestination`].
    pub fn sending_message_destination(&self) -> &MessageDestination {
        &self.sending_message_destination
    }

    /// Get defined log info.
    ///
    /// For more info, read [`SysCallsConfigBuilder::with_log_info`].
    pub fn log_info(&self) -> Option<&String> {
        self.log_info.as_ref()
    }

    /// Gen sys-calls params config.
    pub fn params_config(&self) -> &SysCallsParamsConfig {
        &self.params_config
    }
}

/// Message destination choice.
///
/// `gr_send*` sys-calls generated from this crate can send messages
/// to different destination in accordance to the config.
/// It's either to the message source, to some existing known address,
/// or to some random, most probably non-existing, address.
#[derive(Debug, Clone, Default)]
pub enum MessageDestination {
    Source,
    ExistingAddresses(NonEmpty<HashWithValue>),
    #[default]
    Random,
}

impl MessageDestination {
    /// Check whether message destination is a result of `gr_source`.
    pub fn is_source(&self) -> bool {
        matches!(&self, MessageDestination::Source)
    }

    /// Check whether message destination is defined randomly.
    pub fn is_random(&self) -> bool {
        matches!(&self, MessageDestination::Random)
    }

    /// Check whether message destination is defined from a collection of existing addresses.
    pub fn is_existing_addresses(&self) -> bool {
        matches!(&self, MessageDestination::ExistingAddresses(_))
    }
}
