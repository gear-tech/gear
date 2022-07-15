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
use alloc::{collections::BTreeSet, string::String};
use core::fmt;
use gear_backend_common::{
    error_processor::IntoExtError, AsTerminationReason, BackendError, Environment, IntoExtInfo,
    TerminationReason, TrapExplanation,
};
use gear_core::{
    env::{Ext, ExtCarrier},
    memory::WasmPageNumber,
    message::DispatchKind,
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
    #[display(fmt = "{}", _0)]
    Memory(MemoryError),
    #[display(fmt = "{}", _0)]
    PostExecutionHandler(String),
}

/// Environment to run one module at a time providing Ext.
pub struct SandboxEnvironment;

pub(crate) struct Runtime<'a, E: Ext> {
    pub ext: &'a mut ExtCarrier<E>,
    pub memory: MemoryWrap,
    pub err: FuncError<E::Error>,
}

// A helping wrapper for `EnvironmentDefinitionBuilder` and `forbidden_funcs`.
// It makes adding functions to `EnvironmentDefinitionBuilder` shorter.
struct EnvBuilder<'a, E: Ext> {
    env_def_builder: EnvironmentDefinitionBuilder<Runtime<'a, E>>,
    forbidden_funcs: &'a BTreeSet<&'static str>,
}

impl<'a, E: Ext + IntoExtInfo + 'static> EnvBuilder<'a, E> {
    fn add_func(&mut self, name: &str, f: HostFuncType<Runtime<'a, E>>)
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

impl<'a, E: Ext> From<EnvBuilder<'a, E>> for EnvironmentDefinitionBuilder<Runtime<'a, E>> {
    fn from(builder: EnvBuilder<'a, E>) -> Self {
        builder.env_def_builder
    }
}

impl<E> Environment<E> for SandboxEnvironment
where
    E: Ext + IntoExtInfo + 'static,
    E::Error: AsTerminationReason + IntoExtError,
{
    type Memory = MemoryWrap;
    type Error = SandboxEnvironmentError;

    fn execute<F, T>(
        ext: &mut ExtCarrier<E>,
        binary: &[u8],
        entries: BTreeSet<DispatchKind>,
        mem_size: WasmPageNumber,
        entry_point: &DispatchKind,
        pre_execution_handler: F,
    ) -> Result<(TerminationReason, Self::Memory, Option<WasmPageNumber>), BackendError<Self::Error>>
    where
        F: FnOnce(&mut Self::Memory) -> Result<(), T>,
        T: fmt::Display,
    {
        let mut builder = EnvBuilder::<E> {
            env_def_builder: EnvironmentDefinitionBuilder::new(),
            forbidden_funcs: &ext.with(|ext| ext.forbidden_funcs().clone()).unwrap(),
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

        let mem: DefaultExecutorMemory = match SandboxMemory::new(mem_size.0, None) {
            Ok(mem) => mem,
            Err(e) => {
                return Err(BackendError {
                    reason: SandboxEnvironmentError::CreateEnvMemory(e),
                })
            }
        };

        env_builder.add_memory("env", "memory", mem.clone());
        env_builder.add_host_func("env", "alloc", Funcs::alloc);
        env_builder.add_host_func("env", "free", Funcs::free);
        env_builder.add_host_func("env", "gas", Funcs::gas);

        let mut runtime = Runtime {
            ext: ext,
            memory: MemoryWrap::new(mem),
            err: FuncError::Terminated(TerminationReason::Success),
        };

        let mut instance = match Instance::new(binary, &env_builder, &mut runtime) {
            Ok(inst) => inst,
            Err(e) => {
                return Err(BackendError {
                    reason: SandboxEnvironmentError::ModuleInstantiation(e),
                })
            }
        };

        pre_execution_handler(&mut runtime.memory);

        let res = if entries.contains(entry_point) {
            instance.invoke(entry_point.into_entry(), &[], &mut runtime)
        } else {
            Ok(ReturnValue::Unit)
        };

        let Runtime {
            ext,
            memory,
            err: trap,
        } = runtime;

        log::debug!("execution res = {:?}", res);

        let trap_explanation = ext.with(|ext| ext.trap_explanation()).unwrap();

        let termination = if res.is_err() {
            let reason = trap_explanation
                .map(TerminationReason::Trap)
                .unwrap_or_else(|| trap.into_termination_reason());

            // success is unacceptable when there is error
            if let TerminationReason::Success = reason {
                TerminationReason::Trap(TrapExplanation::Unknown)
            } else {
                reason
            }
        } else {
            TerminationReason::Success
        };

        // '__gear_stack_end' export is inserted in wasm-proc or wasm-builder
        let stack_end_page = {
            let global = instance
                .get_global_val("__gear_stack_end")
                .and_then(|global| {
                    global.as_i32().and_then(|addr| {
                        if addr < 0 {
                            None
                        } else {
                            Some(WasmPageNumber(
                                (addr as usize / WasmPageNumber::size()) as u32,
                            ))
                        }
                    })
                });
            global
        };

        // match post_execution_handler(&memory) {
        //     Ok(_) => ,
        //     Err(e) => Err(BackendError {
        //         reason: SandboxEnvironmentError::PostExecutionHandler(e.to_string()),
        //     }),
        // }
        Ok((termination, memory, stack_end_page))
    }
}
