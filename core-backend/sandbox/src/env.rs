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

use crate::{
    funcs::{FuncError, FuncsHandler as Funcs},
    memory::MemoryWrap,
};
use alloc::{
    collections::BTreeSet,
    format,
    string::{String, ToString},
    vec::Vec,
};
use core::fmt;
use gear_backend_common::{
    error_processor::IntoExtError, AsTerminationReason, BackendError, BackendReport, Environment,
    IntoExtInfo, TerminationReason, TrapExplanation,
};
use gear_core::{
    env::{Ext, ExtCarrier},
    gas::GasAmount,
    memory::{Memory, WasmPageNumber},
};
use gear_core_errors::MemoryError;
use sp_sandbox::{
    default_executor::{EnvironmentDefinitionBuilder, Instance, Memory as DefaultExecutorMemory},
    HostFuncType, ReturnValue, SandboxEnvironmentBuilder, SandboxInstance, SandboxMemory,
};

#[derive(Debug, derive_more::Display)]
pub enum SandboxEnvironmentError {
    #[display(fmt = "Unable to instantiate module: {:?}", _0)]
    ModuleInstantiation(sp_sandbox::Error),
    #[display(fmt = "Unable to get wasm module exports: {}", _0)]
    GetWasmExports(String),
    #[display(fmt = "Unable to set module memory data")]
    SetModuleMemoryData,
    #[display(fmt = "Unable to save static pages initial data")]
    SaveStaticPagesInitialData,
    #[display(fmt = "Failed to create env memory: {:?}", _0)]
    CreateEnvMemory(sp_sandbox::Error),
    #[display(fmt = "No trap explanation")]
    NoTrapExplanation,
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
    pub trap: FuncError<E::Error>,
}

// A helping wrapper for `EnvironmentDefinitionBuilder` and `forbidden_funcs`.
// It makes adding functions to `EnvironmentDefinitionBuilder` shorter.
struct EnvBuilder<'a, E: Ext> {
    env_def_builder: EnvironmentDefinitionBuilder<Runtime<E>>,
    forbidden_funcs: &'a BTreeSet<&'static str>,
}

impl<'a, E: Ext + IntoExtInfo + 'static> EnvBuilder<'a, E> {
    fn add_func(&mut self, name: &str, f: HostFuncType<Runtime<E>>)
    where
        E::Error: AsTerminationReason + IntoExtError,
    {
        if self.forbidden_funcs.contains(name) {
            self.env_def_builder
                .add_host_func("env", name, Funcs::forbidden);
        } else {
            self.env_def_builder.add_host_func("env", name, f);
        }
    }
}

impl<E: Ext> From<EnvBuilder<'_, E>> for EnvironmentDefinitionBuilder<Runtime<E>> {
    fn from(builder: EnvBuilder<E>) -> Self {
        builder.env_def_builder
    }
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

impl<E> Environment<E> for SandboxEnvironment<E>
where
    E: Ext + IntoExtInfo + 'static,
    E::Error: AsTerminationReason + IntoExtError,
{
    type Error = SandboxEnvironmentError;

    fn new(
        ext: E,
        binary: &[u8],
        mem_size: WasmPageNumber,
    ) -> Result<Self, BackendError<Self::Error>> {
        let mut builder = EnvBuilder::<E> {
            env_def_builder: EnvironmentDefinitionBuilder::new(),
            forbidden_funcs: ext.forbidden_funcs(),
        };

        builder.add_func("gr_block_height", Funcs::block_height);
        builder.add_func("gr_block_timestamp", Funcs::block_timestamp);
        builder.add_func("gr_create_program", Funcs::create_program);
        builder.add_func("gr_create_program_wgas", Funcs::create_program_wgas);
        builder.add_func("gr_debug", Funcs::debug);
        builder.add_func("gr_error", Funcs::error);
        builder.add_func("gr_exit", Funcs::exit);
        builder.add_func("gr_exit_code", Funcs::exit_code);
        builder.add_func("gr_gas_available", Funcs::gas_available);
        builder.add_func("gr_leave", Funcs::leave);
        builder.add_func("gr_msg_id", Funcs::msg_id);
        builder.add_func("gr_origin", Funcs::origin);
        builder.add_func("gr_program_id", Funcs::program_id);
        builder.add_func("gr_read", Funcs::read);
        builder.add_func("gr_reply", Funcs::reply);
        builder.add_func("gr_reply_commit", Funcs::reply_commit);
        builder.add_func("gr_reply_commit_wgas", Funcs::reply_commit_wgas);
        builder.add_func("gr_reply_push", Funcs::reply_push);
        builder.add_func("gr_reply_to", Funcs::reply_to);
        builder.add_func("gr_reply_wgas", Funcs::reply_wgas);
        builder.add_func("gr_send", Funcs::send);
        builder.add_func("gr_send_commit", Funcs::send_commit);
        builder.add_func("gr_send_commit_wgas", Funcs::send_commit_wgas);
        builder.add_func("gr_send_init", Funcs::send_init);
        builder.add_func("gr_send_push", Funcs::send_push);
        builder.add_func("gr_send_wgas", Funcs::send_wgas);
        builder.add_func("gr_size", Funcs::size);
        builder.add_func("gr_source", Funcs::source);
        builder.add_func("gr_value", Funcs::value);
        builder.add_func("gr_value_available", Funcs::value_available);
        builder.add_func("gr_wait", Funcs::wait);
        builder.add_func("gr_wake", Funcs::wake);
        let mut env_builder: EnvironmentDefinitionBuilder<_> = builder.into();

        let ext_carrier = ExtCarrier::new(ext);

        let mem: DefaultExecutorMemory = match SandboxMemory::new(mem_size.0, None) {
            Ok(mem) => mem,
            Err(e) => {
                return Err(BackendError {
                    reason: SandboxEnvironmentError::CreateEnvMemory(e),
                    gas_amount: ext_carrier.into_inner().into_gas_amount(),
                })
            }
        };

        env_builder.add_memory("env", "memory", mem.clone());
        env_builder.add_host_func("env", "alloc", Funcs::alloc);
        env_builder.add_host_func("env", "free", Funcs::free);
        env_builder.add_host_func("env", "gas", Funcs::gas);

        let mut runtime = Runtime {
            ext: ext_carrier,
            memory: MemoryWrap::new(mem),
            trap: FuncError::Terminated(TerminationReason::Success),
        };

        let instance = match Instance::new(binary, &env_builder, &mut runtime) {
            Ok(inst) => inst,
            Err(e) => {
                return Err(BackendError {
                    reason: SandboxEnvironmentError::ModuleInstantiation(e),
                    gas_amount: runtime.ext.into_inner().into_gas_amount(),
                })
            }
        };

        let entries = match get_module_exports(binary) {
            Ok(entries) => entries,
            Err(e) => {
                return Err(BackendError {
                    reason: SandboxEnvironmentError::GetWasmExports(e),
                    gas_amount: runtime.ext.into_inner().into_gas_amount(),
                })
            }
        };

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

    fn get_mem_mut(&mut self) -> &mut dyn Memory {
        &mut self.runtime.memory
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

        let Runtime { ext, memory, trap } = self.runtime;

        log::debug!("execution res = {:?}", res);

        let info = ext
            .into_inner()
            .into_ext_info(&memory)
            .map_err(|(reason, gas_amount)| BackendError {
                reason: SandboxEnvironmentError::Memory(reason),
                gas_amount,
            })?;

        let termination = if res.is_err() {
            let reason = info
                .trap_explanation
                .clone()
                .map(TerminationReason::Trap)
                .or_else(|| trap.to_termination_reason())
                .ok_or_else(|| BackendError {
                    reason: SandboxEnvironmentError::NoTrapExplanation,
                    gas_amount: info.gas_amount.clone(),
                })?;

            // success is unacceptable when there is error
            if let TerminationReason::Success = reason {
                TerminationReason::Trap(TrapExplanation::Unknown)
            } else {
                reason
            }
        } else {
            TerminationReason::Success
        };

        match post_execution_handler(&memory) {
            Ok(_) => Ok(BackendReport { termination, info }),
            Err(e) => Err(BackendError {
                reason: SandboxEnvironmentError::PostExecutionHandler(e.to_string()),
                gas_amount: info.gas_amount,
            }),
        }
    }

    fn into_gas_amount(self) -> GasAmount {
        self.runtime.ext.into_inner().into_gas_amount()
    }
}
