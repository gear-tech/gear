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

use gear_wasm_instrument::parity_wasm::elements::{FunctionType, ValueType};

use crate::{GearConfig, ParamRule, Ratio};

#[derive(Debug, Clone, Copy)]
pub enum ParamType {
    Size,            // i32 buffers size in memory
    Ptr,             // i32 pointer
    Gas,             // i64 gas amount
    MessagePosition, // i32 message position
    Duration,        // i32 duration in blocks
    Delay,           // i32 delay in blocks
    Handler,         // i32 handler number
    Alloc,           // i32 alloc pages
    Free,            // i32 free page
}

impl From<ParamType> for ValueType {
    fn from(value: ParamType) -> Self {
        match value {
            ParamType::Gas => ValueType::I64,
            _ => ValueType::I32,
        }
    }
}

impl ParamType {
    pub fn into_param_rule(self, config: &GearConfig) -> ParamRule {
        match self {
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
    pub fn new<const N: usize, const M: usize>(
        config: &GearConfig,
        params: [ParamType; N],
        results: [ValueType; M],
        frequency: Ratio,
    ) -> Self {
        Self {
            params: params.iter().copied().map(Into::into).collect(),
            results: results.to_vec(),
            param_rules: params
                .iter()
                .copied()
                .map(|param| param.into_param_rule(config))
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
pub(crate) fn sys_calls_table(config: &GearConfig) -> BTreeMap<&'static str, SysCallInfo> {
    use ParamType::*;
    use ValueType::*;
    let mut res = BTreeMap::new();
    let frequency = config.sys_call_freq;

    res.insert("alloc", SysCallInfo::new(config, [Alloc], [I32], frequency));
    res.insert("free", SysCallInfo::new(config, [Free], [], frequency));

    res.insert(
        "gr_debug",
        SysCallInfo::new(config, [Ptr, Size], [], frequency),
    );
    res.insert(
        "gr_error",
        SysCallInfo::new(config, [Ptr], [I32], frequency),
    );

    res.insert(
        "gr_block_height",
        SysCallInfo::new(config, [], [I32], frequency),
    );
    res.insert(
        "gr_block_timestamp",
        SysCallInfo::new(config, [], [I64], frequency),
    );
    res.insert("gr_exit", SysCallInfo::new(config, [Ptr], [], frequency));
    res.insert(
        "gr_gas_available",
        SysCallInfo::new(config, [], [I64], frequency),
    );
    res.insert(
        "gr_program_id",
        SysCallInfo::new(config, [Ptr], [], frequency),
    );
    res.insert("gr_origin", SysCallInfo::new(config, [Ptr], [], frequency));
    res.insert("gr_leave", SysCallInfo::new(config, [], [], frequency));
    res.insert(
        "gr_value_available",
        SysCallInfo::new(config, [Ptr], [], frequency),
    );
    res.insert("gr_wait", SysCallInfo::new(config, [], [], frequency));
    res.insert(
        "gr_wait_up_to",
        SysCallInfo::new(config, [Duration], [], frequency),
    );
    res.insert(
        "gr_wait_for",
        SysCallInfo::new(config, [Duration], [], frequency),
    );
    res.insert(
        "gr_wake",
        SysCallInfo::new(config, [Ptr, Delay], [I32], frequency),
    );

    res.insert(
        "gr_status_code",
        SysCallInfo::new(config, [Ptr], [I32], frequency),
    );
    res.insert(
        "gr_message_id",
        SysCallInfo::new(config, [Ptr], [], frequency),
    );
    res.insert(
        "gr_read",
        SysCallInfo::new(config, [MessagePosition, Size, Ptr], [I32], frequency),
    );
    res.insert(
        "gr_reply",
        SysCallInfo::new(config, [Ptr, Size, Ptr, Ptr, Delay], [I32], frequency),
    );
    res.insert(
        "gr_reply_wgas",
        SysCallInfo::new(config, [Ptr, Size, Gas, Ptr, Delay, Ptr], [I32], frequency),
    );
    res.insert(
        "gr_reply_commit",
        SysCallInfo::new(config, [Ptr, Delay, Ptr], [I32], frequency),
    );
    res.insert(
        "gr_reply_commit_wgas",
        SysCallInfo::new(config, [Gas, Ptr, Delay, Ptr], [I32], frequency),
    );
    res.insert(
        "gr_reply_push",
        SysCallInfo::new(config, [Ptr, Size], [I32], frequency),
    );
    res.insert(
        "gr_reply_to",
        SysCallInfo::new(config, [Ptr], [I32], frequency),
    );
    res.insert(
        "gr_send",
        SysCallInfo::new(config, [Ptr, Ptr, Size, Ptr, Delay, Ptr], [I32], frequency),
    );
    res.insert(
        "gr_send_wgas",
        SysCallInfo::new(
            config,
            [Ptr, Ptr, Size, Gas, Ptr, Delay, Ptr],
            [I32],
            frequency,
        ),
    );
    res.insert(
        "gr_send_commit",
        SysCallInfo::new(config, [Handler, Ptr, Ptr, Delay, Ptr], [I32], frequency),
    );
    res.insert(
        "gr_send_commit_wgas",
        SysCallInfo::new(
            config,
            [Handler, Ptr, Gas, Ptr, Delay, Ptr],
            [I32],
            frequency,
        ),
    );
    res.insert(
        "gr_send_init",
        SysCallInfo::new(config, [Handler], [I32], frequency),
    );
    res.insert(
        "gr_send_push",
        SysCallInfo::new(config, [Handler, Ptr, Size], [I32], frequency),
    );
    res.insert("gr_size", SysCallInfo::new(config, [], [I32], frequency));
    res.insert("gr_source", SysCallInfo::new(config, [Ptr], [], frequency));
    res.insert("gr_value", SysCallInfo::new(config, [Ptr], [], frequency));

    res.insert(
        "gr_create_program",
        SysCallInfo::new(
            config,
            [Ptr, Ptr, Size, Ptr, Size, Ptr, Delay, Ptr, Ptr],
            [I32],
            frequency,
        ),
    );
    res.insert(
        "gr_create_program_wgas",
        SysCallInfo::new(
            config,
            [Ptr, Ptr, Size, Ptr, Size, Gas, Ptr, Delay, Ptr, Ptr],
            [I32],
            frequency,
        ),
    );

    res
}

/// Check that all sys calls are supported by backend.
#[test]
fn test_sys_calls_table() {
    use gear_backend_common::{mock::MockExt, Environment, TerminationReason};
    use gear_backend_wasmi::WasmiEnvironment;
    use gear_core::message::DispatchKind;
    use gear_wasm_instrument::{
        parity_wasm::{self, builder},
        wasm_instrument::gas_metering::ConstantCostRules,
    };

    let config = GearConfig::new_normal();
    let table = sys_calls_table(&config);

    // Make module with one empty function.
    let mut module = builder::module()
        .function()
        .signature()
        .build()
        .build()
        .build();

    // Insert syscalls imports.
    for (name, info) in table {
        let types = module.type_section_mut().unwrap().types_mut();
        let type_no = types.len() as u32;
        types.push(parity_wasm::elements::Type::Function(info.func_type()));

        module = builder::from_module(module)
            .import()
            .module("env")
            .external()
            .func(type_no)
            .field(name)
            .build()
            .build();
    }

    let module =
        gear_wasm_instrument::inject(module, &ConstantCostRules::default(), "env").unwrap();
    let code = module.into_bytes().unwrap();

    // Execute wasm and check success.
    let ext = MockExt::default();
    let env = WasmiEnvironment::new(ext, &code, Default::default(), 0.into()).unwrap();
    let res = env
        .execute(&DispatchKind::Init, |_, _| -> Result<(), u32> { Ok(()) })
        .unwrap();

    assert_eq!(res.termination_reason, TerminationReason::Success);
}
