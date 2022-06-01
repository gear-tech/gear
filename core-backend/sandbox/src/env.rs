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

use crate::{funcs::FuncError, memory::MemoryWrap};
use alloc::{
    collections::BTreeMap,
    format,
    string::{String, ToString},
    vec::Vec,
};
use core::fmt;
use gear_backend_common::{
    error_processor::IntoExtError, AsTerminationReason, BackendError, BackendReport, Environment,
    IntoExtInfo, TerminationReason, TerminationReasonKind,
};
use gear_core::{
    env::{Ext, ExtCarrier},
    gas::GasAmount,
    memory::{Memory, PageBuf, PageNumber, WasmPageNumber},
};
use gear_core_errors::MemoryError;
use sp_sandbox::{
    default_executor::{EnvironmentDefinitionBuilder, Instance, Memory as DefaultExecutorMemory},
    ReturnValue, SandboxEnvironmentBuilder, SandboxInstance, SandboxMemory,
};

#[derive(Debug, derive_more::Display)]
pub enum SandboxEnvironmentError {
    #[display(fmt = "Unable to instantiate module")]
    ModuleInstantiation,
    #[display(fmt = "Unable to get wasm module exports")]
    GetWasmExports,
    #[display(fmt = "Unable to set module memory data")]
    SetModuleMemoryData,
    #[display(fmt = "Failed to create env memory")]
    CreateEnvMemory,
    #[display(fmt = "{}", _0)]
    Memory(MemoryError),
    #[display(fmt = "{}", _0)]
    PostExecutionHandler(String),
}

/// Environment to run one module at a time providing Ext.
pub struct SandboxEnvironment<E: Ext + IntoExtInfo> {
    runtime: Runtime<E>,
    instance: Instance<Runtime<E>>,
    entries: Vec<String>,
}

pub(crate) struct Runtime<E: Ext> {
    pub ext: ExtCarrier<E>,
    pub memory: MemoryWrap,
    pub trap: Option<FuncError<E::Error>>,
    pub termination_reason: Option<TerminationReasonKind>,
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
    memory: &mut impl Memory,
    pages: &BTreeMap<PageNumber, PageBuf>,
) -> Result<(), String> {
    let memory_size = memory.size();
    for (page, buf) in pages {
        if page.to_wasm_page() >= memory_size {
            return Err(format!(
                "{:?} is out of memory size: {:?}",
                page, memory_size
            ));
        }
        memory
            .write(page.offset(), &buf[..])
            .map_err(|e| format!("Cannot write mem to {:?}: {:?}", page, e))?;
    }
    Ok(())
}

impl<E> Environment<E> for SandboxEnvironment<E>
where
    E: Ext + IntoExtInfo + 'static,
    E::Error: AsTerminationReason + IntoExtError,
{
    type Error = SandboxEnvironmentError;

    fn new(
        ext: E,
        binary: &[u8],
        memory_pages: &BTreeMap<PageNumber, PageBuf>,
        mem_size: WasmPageNumber,
    ) -> Result<Self, BackendError<Self::Error>> {
        let ext_carrier = ExtCarrier::new(ext);

        let mem: DefaultExecutorMemory = match SandboxMemory::new(mem_size.0, None) {
            Ok(mem) => mem,
            Err(e) => {
                return Err(BackendError {
                    reason: SandboxEnvironmentError::CreateEnvMemory,
                    description: Some(format!("{:?}", e).into()),
                    gas_amount: ext_carrier.into_inner().into_gas_amount(),
                })
            }
        };

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
        env_builder.add_host_func("env", "gr_reply_commit_wgas", funcs::reply_commit_wgas);
        env_builder.add_host_func("env", "gr_reply_to", funcs::reply_to);
        env_builder.add_host_func("env", "gr_reply_push", funcs::reply_push);
        env_builder.add_host_func("env", "gr_reply_wgas", funcs::reply_wgas);
        env_builder.add_host_func("env", "gr_debug", funcs::debug);
        env_builder.add_host_func("env", "gr_gas_available", funcs::gas_available);
        env_builder.add_host_func("env", "gr_msg_id", funcs::msg_id);
        env_builder.add_host_func("env", "gr_leave", funcs::leave);
        env_builder.add_host_func("env", "gr_wait", funcs::wait);
        env_builder.add_host_func("env", "gr_wake", funcs::wake);
        env_builder.add_host_func("env", "gr_error", funcs::error);
        env_builder.add_host_func("env", "gas", funcs::gas);

        let mut runtime = Runtime {
            ext: ext_carrier,
            memory: MemoryWrap::new(mem),
            trap: None,
            termination_reason: None,
        };

        let instance = match Instance::new(binary, &env_builder, &mut runtime) {
            Ok(inst) => inst,
            Err(e) => {
                return Err(BackendError {
                    reason: SandboxEnvironmentError::ModuleInstantiation,
                    description: Some(format!("{:?}", e).into()),
                    gas_amount: runtime.ext.into_inner().into_gas_amount(),
                })
            }
        };

        log::debug!(
            "wasm mem addr = {:#x?}",
            runtime.memory.get_buffer_host_addr()
        );

        let entries = match get_module_exports(binary) {
            Ok(entries) => entries,
            Err(e) => {
                return Err(BackendError {
                    reason: SandboxEnvironmentError::GetWasmExports,
                    description: Some(format!("{:?}", e).into()),
                    gas_amount: runtime.ext.into_inner().into_gas_amount(),
                })
            }
        };

        // Set module memory.
        if let Err(e) = set_pages(&mut runtime.memory, memory_pages) {
            return Err(BackendError {
                reason: SandboxEnvironmentError::SetModuleMemoryData,
                description: Some(format!("{:?}", e).into()),
                gas_amount: runtime.ext.into_inner().into_gas_amount(),
            });
        }

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

    fn get_mem(&self) -> &dyn Memory {
        &self.runtime.memory
    }

    fn execute<F, T>(
        mut self,
        entry_point: &str,
        post_execution_handler: F,
    ) -> Result<BackendReport, BackendError<Self::Error>>
    where
        F: FnOnce(&dyn Memory) -> Result<(), T>,
        T: fmt::Display,
    {
        let res = if self.entries.contains(&String::from(entry_point)) {
            self.instance.invoke(entry_point, &[], &mut self.runtime)
        } else {
            Ok(ReturnValue::Unit)
        };

        let Runtime {
            ext,
            memory,
            trap,
            termination_reason,
        } = self.runtime;

        log::debug!("execution res = {:?}", res);

        let info = ext
            .into_inner()
            .into_ext_info(&memory)
            .map_err(|(reason, gas_amount)| BackendError {
                reason: SandboxEnvironmentError::Memory(reason),
                description: None,
                gas_amount,
            })?;

        let termination = if res.is_err() {
            let reason = match termination_reason {
                Some(TerminationReasonKind::Exit) => {
                    info.exit_argument.map(TerminationReason::Exit)
                }
                Some(TerminationReasonKind::Wait) => Some(TerminationReason::Wait),
                Some(TerminationReasonKind::Leave) => Some(TerminationReason::Leave),
                Some(TerminationReasonKind::GasAllowanceExceeded) => {
                    Some(TerminationReason::GasAllowanceExceeded)
                }
                None => None,
            };

            reason.unwrap_or_else(|| TerminationReason::Trap {
                explanation: info.trap_explanation.clone(),
                description: trap.map(|e| e.to_string()).map(Into::into),
            })
        } else {
            TerminationReason::Success
        };

        match post_execution_handler(&memory) {
            Ok(_) => Ok(BackendReport { termination, info }),
            Err(e) => Err(BackendError {
                reason: SandboxEnvironmentError::PostExecutionHandler(e.to_string()),
                description: None,
                gas_amount: info.gas_amount,
            }),
        }
    }

    fn into_gas_amount(self) -> GasAmount {
        self.runtime.ext.into_inner().into_gas_amount()
    }
}
