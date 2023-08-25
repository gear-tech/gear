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

//! Entities describing sys-call param, more precisely, it's allowed values.
//!
//! Types here are used to create [`crate::SysCallsConfig`].

use arbitrary::{Result, Unstructured};
use gear_wasm_instrument::syscalls::ParamType;
use std::{collections::HashMap, ops::RangeInclusive};

/// Sys-calls params config.
///
/// This is basically a map, which creates a relationship between each kind of
/// param, that a sys-call can have, and allowed values ("rules") for each of
/// the params.
///
/// # Note:
/// If you set the rule for [`ParamType::Ptr`] then it wouldn't be applied as
/// we select value for the pointers in accordance with current memory size.
#[derive(Debug, Clone)]
pub struct SysCallsParamsConfig(HashMap<ParamType, SysCallParamAllowedValues>);

impl SysCallsParamsConfig {
    /// New [`SysCallsParamsConfig`] with all rules set to produce one constant value.
    pub fn all_constant_value(value: i64) -> Self {
        let allowed_values: SysCallParamAllowedValues = (value..=value).into();
        Self(
            [
                ParamType::Size,
                ParamType::Gas,
                ParamType::MessagePosition,
                ParamType::Duration,
                ParamType::Delay,
                ParamType::Handler,
                ParamType::Alloc,
                ParamType::Free,
            ]
            .into_iter()
            .map(|param_type| (param_type, allowed_values.clone()))
            .collect(),
        )
    }

    /// Get allowed values for the `param`.
    pub fn get_rule(&self, param: &ParamType) -> Option<SysCallParamAllowedValues> {
        self.0.get(param).cloned()
    }

    /// Set allowed values for the `param`.
    pub fn add_rule(&mut self, param: ParamType, allowed_values: SysCallParamAllowedValues) {
        matches!(param, ParamType::Ptr(..))
            .then(|| panic!("ParamType::Ptr(..) isn't supported in SysCallsParamsConfig"));

        self.0.insert(param, allowed_values);
    }
}

impl Default for SysCallsParamsConfig {
    fn default() -> Self {
        Self(
            [
                (ParamType::Size, (0..=0x10000).into()),
                // There are no rules for memory arrays and pointers as they are chosen
                // in accordance to memory pages config.
                (ParamType::Gas, (0..=250_000_000_000).into()),
                (ParamType::MessagePosition, (0..=10).into()),
                (ParamType::Duration, (1..=8).into()),
                (ParamType::Delay, (0..=4).into()),
                (ParamType::Handler, (0..=100).into()),
                (ParamType::Alloc, (0..=512).into()),
                (ParamType::Free, (0..=512).into()),
            ]
            .into_iter()
            .collect(),
        )
    }
}

/// Range of allowed values for the sys-call param.
#[derive(Debug, Clone)]
pub struct SysCallParamAllowedValues(RangeInclusive<i64>);

impl From<RangeInclusive<i64>> for SysCallParamAllowedValues {
    fn from(range: RangeInclusive<i64>) -> Self {
        Self(range)
    }
}

impl SysCallParamAllowedValues {
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

impl Default for SysCallParamAllowedValues {
    fn default() -> Self {
        Self::zero()
    }
}

impl SysCallParamAllowedValues {
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
