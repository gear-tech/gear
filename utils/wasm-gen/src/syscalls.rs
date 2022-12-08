// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
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
pub struct SysCallInfo {
    /// Syscall signature params.
    pub params: Vec<ValueType>,
    /// Syscall signature results.
    pub results: Vec<ValueType>,
    /// Syscall params allowed input values.
    pub param_rules: Vec<ParamRule>,
    /// Syscall frequency in generated code.
    pub frequency: Ratio,
}

impl SysCallInfo {
    pub fn new(config: &GearConfig, signature: SysCallSignature, frequency: Ratio) -> Self {
        Self {
            params: signature.params.iter().copied().map(Into::into).collect(),
            results: signature.results.to_vec(),
            param_rules: signature
                .params
                .iter()
                .copied()
                .map(|param| param_to_rule(param, config))
                .collect(),
            frequency,
        }
    }

    pub fn func_type(&self) -> FunctionType {
        FunctionType::new(self.params.clone(), self.results.clone())
    }
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
        let restricted_ratio = (1, 100).into();
        Self {
            alloc_param_rule: ParamRule {
                allowed_values: 0..=512,
                restricted_ratio,
            },
            free_param_rule: ParamRule {
                allowed_values: 0..=512,
                restricted_ratio,
            },
            ptr_rule: ParamRule {
                allowed_values: 0..=513 * 0x10000 - 1,
                restricted_ratio,
            },
            memory_size_rule: ParamRule {
                allowed_values: 0..=0x10000,
                restricted_ratio: (10, 100).into(),
            },
            no_rule: ParamRule {
                allowed_values: 0..=0,
                restricted_ratio: (100, 100).into(),
            },
            gas_rule: ParamRule {
                allowed_values: 0..=250_000_000_000,
                restricted_ratio,
            },
            message_position: ParamRule {
                allowed_values: 0..=10,
                restricted_ratio,
            },
            duration_in_blocks: ParamRule {
                allowed_values: 0..=10000,
                restricted_ratio,
            },
            handler: ParamRule {
                allowed_values: 0..=100,
                restricted_ratio,
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
                SysCallInfo::new(config, name.signature(), config.sys_call_freq),
            )
        })
        .collect()
}
