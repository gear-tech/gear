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
use gear_wasm_instrument::{parity_wasm::elements::ValueType, syscalls::ParamType};
use std::{collections::HashMap, ops::RangeInclusive};

#[derive(Debug)]
pub(crate) enum ProcessedSysCallParams {
    Alloc,
    Value {
        value_type: ValueType,
        allowed_values: Option<SysCallParamAllowedValues>,
    },
    MemoryArray,
    MemoryPtrValue,
}

pub(crate) fn process_sys_call_params(
    params: &[ParamType],
    params_config: &SysCallsParamsConfig,
) -> Vec<ProcessedSysCallParams> {
    let mut res = Vec::with_capacity(params.len());
    let mut skip_next_param = false;
    for &param in params {
        if skip_next_param {
            skip_next_param = false;
            continue;
        }
        let processed_param = match param {
            ParamType::Alloc => ProcessedSysCallParams::Alloc,
            ParamType::Ptr(maybe_idx) => maybe_idx
                .map(|_| {
                    // skipping next as we don't need the following `Size` param,
                    // because it will be chosen in accordance to the wasm module
                    // memory pages config.
                    skip_next_param = true;
                    ProcessedSysCallParams::MemoryArray
                })
                .unwrap_or(ProcessedSysCallParams::MemoryPtrValue),
            _ => ProcessedSysCallParams::Value {
                value_type: param.into(),
                allowed_values: params_config.get_rule(&param),
            },
        };

        res.push(processed_param);
    }

    res
}

/// Sys-calls params config.
///
/// This is basically a map, which creates a relationship between each kind of
/// param, that a sys-call can have, and allowed values ("rules") for each of
/// the params.
#[derive(Debug, Clone)]
pub struct SysCallsParamsConfig(HashMap<ParamType, SysCallParamAllowedValues>);

impl SysCallsParamsConfig {
    /// Get allowed values for the `param`.
    pub fn get_rule(&self, param: &ParamType) -> Option<SysCallParamAllowedValues> {
        self.0.get(param).cloned()
    }

    /// Set allowed values for the `param`.
    pub fn add_rule(&mut self, param: ParamType, allowed_values: SysCallParamAllowedValues) {
        self.0.insert(param, allowed_values);
    }
}

impl Default for SysCallsParamsConfig {
    fn default() -> Self {
        Self(
            [
                (ParamType::Size, (0..=0x10000).into()),
                // There are no rules for memory arrays as they are chose in accordance
                // to memory pages config.
                (ParamType::Ptr(None), (0..=513 * 0x10000 - 1).into()),
                (ParamType::Gas, (0..=250_000_000_000).into()),
                (ParamType::MessagePosition, (0..=10).into()),
                (ParamType::Duration, (0..=100).into()),
                (ParamType::Delay, (0..=100).into()),
                (ParamType::Handler, (0..=100).into()),
                (ParamType::Alloc, (0..=512).into()),
                (ParamType::Free, (0..=512).into()),
            ]
            .iter()
            .cloned()
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
