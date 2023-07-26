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

//! Entities describing possible amount for each sys-call to be injected into wasm module.
//!
//! Types here are used to create [`crate::SysCallsConfig`].

use gear_wasm_instrument::syscalls::SysCallName;
use std::{collections::BTreeMap, ops::RangeInclusive};

/// Possible amount ranges for each sys-call.
#[derive(Debug, Clone)]
pub struct SysCallsAmountRanges(BTreeMap<SysCallName, RangeInclusive<u32>>);

impl SysCallsAmountRanges {
    /// Instantiate a sys-calls amounts ranges map, where each gear sys-call is injected into wasm-module only once.
    pub fn all_once() -> Self {
        Self(
            SysCallName::instrumentable()
                .into_iter()
                .map(|name| (name, (1..=1)))
                .collect(),
        )
    }

    /// Instantiate a sys-calls amounts ranges map, where no gear sys-call is ever injected into wasm-module.
    pub fn all_never() -> Self {
        Self(
            SysCallName::instrumentable()
                .into_iter()
                .map(|name| (name, (0..=0)))
                .collect(),
        )
    }

    /// Get amount possible sys-call amount range.
    pub fn get(&self, name: SysCallName) -> RangeInclusive<u32> {
        self.0
            .get(&name)
            .cloned()
            .expect("instantiated with all sys-calls set")
    }

    /// Same as [`SysCallsAmountRanges::set`], but consumes the type and returns
    /// it updated.
    pub fn with(mut self, name: SysCallName, min: u32, max: u32) -> Self {
        self.set(name, min, max);

        self
    }

    /// Sets possible amount range for the the sys-call.
    pub fn set(&mut self, name: SysCallName, min: u32, max: u32) {
        self.0.insert(name, min..=max);
    }

    ///  Same as [`SysCallsAmountRanges::set`], but sets amount ranges for multiple sys-calls.
    pub fn set_multiple(
        &mut self,
        sys_calls_freqs: impl Iterator<Item = (SysCallName, RangeInclusive<u32>)>,
    ) {
        self.0.extend(sys_calls_freqs)
    }
}

impl Default for SysCallsAmountRanges {
    fn default() -> Self {
        Self::all_once()
    }
}
