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

use std::collections::BTreeMap;

use gear_wasm_instrument::{
    parity_wasm::elements::{FunctionType, ValueType},
    syscalls::{ParamType, SysCallName, SysCallSignature},
};

use crate::{GearConfig, ParamRule, Ratio};

pub fn param_to_rule(param: ParamType, config: &GearConfig) -> ParamRule {
    match param {
        ParamType::Size => config.sys_calls.memory_size_rule.clone(),
        ParamType::Ptr => config.sys_calls.ptr_rule.clone(),
        ParamType::Gas => config.sys_calls.gas_rule.clone(),
        ParamType::MessagePosition => config.sys_calls.message_position.clone(),
        ParamType::Duration => config.sys_calls.duration_in_blocks.clone(),
        ParamType::Delay => config.sys_calls.duration_in_blocks.clone(),
        ParamType::Handler => config.sys_calls.handler.clone(),
        ParamType::Alloc => config.sys_calls.alloc_param_rule.clone(),
        ParamType::Free => config.sys_calls.free_param_rule.clone(),
    }
}

/// Syscall function info and config.
#[derive(Debug)]
pub struct SysCallInfo {
    /// Syscall signature params.
    pub params: Vec<ValueType>,
    /// Syscall signature results.
    pub results: Vec<ValueType>,
    /// Syscall frequency in generated code.
    pub frequency: Ratio,
    /// Syscall allowed input values.
    pub parameter_rules: Vec<Parameter>,
}

impl SysCallInfo {
    pub fn new(
        config: &GearConfig,
        signature: SysCallSignature,
        frequency: Ratio,
        skip_memory_array: bool,
    ) -> Self {
        Self {
            params: signature.params.iter().copied().map(Into::into).collect(),
            results: signature.results.to_vec(),
            frequency,
            parameter_rules: Self::into_parameter_rules(
                config,
                signature.params,
                skip_memory_array,
            ),
        }
    }

    pub fn func_type(&self) -> FunctionType {
        FunctionType::new(self.params.clone(), self.results.clone())
    }

    fn into_parameter_rules(
        config: &GearConfig,
        parameters: Vec<ParamType>,
        skip_memory_array: bool,
    ) -> Vec<Parameter> {
        let mut rules = Vec::with_capacity(parameters.len());
        for parameter in parameters.into_iter() {
            match parameter {
                ParamType::Size => match rules.last_mut() {
                    None => rules.push((parameter, false)),
                    Some((first, memory_array)) => match (first, *memory_array) {
                        (ParamType::Ptr, false) if !skip_memory_array => *memory_array = true,
                        _ => rules.push((parameter, false)),
                    },
                },

                _ => rules.push((parameter, false)),
            }
        }

        rules
            .into_iter()
            .map(|(arg_type, memory_array)| match memory_array {
                true => Parameter::MemoryArray,
                false => match arg_type {
                    ParamType::Ptr => Parameter::MemoryValue,
                    ParamType::Alloc => Parameter::Alloc,
                    _ => Parameter::Value {
                        value_type: arg_type.into(),
                        rule: param_to_rule(arg_type, config),
                    },
                },
            })
            .collect()
    }
}

/// Newtype for additional validation of calls arguments.
///
/// Parameters describing memory access should have correct values
/// if required. For example, offset + length < memory_size for arrays.
#[derive(Debug)]
pub enum Parameter {
    /// Some value with its type and generating rule.
    Value {
        value_type: ValueType,
        rule: ParamRule,
    },
    /// Offset and length in memory for an array. Both have type i32.
    MemoryArray,
    /// Pointer in memory for some primitive type value.
    MemoryValue,
    /// Argument to `alloc` syscall.
    Alloc,
}

/// Syscalls config.
#[derive(Debug, Clone)]
pub struct SyscallsConfig {
    pub alloc_param_rule: ParamRule,
    pub free_param_rule: ParamRule,
    pub ptr_rule: ParamRule,
    pub memory_size_rule: ParamRule,
    pub no_rule: ParamRule,
    pub gas_rule: ParamRule,
    pub message_position: ParamRule,
    pub duration_in_blocks: ParamRule,
    pub handler: ParamRule,
}

impl Default for SyscallsConfig {
    fn default() -> Self {
        let unrestricted_ratio = (1, 100).into();
        Self {
            alloc_param_rule: ParamRule {
                allowed_values: 0..=512,
                unrestricted_ratio,
            },
            free_param_rule: ParamRule {
                allowed_values: 0..=512,
                unrestricted_ratio,
            },
            ptr_rule: ParamRule {
                allowed_values: 0..=513 * 0x10000 - 1,
                unrestricted_ratio,
            },
            memory_size_rule: ParamRule {
                allowed_values: 0..=0x10000,
                unrestricted_ratio: (10, 100).into(),
            },
            no_rule: ParamRule {
                allowed_values: 0..=0,
                unrestricted_ratio: (100, 100).into(),
            },
            gas_rule: ParamRule {
                allowed_values: 0..=250_000_000_000,
                unrestricted_ratio,
            },
            message_position: ParamRule {
                allowed_values: 0..=10,
                unrestricted_ratio,
            },
            duration_in_blocks: ParamRule {
                allowed_values: 0..=10000,
                unrestricted_ratio,
            },
            handler: ParamRule {
                allowed_values: 0..=100,
                unrestricted_ratio,
            },
        }
    }
}

/// Make syscalls table for given config.
pub(crate) fn sys_calls_table(config: &GearConfig) -> BTreeMap<SysCallName, SysCallInfo> {
    SysCallName::instrumentable()
        .into_iter()
        .map(|name| {
            (
                name,
                SysCallInfo::new(config, name.signature(), config.sys_call_freq, {
                    name == SysCallName::SendInput || name == SysCallName::SendInputWGas
                }),
            )
        })
        .collect()
}
