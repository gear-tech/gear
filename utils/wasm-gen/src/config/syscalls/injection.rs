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

//! Entities describing possible injection types for each syscall.
//! These entities allows to configure which syscalls to insert into
//! code section of wasm module and which ones to simply import.
//!
//! Types here are used to create [`crate::SysCallsConfig`].

use crate::InvocableSysCall;

use gear_wasm_instrument::syscalls::SysCallName;
use std::{collections::HashMap, ops::RangeInclusive};

/// This enum defines how the syscall should be injected into wasm module.
#[derive(Debug, Clone)]
pub enum SysCallInjectionType {
    /// Don't modify wasm module at all.
    None,
    /// Sys-call import will be injected into import section of wasm module,
    /// but the `wasm-gen` generators will not call that syscall.
    ///
    /// It should be used in cases where you don't need to invoke an actual syscall.
    /// For example, `precision_gr_reservation_send` syscall uses `gr_reserve_gas` under
    /// the hood. In this case, `gr_reserve_gas` will be imported but will not be called.
    Import,
    /// Sys-call import will be injected into import section of wasm module,
    /// and the `wasm-gen` generators will insert invoke instructions for that syscall.
    ///
    /// It also has `syscall_amount_range: RangeInclusive<u32>` - the range from which
    /// amount of syscalls will be generated for injection into code section of wasm module.
    Function(RangeInclusive<u32>),
}

/// Possible injection types for each syscall.
#[derive(Debug, Clone)]
pub struct SysCallsInjectionTypes(HashMap<InvocableSysCall, SysCallInjectionType>);

impl SysCallsInjectionTypes {
    /// Instantiate a syscalls map, where each gear syscall is injected into wasm-module only once.
    pub fn all_once() -> Self {
        Self::new_with_injection_type(SysCallInjectionType::Function(1..=1))
    }

    /// Instantiate a syscalls map, where no gear syscall is ever injected into wasm-module.
    pub fn all_never() -> Self {
        Self::new_with_injection_type(SysCallInjectionType::None)
    }

    /// Instantiate a syscalls map with given injection type.
    fn new_with_injection_type(injection_type: SysCallInjectionType) -> Self {
        let syscalls = SysCallName::instrumentable();
        Self(
            syscalls
                .iter()
                .cloned()
                .map(|name| (InvocableSysCall::Loose(name), injection_type.clone()))
                .chain(syscalls.iter().cloned().filter_map(|name| {
                    InvocableSysCall::has_precise_variant(name)
                        .then_some((InvocableSysCall::Precise(name), injection_type.clone()))
                }))
                .collect(),
        )
    }

    /// Gets injection type for given syscall.
    pub fn get(&self, name: InvocableSysCall) -> SysCallInjectionType {
        self.0
            .get(&name)
            .cloned()
            .expect("instantiated with all syscalls set")
    }

    /// Sets possible amount range for the the syscall.
    pub fn set(&mut self, name: InvocableSysCall, min: u32, max: u32) {
        self.0
            .insert(name, SysCallInjectionType::Function(min..=max));

        if let InvocableSysCall::Precise(syscall) = name {
            let Some(required_imports) = InvocableSysCall::required_imports_for_syscall(syscall) else {
                return;
            };

            for &syscall_import in required_imports {
                println!("IMPORT REQUIRED {}", syscall_import.to_str());
                self.enable_syscall_import(InvocableSysCall::Loose(syscall_import));
            }
        }
    }

    /// Imports the given syscall if necessary.
    pub(crate) fn enable_syscall_import(&mut self, name: InvocableSysCall) {
        if let Some(injection_type @ SysCallInjectionType::None) = self.0.get_mut(&name) {
            *injection_type = SysCallInjectionType::Import;
        } else {
            todo!("!!!");
        }
    }

    /// Same as [`SysCallsInjectionTypes::set`], but sets amount ranges for multiple syscalls.
    pub fn set_multiple(
        &mut self,
        syscalls_freqs: impl Iterator<Item = (InvocableSysCall, RangeInclusive<u32>)>,
    ) {
        for (name, range) in syscalls_freqs {
            let (min, max) = range.into_inner();
            self.set(name, min, max);
        }
    }
}

impl Default for SysCallsInjectionTypes {
    fn default() -> Self {
        Self::all_once()
    }
}
