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
    runtime::Runtime,
};
use alloc::{
    collections::BTreeSet,
    string::{String, ToString},
};
use core::fmt;
use gear_backend_common::{
    calc_stack_end, error_processor::IntoExtError, AsTerminationReason, BackendReport, Environment,
    GetGasAmount, IntoExtInfo, StackEndError, TerminationReason, TrapExplanation,
    STACK_END_EXPORT_NAME,
};
use gear_core::{env::Ext, gas::GasAmount, memory::WasmPageNumber, message::DispatchKind};
use sp_sandbox::{
    default_executor::{EnvironmentDefinitionBuilder, Instance, Memory as DefaultExecutorMemory},
    HostFuncType, ReturnValue, SandboxEnvironmentBuilder, SandboxInstance, SandboxMemory,
};

#[derive(Debug, derive_more::Display, derive_more::From)]
pub enum SandboxEnvironmentError {
    #[display(fmt = "Failed to create env memory: {:?}", _0)]
    CreateEnvMemory(sp_sandbox::Error),
    #[display(fmt = "Unable to instantiate module: {:?}", _0)]
    ModuleInstantiation(sp_sandbox::Error),
    #[display(fmt = "Unable to get wasm module exports: {}", _0)]
    GetWasmExports(String),
    #[display(fmt = "Unable to set module memory data")]
    SetModuleMemoryData,
    #[display(fmt = "Unable to save static pages initial data")]
    SaveStaticPagesInitialData,
    #[display(fmt = "{}", _0)]
    PreExecutionHandler(String),
    #[from]
    StackEnd(StackEndError),
}

#[derive(Debug, derive_more::Display, derive_more::From)]
#[display(fmt = "{}", error)]
pub struct Error {
    gas_amount: GasAmount,
    error: SandboxEnvironmentError,
}

impl GetGasAmount for Error {
    fn gas_amount(&self) -> GasAmount {
        self.gas_amount.clone()
    }
}

/// Environment to run one module at a time providing Ext.
pub struct SandboxEnvironment;

// A helping wrapper for `EnvironmentDefinitionBuilder` and `forbidden_funcs`.
// It makes adding functions to `EnvironmentDefinitionBuilder` shorter.
struct EnvBuilder<'a, E: Ext> {
    env_def_builder: EnvironmentDefinitionBuilder<Runtime<'a, E>>,
    forbidden_funcs: &'a BTreeSet<&'static str>,
}

impl<'a, E: Ext + IntoExtInfo<E::Error> + 'static> EnvBuilder<'a, E> {
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
    E: Ext + IntoExtInfo<E::Error> + GetGasAmount + 'static,
    E::Error: AsTerminationReason + IntoExtError,
{
    type Memory = MemoryWrap;
    type Error = Error;

    fn execute<F, T>(
        mut ext: E,
        binary: &[u8],
        entries: BTreeSet<DispatchKind>,
        mem_size: WasmPageNumber,
        entry_point: &DispatchKind,
        pre_execution_handler: F,
    ) -> Result<BackendReport<Self::Memory, E>, Self::Error>
    where
        F: FnOnce(&mut Self::Memory, Option<WasmPageNumber>) -> Result<(), T>,
        T: fmt::Display,
    {
        use SandboxEnvironmentError::*;

        let mut builder = EnvBuilder::<E> {
            env_def_builder: EnvironmentDefinitionBuilder::new(),
            forbidden_funcs: &ext.forbidden_funcs().clone(),
        };

        builder.add_func("gr_block_height", Funcs::block_height);
        builder.add_func("gr_block_timestamp", Funcs::block_timestamp);
        builder.add_func("gr_create_program", Funcs::create_program);
        builder.add_func("gr_create_program_wgas", Funcs::create_program_wgas);
        builder.add_func("gr_debug", Funcs::debug);
        builder.add_func("gr_error", Funcs::error);
        builder.add_func("gr_exit", Funcs::exit);
        builder.add_func("gr_exit_code", Funcs::exit_code);
        builder.add_func("gr_reserve_gas", Funcs::reserve_gas);
        builder.add_func("gr_unreserve_gas", Funcs::unreserve_gas);
        builder.add_func("gr_gas_available", Funcs::gas_available);
        builder.add_func("gr_leave", Funcs::leave);
        builder.add_func("gr_message_id", Funcs::message_id);
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
        builder.add_func("gr_wait_for", Funcs::wait_for);
        builder.add_func("gr_wait_up_to", Funcs::wait_up_to);
        builder.add_func("gr_wake", Funcs::wake);
        builder.add_func("gr_reserve_gas", Funcs::reserve_gas);
        builder.add_func("gr_unreserve_gas", Funcs::unreserve_gas);
        let mut env_builder: EnvironmentDefinitionBuilder<_> = builder.into();

        let mem: DefaultExecutorMemory = match SandboxMemory::new(mem_size.0, None) {
            Ok(mem) => mem,
            Err(e) => return Err((ext.gas_amount(), CreateEnvMemory(e)).into()),
        };

        env_builder.add_memory("env", "memory", mem.clone());
        env_builder.add_host_func("env", "alloc", Funcs::alloc);
        env_builder.add_host_func("env", "free", Funcs::free);
        env_builder.add_host_func("env", "gas", Funcs::gas);

        let mut memory_wrap = MemoryWrap::new(mem.clone());
        let mut runtime = Runtime {
            ext: &mut ext,
            memory: &mem,
            memory_wrap: &mut memory_wrap,
            err: FuncError::Terminated(TerminationReason::Success),
        };

        let mut instance = match Instance::new(binary, &env_builder, &mut runtime) {
            Ok(inst) => inst,
            Err(e) => return Err((runtime.ext.gas_amount(), ModuleInstantiation(e)).into()),
        };

        let stack_end = instance
            .get_global_val(STACK_END_EXPORT_NAME)
            .and_then(|global| global.as_i32());
        let stack_end_page = match calc_stack_end(stack_end) {
            Ok(s) => s,
            Err(e) => return Err((runtime.ext.gas_amount(), StackEnd(e)).into()),
        };

        match pre_execution_handler(runtime.memory_wrap, stack_end_page) {
            Ok(_) => (),
            Err(e) => {
                return Err((runtime.ext.gas_amount(), PreExecutionHandler(e.to_string())).into());
            }
        }

        let res = if entries.contains(entry_point) {
            instance.invoke(entry_point.into_entry(), &[], &mut runtime)
        } else {
            Ok(ReturnValue::Unit)
        };

        let Runtime { err: trap, .. } = runtime;
        drop(instance);

        log::debug!("SandboxEnvironment::execute res = {res:?}");

        let trap_explanation = ext.trap_explanation();

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

        Ok(BackendReport {
            termination_reason: termination,
            memory_wrap,
            ext,
        })
    }
}
