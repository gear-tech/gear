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

use parity_wasm::elements::{FunctionType, ValueType};

use crate::{GearConfig, ParamRule, Ratio};

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
    pub fn func_type(&self) -> FunctionType {
        FunctionType::new(self.params.clone(), self.results.clone())
    }
}

/// Syscalls config.
#[derive(Debug, Clone)]
pub struct SyscallsConfig {
    pub alloc_param_rules: ParamRule,
    pub free_param_rules: ParamRule,
    pub ptr_rule: ParamRule,
    pub memory_size_rule: ParamRule,
    pub no_rule: ParamRule,
}

impl Default for SyscallsConfig {
    fn default() -> Self {
        Self {
            alloc_param_rules: ParamRule {
                allowed_values: 0..=512,
                restricted_ratio: (1, 100).into(),
            },
            free_param_rules: ParamRule {
                allowed_values: 0..=512,
                restricted_ratio: (1, 100).into(),
            },
            ptr_rule: ParamRule {
                allowed_values: 0..=513 * 0x10000 - 1,
                restricted_ratio: (1, 100).into(),
            },
            memory_size_rule: ParamRule {
                allowed_values: 0..=0x10000,
                restricted_ratio: (10, 100).into(),
            },
            no_rule: ParamRule {
                allowed_values: 0..=0,
                restricted_ratio: (100, 100).into(),
            },
        }
    }
}

/// Make syscalls table for given config.
pub(crate) fn sys_calls_table(config: &GearConfig) -> BTreeMap<&'static str, SysCallInfo> {
    use ValueType::*;
    let mut res = BTreeMap::new();
    let frequency = config.sys_call_freq;

    let ptr_rule = || config.sys_calls.ptr_rule.clone();
    let size_rule = || config.sys_calls.memory_size_rule.clone();
    let no_rule = || config.sys_calls.no_rule.clone();

    // alloc(pages: u32) -> usize;
    res.insert(
        "alloc",
        SysCallInfo {
            params: [I32].to_vec(),
            results: [I32].to_vec(),
            param_rules: [config.sys_calls.alloc_param_rules.clone()].to_vec(),
            frequency,
        },
    );
    // free(page: u32);
    res.insert(
        "free",
        SysCallInfo {
            params: [I32].to_vec(),
            results: [].to_vec(),
            param_rules: [config.sys_calls.free_param_rules.clone()].to_vec(),
            frequency,
        },
    );

    // gr_debug(msg_ptr: *const u8, msg_len: u32);
    res.insert(
        "gr_debug",
        SysCallInfo {
            params: [I32, I32].to_vec(),
            results: [].to_vec(),
            param_rules: [ptr_rule(), size_rule()].to_vec(),
            frequency,
        },
    );
    // gr_error(data: *mut u8);
    res.insert(
        "gr_error",
        SysCallInfo {
            params: [I32].to_vec(),
            results: [].to_vec(),
            param_rules: [ptr_rule()].to_vec(),
            frequency,
        },
    );

    // gr_block_height() -> u32;
    res.insert(
        "gr_block_height",
        SysCallInfo {
            params: [].to_vec(),
            results: [I32].to_vec(),
            param_rules: [].to_vec(),
            frequency,
        },
    );
    // gr_block_timestamp() -> u64;
    res.insert(
        "gr_block_timestamp",
        SysCallInfo {
            params: [].to_vec(),
            results: [I64].to_vec(),
            param_rules: [].to_vec(),
            frequency,
        },
    );
    // gr_exit(value_dest_ptr: *const u8) -> !;
    res.insert(
        "gr_exit",
        SysCallInfo {
            params: [I32].to_vec(),
            results: [].to_vec(),
            param_rules: [ptr_rule()].to_vec(),
            frequency,
        },
    );
    // gr_gas_available() -> u64;
    res.insert(
        "gr_gas_available",
        SysCallInfo {
            params: [].to_vec(),
            results: [I64].to_vec(),
            param_rules: [].to_vec(),
            frequency,
        },
    );
    // gr_program_id(val: *mut u8);
    res.insert(
        "gr_program_id",
        SysCallInfo {
            params: [I32].to_vec(),
            results: [].to_vec(),
            param_rules: [ptr_rule()].to_vec(),
            frequency,
        },
    );
    // gr_origin(origin_ptr: *mut u8);
    res.insert(
        "gr_origin",
        SysCallInfo {
            params: [I32].to_vec(),
            results: [].to_vec(),
            param_rules: [ptr_rule()].to_vec(),
            frequency,
        },
    );
    // gr_leave() -> !;
    res.insert(
        "gr_leave",
        SysCallInfo {
            params: [].to_vec(),
            results: [].to_vec(),
            param_rules: [].to_vec(),
            frequency,
        },
    );
    // gr_value_available(val: *mut u8);
    res.insert(
        "gr_value_available",
        SysCallInfo {
            params: [I32].to_vec(),
            results: [].to_vec(),
            param_rules: [ptr_rule()].to_vec(),
            frequency,
        },
    );
    // gr_wait() -> !;
    res.insert(
        "gr_wait",
        SysCallInfo {
            params: [].to_vec(),
            results: [].to_vec(),
            param_rules: [].to_vec(),
            frequency,
        },
    );
    // gr_wait_up_to(duration: *const u8) -> !;
    res.insert(
        "gr_wait_up_to",
        SysCallInfo {
            params: [I32].to_vec(),
            results: [].to_vec(),
            param_rules: [ptr_rule()].to_vec(),
            frequency,
        },
    );
    // gr_wait_for(duration: *const u8) -> !;
    res.insert(
        "gr_wait_for",
        SysCallInfo {
            params: [I32].to_vec(),
            results: [].to_vec(),
            param_rules: [ptr_rule()].to_vec(),
            frequency,
        },
    );
    // gr_wake(waker_id_ptr: *const u8);
    res.insert(
        "gr_wake",
        SysCallInfo {
            params: [I32].to_vec(),
            results: [].to_vec(),
            param_rules: [ptr_rule()].to_vec(),
            frequency,
        },
    );

    // gr_exit_code() -> i32;
    res.insert(
        "gr_exit_code",
        SysCallInfo {
            params: [].to_vec(),
            results: [I32].to_vec(),
            param_rules: [].to_vec(),
            frequency,
        },
    );
    // gr_msg_id(val: *mut u8);
    res.insert(
        "gr_msg_id",
        SysCallInfo {
            params: [I32].to_vec(),
            results: [].to_vec(),
            param_rules: [ptr_rule()].to_vec(),
            frequency,
        },
    );
    // gr_read(at: u32, len: u32, dest: *mut u8);
    res.insert(
        "gr_exit_code",
        SysCallInfo {
            params: [I32, I32, I32].to_vec(),
            results: [].to_vec(),
            param_rules: [no_rule(), size_rule(), ptr_rule()].to_vec(),
            frequency,
        },
    );
    // gr_reply(data_ptr: *const u8, data_len: u32, value_ptr: *const u8, message_id_ptr: *mut u8) -> SyscallError;
    res.insert(
        "gr_reply",
        SysCallInfo {
            params: [I32, I32, I32, I32].to_vec(),
            results: [I32].to_vec(),
            param_rules: [ptr_rule(), size_rule(), ptr_rule(), ptr_rule()].to_vec(),
            frequency,
        },
    );
    // gr_reply_wgas(
    //     data_ptr: *const u8,
    //     data_len: u32,
    //     gas_limit: u64,
    //     value_ptr: *const u8,
    //     message_id_ptr: *mut u8,
    // ) -> SyscallError;
    res.insert(
        "gr_reply_wgas",
        SysCallInfo {
            params: [I32, I32, I64, I32, I32].to_vec(),
            results: [I32].to_vec(),
            param_rules: [ptr_rule(), size_rule(), no_rule(), ptr_rule(), ptr_rule()].to_vec(),
            frequency,
        },
    );
    // gr_reply_commit(value_ptr: *const u8, message_id_ptr: *mut u8) -> SyscallError;
    res.insert(
        "gr_reply_commit",
        SysCallInfo {
            params: [I32, I32].to_vec(),
            results: [I32].to_vec(),
            param_rules: [ptr_rule(), ptr_rule()].to_vec(),
            frequency,
        },
    );
    // gr_reply_commit_wgas(
    //     gas_limit: u64,
    //     value_ptr: *const u8,
    //     message_id_ptr: *mut u8,
    // ) -> SyscallError;
    res.insert(
        "gr_reply_commit_wgas",
        SysCallInfo {
            params: [I64, I32, I32].to_vec(),
            results: [I32].to_vec(),
            param_rules: [no_rule(), ptr_rule(), ptr_rule()].to_vec(),
            frequency,
        },
    );
    // gr_reply_push(data_ptr: *const u8, data_len: u32) -> SyscallError;
    res.insert(
        "gr_reply_push",
        SysCallInfo {
            params: [I32, I32].to_vec(),
            results: [I32].to_vec(),
            param_rules: [ptr_rule(), size_rule()].to_vec(),
            frequency,
        },
    );
    // gr_reply_to(dest: *mut u8);
    res.insert(
        "gr_reply_to",
        SysCallInfo {
            params: [I32].to_vec(),
            results: [].to_vec(),
            param_rules: [ptr_rule()].to_vec(),
            frequency,
        },
    );
    // gr_send(
    //     program: *const u8,
    //     data_ptr: *const u8,
    //     data_len: u32,
    //     value_ptr: *const u8,
    //     message_id_ptr: *mut u8,
    // ) -> SyscallError;
    res.insert(
        "gr_send",
        SysCallInfo {
            params: [I32, I32, I32, I32, I32].to_vec(),
            results: [I32].to_vec(),
            param_rules: [ptr_rule(), ptr_rule(), size_rule(), ptr_rule(), ptr_rule()].to_vec(),
            frequency,
        },
    );
    // gr_send_wgas(
    //     program: *const u8,
    //     data_ptr: *const u8,
    //     data_len: u32,
    //     gas_limit: u64,
    //     value_ptr: *const u8,
    //     message_id_ptr: *mut u8,
    // ) -> SyscallError;
    res.insert(
        "gr_send_wgas",
        SysCallInfo {
            params: [I32, I32, I32, I64, I32, I32].to_vec(),
            results: [I32].to_vec(),
            param_rules: [
                ptr_rule(),
                ptr_rule(),
                size_rule(),
                no_rule(),
                ptr_rule(),
                ptr_rule(),
            ]
            .to_vec(),
            frequency,
        },
    );
    // gr_send_commit(
    //     handle: u32,
    //     message_id_ptr: *mut u8,
    //     program: *const u8,
    //     value_ptr: *const u8,
    // ) -> SyscallError;
    res.insert(
        "gr_send_commit",
        SysCallInfo {
            params: [I32, I32, I32, I32].to_vec(),
            results: [I32].to_vec(),
            param_rules: [no_rule(), ptr_rule(), ptr_rule(), ptr_rule()].to_vec(),
            frequency,
        },
    );
    // gr_send_commit_wgas(
    //     handle: u32,
    //     message_id_ptr: *mut u8,
    //     program: *const u8,
    //     gas_limit: u64,
    //     value_ptr: *const u8,
    // ) -> SyscallError;
    res.insert(
        "gr_send_commit_wgas",
        SysCallInfo {
            params: [I32, I32, I32, I64, I32].to_vec(),
            results: [I32].to_vec(),
            param_rules: [no_rule(), ptr_rule(), ptr_rule(), no_rule(), ptr_rule()].to_vec(),
            frequency,
        },
    );
    // gr_send_init(handle: *mut u32) -> SyscallError;
    res.insert(
        "gr_send_init",
        SysCallInfo {
            params: [I32].to_vec(),
            results: [I32].to_vec(),
            param_rules: [ptr_rule()].to_vec(),
            frequency,
        },
    );
    // gr_send_push(handle: u32, data_ptr: *const u8, data_len: u32) -> SyscallError;
    res.insert(
        "gr_send_push",
        SysCallInfo {
            params: [I32, I32, I32].to_vec(),
            results: [I32].to_vec(),
            param_rules: [no_rule(), ptr_rule(), size_rule()].to_vec(),
            frequency,
        },
    );
    // gr_size() -> u32;
    res.insert(
        "gr_size",
        SysCallInfo {
            params: [].to_vec(),
            results: [I32].to_vec(),
            param_rules: [].to_vec(),
            frequency,
        },
    );
    // gr_source(program: *mut u8);
    res.insert(
        "gr_source",
        SysCallInfo {
            params: [I32].to_vec(),
            results: [].to_vec(),
            param_rules: [ptr_rule()].to_vec(),
            frequency,
        },
    );
    // gr_value(val: *mut u8);
    res.insert(
        "gr_value",
        SysCallInfo {
            params: [I32].to_vec(),
            results: [].to_vec(),
            param_rules: [ptr_rule()].to_vec(),
            frequency,
        },
    );

    // gr_create_program(
    //     code_hash: *const u8,
    //     salt_ptr: *const u8,
    //     salt_len: u32,
    //     data_ptr: *const u8,
    //     data_len: u32,
    //     value_ptr: *const u8,
    //     program_id_ptr: *mut u8,
    // ) -> SyscallError;
    res.insert(
        "gr_create_program",
        SysCallInfo {
            params: [I32, I32, I32, I32, I32, I32, I32].to_vec(),
            results: [].to_vec(),
            param_rules: [
                ptr_rule(),
                ptr_rule(),
                size_rule(),
                ptr_rule(),
                size_rule(),
                ptr_rule(),
                ptr_rule(),
            ]
            .to_vec(),
            frequency,
        },
    );

    // gr_create_program_wgas(
    //     code_hash: *const u8,
    //     salt_ptr: *const u8,
    //     salt_len: u32,
    //     data_ptr: *const u8,
    //     data_len: u32,
    //     gas_limit: u64,
    //     value_ptr: *const u8,
    //     program_id_ptr: *mut u8,
    // ) -> SyscallError;
    res.insert(
        "gr_create_program_wgas",
        SysCallInfo {
            params: [I32, I32, I32, I32, I32, I64, I32, I32].to_vec(),
            results: [].to_vec(),
            param_rules: [
                ptr_rule(),
                ptr_rule(),
                size_rule(),
                ptr_rule(),
                size_rule(),
                no_rule(),
                ptr_rule(),
                ptr_rule(),
            ]
            .to_vec(),
            frequency,
        },
    );

    res
}

/// Check that all sys calls are supported by backend.
#[test]
fn test_sys_calls_table() {
    use gear_backend_common::{mock::MockExt, Environment, TerminationReason};
    use gear_backend_wasmi::WasmiEnvironment;
    use gear_core::message::DispatchKind;
    use wasm_instrument::parity_wasm::builder;

    let config = GearConfig::default();
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

    let code = module.into_bytes().unwrap();

    // Execute wasm and check success.
    let mut ext = MockExt::default();
    let res = WasmiEnvironment::execute(
        &mut ext,
        &code,
        Default::default(),
        0.into(),
        &DispatchKind::Init,
        |_, _| -> Result<(), u32> { Ok(()) },
    )
    .unwrap();

    assert_eq!(res.termination_reason, TerminationReason::Success);
}
