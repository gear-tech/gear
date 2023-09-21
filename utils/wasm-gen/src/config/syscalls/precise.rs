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

//! Entities describing additional data for precise sys-call.

use gear_wasm_instrument::syscalls::SysCallName;
use std::{collections::HashMap, ops::RangeInclusive};

/// Additional data for precise sys-calls.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreciseSysCallAdditionalData {
    Nothing,
    Range(RangeInclusive<usize>),
}

/// Possible additional data for each precise sys-call.
/// Can be used to write unit tests so you don't have to rely on randomness.
#[derive(Debug, Clone)]
pub struct SysCallsPreciseConfig(HashMap<SysCallName, PreciseSysCallAdditionalData>);

impl SysCallsPreciseConfig {
    /// Create a new sys-calls precise config filled with the given values.
    pub fn new(range: RangeInclusive<usize>) -> Self {
        Self(
            [
                (
                    SysCallName::SendCommit,
                    PreciseSysCallAdditionalData::Range(range.clone()),
                ),
                (
                    SysCallName::SendCommitWGas,
                    PreciseSysCallAdditionalData::Range(range),
                ),
            ]
            .into_iter()
            .collect(),
        )
    }

    /// Get additional data for sys-call.
    pub fn get(&self, name: SysCallName) -> PreciseSysCallAdditionalData {
        self.0
            .get(&name)
            .cloned()
            .unwrap_or(PreciseSysCallAdditionalData::Nothing)
    }

    /// Set additional data for sys-call.
    pub fn set(&mut self, name: SysCallName, additional_data: PreciseSysCallAdditionalData) {
        self.0.insert(name, additional_data);
    }
}

impl Default for SysCallsPreciseConfig {
    fn default() -> Self {
        Self::new(0..=3)
    }
}
