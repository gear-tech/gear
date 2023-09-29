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

//! Entities describing possible injection types for each sys-call.
//!
//! Types here are used to create [`crate::SysCallsConfig`].

use crate::InvocableSysCall;

use gear_wasm_instrument::syscalls::SysCallName;
use std::{collections::HashMap, ops::RangeInclusive};

/// Possible injection type.
#[derive(Debug, Clone)]
pub enum InjectionType {
    /// Don't modify anything at all.
    None,
    /// Inject import into wasm module.
    Import,
    /// Inject import and function call into wasm module.
    Function(RangeInclusive<u32>),
}

/// Possible injection types for each sys-call.
#[derive(Debug, Clone)]
pub struct SysCallsInjectionAmounts(HashMap<InvocableSysCall, InjectionType>);

impl SysCallsInjectionAmounts {
    /// Instantiate a sys-calls map, where each gear sys-call is injected into wasm-module only once.
    pub fn all_once() -> Self {
        Self::new_with_injection_type(InjectionType::Function(1..=1))
    }

    /// Instantiate a sys-calls map, where no gear sys-call is ever injected into wasm-module.
    pub fn all_never() -> Self {
        Self::new_with_injection_type(InjectionType::None)
    }

    /// Instantiate a sys-calls map with given injection type.
    fn new_with_injection_type(injection_type: InjectionType) -> Self {
        let sys_calls = SysCallName::instrumentable();
        Self(
            sys_calls
                .iter()
                .cloned()
                .map(|name| (InvocableSysCall::Loose(name), injection_type.clone()))
                .chain(sys_calls.iter().cloned().filter_map(|name| {
                    InvocableSysCall::has_precise_variant(name)
                        .then_some((InvocableSysCall::Precise(name), injection_type.clone()))
                }))
                .collect(),
        )
    }

    /// Gets injection type for given sys-call.
    pub fn get(&self, name: InvocableSysCall) -> InjectionType {
        self.0
            .get(&name)
            .cloned()
            .expect("instantiated with all sys-calls set")
    }

    /// Sets possible amount range for the the sys-call.
    pub fn set(&mut self, name: InvocableSysCall, min: u32, max: u32) {
        self.0.insert(name, InjectionType::Function(min..=max));

        if let InvocableSysCall::Precise(sys_call) = name {
            let Some(required_imports) = InvocableSysCall::required_imports_for_sys_call(sys_call) else {
                return;
            };

            for &sys_call_import in required_imports {
                self.enable_sys_call(InvocableSysCall::Loose(sys_call_import));
            }
        }
    }

    /// Enables given sys-call.
    pub fn enable_sys_call(&mut self, name: InvocableSysCall) {
        if let Some(injection_type @ InjectionType::None) = self.0.get_mut(&name) {
            *injection_type = InjectionType::Import;
        }
    }

    /// Same as [`SysCallsInjectionAmounts::set`], but sets amount ranges for multiple sys-calls.
    pub fn set_multiple(
        &mut self,
        sys_calls_freqs: impl Iterator<Item = (InvocableSysCall, RangeInclusive<u32>)>,
    ) {
        for (name, range) in sys_calls_freqs {
            let (min, max) = range.into_inner();
            self.set(name, min, max);
        }
    }
}

impl Default for SysCallsInjectionAmounts {
    fn default() -> Self {
        Self::all_once()
    }
}
