// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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

//! sp-sandbox environment for running a module.

use crate::memory::MemoryWrap;
use alloc::{boxed::Box, collections::BTreeMap, format, string::String, vec::Vec};
use gear_backend_common::{
    funcs as common_funcs, BackendError, BackendReport, Environment, IntoExtInfo, TerminationReason,
};
use gear_core::{
    env::{Ext, LaterExt},
    gas::GasAmount,
    memory::{Memory, PageBuf, PageNumber, WasmPageNumber},
};
use sp_sandbox::{
    default_executor::{EnvironmentDefinitionBuilder, Instance, Memory as DefaultExecutorMemory},
    ReturnValue, SandboxEnvironmentBuilder, SandboxInstance, SandboxMemory,
};

/// Environment to run one module at a time providing Ext.
pub struct SandboxEnvironment<E: Ext + IntoExtInfo> {
    runtime: Runtime<E>,
    instance: Instance<Runtime<E>>,
    entries: Vec<String>,
}

pub(crate) struct Runtime<E: Ext> {
    pub ext: LaterExt<E>,
    pub memory: MemoryWrap,
    pub trap: Option<&'static str>,
}

fn get_module_exports(binary: &[u8]) -> Result<Vec<String>, String> {
    Ok(parity_wasm::elements::Module::from_bytes(binary)
        .map_err(|e| format!("Unable to create wasm module: {}", e))?
        .export_section()
        .ok_or_else(|| String::from("Unable to get wasm module section"))?
        .entries()
        .iter()
        .map(|v| String::from(v.field()))
        .collect())
}

fn set_pages(
    memory: &mut dyn Memory,
    pages: &BTreeMap<PageNumber, Option<Box<PageBuf>>>,
) -> Result<(), String> {
    for (num, buf) in pages {
        if let Some(buf) = buf {
            memory
                .write(num.offset(), &buf[..])
                .map_err(|e| format!("Cannot write mem to {:?}: {:?}", num, e))?;
        }
    }
    Ok(())
}

impl<E: Ext + IntoExtInfo + 'static> Environment<E> for SandboxEnvironment<E> {
    fn new(
        ext: E,
        binary: &[u8],
        memory_pages: &BTreeMap<PageNumber, Option<Box<PageBuf>>>,
        mem_size: WasmPageNumber,
    ) -> Result<Self, BackendError<'static>> {
        let mut later_ext = LaterExt::default();
        later_ext.set(ext);

        let mem: DefaultExecutorMemory =
            SandboxMemory::new(mem_size.0, None).map_err(|e| BackendError {
                reason: "Create env memory fail",
                description: Some(format!("{:?}", e).into()),
                gas_amount: later_ext.unset().into_gas_amount(),
            })?;

        let mut env_builder = EnvironmentDefinitionBuilder::new();

        use crate::funcs::FuncsHandler as funcs;
        env_builder.add_memory("env", "memory", mem.clone());
        env_builder.add_host_func("env", "alloc", funcs::alloc);
        env_builder.add_host_func("env", "free", funcs::free);
        env_builder.add_host_func("env", "gr_block_height", funcs::block_height);
        env_builder.add_host_func("env", "gr_block_timestamp", funcs::block_timestamp);
        env_builder.add_host_func("env", "gr_create_program_wgas", funcs::create_program_wgas);
        env_builder.add_host_func("env", "gr_exit", funcs::exit);
        env_builder.add_host_func("env", "gr_exit_code", funcs::exit_code);
        env_builder.add_host_func("env", "gr_origin", funcs::origin);
        env_builder.add_host_func("env", "gr_send", funcs::send);
        env_builder.add_host_func("env", "gr_send_wgas", funcs::send_wgas);
        env_builder.add_host_func("env", "gr_send_commit", funcs::send_commit);
        env_builder.add_host_func("env", "gr_send_commit_wgas", funcs::send_commit_wgas);
        env_builder.add_host_func("env", "gr_send_init", funcs::send_init);
        env_builder.add_host_func("env", "gr_send_push", funcs::send_push);
        env_builder.add_host_func("env", "gr_read", funcs::read);
        env_builder.add_host_func("env", "gr_size", funcs::size);
        env_builder.add_host_func("env", "gr_source", funcs::source);
        env_builder.add_host_func("env", "gr_program_id", funcs::program_id);
        env_builder.add_host_func("env", "gr_value", funcs::value);
        env_builder.add_host_func("env", "gr_value_available", funcs::value_available);
        env_builder.add_host_func("env", "gr_reply", funcs::reply);
        env_builder.add_host_func("env", "gr_reply_commit", funcs::reply_commit);
        env_builder.add_host_func("env", "gr_reply_to", funcs::reply_to);
        env_builder.add_host_func("env", "gr_reply_push", funcs::reply_push);
        env_builder.add_host_func("env", "gr_debug", funcs::debug);
        env_builder.add_host_func("env", "gr_gas_available", funcs::gas_available);
        env_builder.add_host_func("env", "gr_msg_id", funcs::msg_id);
        env_builder.add_host_func("env", "gr_leave", funcs::leave);
        env_builder.add_host_func("env", "gr_wait", funcs::wait);
        env_builder.add_host_func("env", "gr_wake", funcs::wake);
        env_builder.add_host_func("env", "gas", funcs::gas);

        let mut runtime = Runtime {
            ext: later_ext,
            memory: MemoryWrap::new(mem),
            trap: None,
        };

        let instance =
            Instance::new(binary, &env_builder, &mut runtime).map_err(|e| BackendError {
                reason: "Unable to instanciate module",
                description: Some(format!("{:?}", e).into()),
                gas_amount: runtime.ext.unset().into_gas_amount(),
            })?;

        let entries = get_module_exports(binary).map_err(|e| BackendError {
            reason: "Unable to get wasm module exports",
            description: Some(format!("{:?}", e).into()),
            gas_amount: runtime.ext.unset().into_gas_amount(),
        })?;

        // Set module memory.
        set_pages(&mut runtime.memory, memory_pages).map_err(|e| BackendError {
            reason: "Unable to set module memory data",
            description: Some(format!("{:?}", e).into()),
            gas_amount: runtime.ext.unset().into_gas_amount(),
        })?;

        Ok(SandboxEnvironment {
            runtime,
            instance,
            entries,
        })
    }

    fn get_stack_mem_end(&mut self) -> Option<WasmPageNumber> {
        // '__gear_stack_end' export is inserted in wasm-proc or wasm-builder
        let global = self.instance.get_global_val("__gear_stack_end")?;
        global.as_i32().and_then(|addr| {
            if addr < 0 {
                None
            } else {
                Some(WasmPageNumber(
                    (addr as usize / WasmPageNumber::size()) as u32,
                ))
            }
        })
    }

    fn get_wasm_memory_begin_addr(&mut self) -> u64 {
        self.runtime.memory.get_wasm_memory_begin_addr()
    }

    fn execute(&mut self, entry_point: &str) -> Result<BackendReport, BackendError> {
        let res = if self.entries.contains(&String::from(entry_point)) {
            self.instance.invoke(entry_point, &[], &mut self.runtime)
        } else {
            Ok(ReturnValue::Unit)
        };

        log::debug!("execution res = {:?}", res);

        let info = self
            .runtime
            .ext
            .unset()
            .into_ext_info(|ptr, buff| {
                self.runtime
                    .memory
                    .read(ptr, buff)
                    .map_err(|_err| "Cannot read sandbox mem")
            })
            .map_err(|(reason, gas_amount)| BackendError {
                reason,
                description: None,
                gas_amount,
            })?;

        let termination = if res.is_err() {
            let reason = if let Some(trap) = self.runtime.trap {
                if let Some(value_dest) = info.exit_argument {
                    Some(TerminationReason::Exit(value_dest))
                } else if common_funcs::is_wait_trap(trap) {
                    Some(TerminationReason::Wait)
                } else if common_funcs::is_leave_trap(trap) {
                    Some(TerminationReason::Leave)
                } else if common_funcs::is_gas_allowance_trap(trap) {
                    Some(TerminationReason::GasAllowanceExceed)
                } else {
                    None
                }
            } else {
                None
            };

            reason.unwrap_or_else(|| TerminationReason::Trap {
                explanation: info.trap_explanation,
                description: self.runtime.trap.map(Into::into),
            })
        } else {
            TerminationReason::Success
        };

        let wasm_memory_addr = self.get_wasm_memory_begin_addr();

        Ok(BackendReport {
            termination,
            wasm_memory_addr,
            info,
        })
    }

    fn drop_env(&mut self) -> GasAmount {
        self.runtime.ext.unset().into_gas_amount()
    }
}
