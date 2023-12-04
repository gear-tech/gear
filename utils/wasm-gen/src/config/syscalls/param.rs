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

//! Entities describing syscall param, more precisely, it's allowed values.
//!
//! Types here are used to create [`crate::SyscallsConfig`].

use crate::DEFAULT_INITIAL_SIZE;
use arbitrary::{Result, Unstructured};
use std::{collections::HashMap, ops::RangeInclusive};

pub use gear_wasm_instrument::syscalls::{ParamType, RegularParamType};

/// Syscalls params config.
///
/// This is basically a map, which creates a relationship between each kind of
/// param, that a syscall can have, and allowed values ("rules") for each of
/// the params.
///
/// # Note:
///
/// Configs with some [`ParamType`] variants will not be applied, as we select
/// values for all memory-related operations in accordance to generated WASM
/// module parameters:
///  - [`ParamType::Alloc`] and [`ParamType::Ptr`] will always be ignored.
///  - [`ParamType::Size`] will be ignored when it means length of some in-memory
/// array.
#[derive(Debug, Clone)]
pub struct SyscallsParamsConfig(HashMap<ParamType, SyscallParamAllowedValues>);

impl SyscallsParamsConfig {
    pub fn empty() -> Self {
        Self(HashMap::new())
    }

    /// New [`SyscallsParamsConfig`] with all rules set to produce one constant value.
    pub fn all_constant_value(value: i64) -> Self {
        use ParamType::*;
        use RegularParamType::*;

        let allowed_values: SyscallParamAllowedValues = (value..=value).into();
        Self(
            [
                Regular(Length),
                Regular(Gas),
                Regular(Offset),
                Regular(DurationBlockNumber),
                Regular(DelayBlockNumber),
                Regular(Handler),
                Regular(Free),
                Regular(Version),
            ]
            .into_iter()
            .map(|param_type| (param_type, allowed_values.clone()))
            .collect(),
        )
    }

    /// Get allowed values for the `param`.
    pub fn get_rule(&self, param: &ParamType) -> Option<SyscallParamAllowedValues> {
        self.0.get(param).cloned()
    }

    /// Set allowed values for the `param`.
    pub fn add_rule(&mut self, param: ParamType, allowed_values: SyscallParamAllowedValues) {
        matches!(param, ParamType::Regular(RegularParamType::Pointer(_)))
            .then(|| panic!("ParamType::Ptr(..) isn't supported in SyscallsParamsConfig"));

        self.0.insert(param, allowed_values);
    }
}

impl Default for SyscallsParamsConfig {
    fn default() -> Self {
        use ParamType::*;
        use RegularParamType::*;

        let free_start = DEFAULT_INITIAL_SIZE as i64;
        let free_end = free_start + 5;
        Self(
            [
                (Regular(Length), (0..=0x10000).into()),
                // There are no rules for memory arrays and pointers as they are chosen
                // in accordance to memory pages config.
                (Regular(Gas), (0..=250_000_000_000).into()),
                (Regular(Offset), (0..=10).into()),
                (Regular(DurationBlockNumber), (1..=8).into()),
                (Regular(DelayBlockNumber), (0..=4).into()),
                (Regular(Handler), (0..=100).into()),
                (Regular(Free), (free_start..=free_end).into()),
                (Regular(Version), (1..=1).into()),
                (Regular(FreeUpperBound), (0..=10).into()),
            ]
            .into_iter()
            .collect(),
        )
    }
}

/// Range of allowed values for the syscall param.
#[derive(Debug, Clone)]
pub struct SyscallParamAllowedValues(RangeInclusive<i64>);

impl From<RangeInclusive<i64>> for SyscallParamAllowedValues {
    fn from(range: RangeInclusive<i64>) -> Self {
        Self(range)
    }
}

impl SyscallParamAllowedValues {
    /// Zero param value.
    ///
    /// That means that for particular param `0` will be always
    /// it's value.
    pub fn zero() -> Self {
        Self(0..=0)
    }

    /// Constant param value.
    ///
    /// That means that for particular param `value` will be always
    /// it's value.
    pub fn constant(value: i64) -> Self {
        Self(value..=value)
    }
}

impl Default for SyscallParamAllowedValues {
    fn default() -> Self {
        Self::zero()
    }
}

impl SyscallParamAllowedValues {
    /// Get i32 value for the param from it's allowed range.
    pub fn get_i32(&self, unstructured: &mut Unstructured) -> Result<i32> {
        let current_range_start = *self.0.start();
        let current_range_end = *self.0.end();

        let start = if current_range_start < i32::MIN as i64 {
            i32::MIN
        } else {
            current_range_start as i32
        };
        let end = if current_range_end > i32::MAX as i64 {
            i32::MAX
        } else {
            current_range_end as i32
        };

        unstructured.int_in_range(start..=end)
    }

    /// Get i64 value for the param from it's allowed range.
    pub fn get_i64(&self, unstructured: &mut Unstructured) -> Result<i64> {
        unstructured.int_in_range(self.0.clone())
    }
}
