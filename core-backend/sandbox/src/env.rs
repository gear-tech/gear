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
    runtime::{self, Runtime},
};
use alloc::{
    collections::BTreeSet,
    string::{String, ToString},
};
use core::fmt;
use gear_backend_common::{
    calc_stack_end,
    error_processor::IntoExtError,
    AsTerminationReason, BackendReport, Environment, GetGasAmount, IntoExtInfo, StackEndError,
    SysCallName::{self, *},
    TerminationReason, TrapExplanation, STACK_END_EXPORT_NAME,
};
use gear_core::{env::Ext, gas::GasAmount, memory::WasmPageNumber, message::DispatchKind};
use gear_wasm_instrument::{GLOBAL_NAME_ALLOWANCE, GLOBAL_NAME_GAS};
use sp_sandbox::{
    default_executor::{EnvironmentDefinitionBuilder, Instance, Memory as DefaultExecutorMemory},
    HostFuncType, InstanceGlobals, ReturnValue, SandboxEnvironmentBuilder, SandboxInstance,
    SandboxMemory, Value,
};

#[derive(Debug, derive_more::Display, derive_more::From)]
pub enum SandboxEnvironmentError {
    #[display(fmt = "Failed to create env memory: {_0:?}")]
    CreateEnvMemory(sp_sandbox::Error),
    #[display(fmt = "Unable to instantiate module: {_0:?}")]
    ModuleInstantiation(sp_sandbox::Error),
    #[display(fmt = "Unable to get wasm module exports: {_0}")]
    GetWasmExports(String),
    #[display(fmt = "Unable to set module memory data")]
    SetModuleMemoryData,
    #[display(fmt = "Unable to save static pages initial data")]
    SaveStaticPagesInitialData,
    #[display(fmt = "{_0}")]
    PreExecutionHandler(String),
    #[from]
    StackEnd(StackEndError),
    #[display(fmt = "Mutable globals are not supported")]
    MutableGlobalsNotSupported,
    #[display(fmt = "Gas counter not found or has wrong type")]
    WrongInjectedGas,
    #[display(fmt = "Allowance counter not found or has wrong type")]
    WrongInjectedAllowance,
}

#[derive(Debug, derive_more::Display, derive_more::From)]
#[display(fmt = "{error}")]
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
pub struct SandboxEnvironment<E: Ext> {
    instance: Instance<Runtime<E>>,
    runtime: Runtime<E>,
    entries: BTreeSet<DispatchKind>,
    entry_point: DispatchKind,
}

// A helping wrapper for `EnvironmentDefinitionBuilder` and `forbidden_funcs`.
// It makes adding functions to `EnvironmentDefinitionBuilder` shorter.
struct EnvBuilder<E: Ext> {
    env_def_builder: EnvironmentDefinitionBuilder<Runtime<E>>,
    forbidden_funcs: BTreeSet<&'static str>,
    funcs_count: usize,
}

impl<E: Ext + IntoExtInfo<E::Error> + 'static> EnvBuilder<E> {
    fn add_func(&mut self, name: SysCallName, f: HostFuncType<Runtime<E>>)
    where
        E::Error: AsTerminationReason + IntoExtError,
    {
        let name = name.to_str();
        if self.forbidden_funcs.contains(name) {
            self.env_def_builder
                .add_host_func("env", name, Funcs::forbidden);
        } else {
            self.env_def_builder.add_host_func("env", name, f);
        }

        self.funcs_count += 1;
    }

    fn add_memory(&mut self, memory: DefaultExecutorMemory)
    where
        E::Error: AsTerminationReason + IntoExtError,
    {
        self.env_def_builder.add_memory("env", "memory", memory);
    }
}

impl<E: Ext> From<EnvBuilder<E>> for EnvironmentDefinitionBuilder<Runtime<E>> {
    fn from(builder: EnvBuilder<E>) -> Self {
        builder.env_def_builder
    }
}

impl<E> Environment<E> for SandboxEnvironment<E>
where
    E: Ext + IntoExtInfo<E::Error> + GetGasAmount + 'static,
    E::Error: AsTerminationReason + IntoExtError,
{
    type Memory = MemoryWrap;
    type Error = Error;

    fn new(
        ext: E,
        binary: &[u8],
        entry_point: DispatchKind,
        entries: BTreeSet<DispatchKind>,
        mem_size: WasmPageNumber,
    ) -> Result<Self, Self::Error> {
        use SandboxEnvironmentError::*;

        let mut builder = EnvBuilder::<E> {
            env_def_builder: EnvironmentDefinitionBuilder::new(),
            forbidden_funcs: ext
                .forbidden_funcs()
                .iter()
                .copied()
                .chain(entry_point.forbidden_funcs())
                .collect(),
            funcs_count: 0,
        };

        builder.add_func(BlockHeight, Funcs::block_height);
        builder.add_func(BlockTimestamp, Funcs::block_timestamp);
        builder.add_func(CreateProgram, Funcs::create_program);
        builder.add_func(CreateProgramWGas, Funcs::create_program_wgas);
        builder.add_func(Debug, Funcs::debug);
        builder.add_func(Error, Funcs::error);
        builder.add_func(Exit, Funcs::exit);
        builder.add_func(StatusCode, Funcs::status_code);
        builder.add_func(ReserveGas, Funcs::reserve_gas);
        builder.add_func(UnreserveGas, Funcs::unreserve_gas);
        builder.add_func(GasAvailable, Funcs::gas_available);
        builder.add_func(Leave, Funcs::leave);
        builder.add_func(MessageId, Funcs::message_id);
        builder.add_func(Origin, Funcs::origin);
        builder.add_func(ProgramId, Funcs::program_id);
        builder.add_func(Random, Funcs::random);
        builder.add_func(Read, Funcs::read);
        builder.add_func(Reply, Funcs::reply);
        builder.add_func(ReplyCommit, Funcs::reply_commit);
        builder.add_func(ReplyCommitWGas, Funcs::reply_commit_wgas);
        builder.add_func(ReplyPush, Funcs::reply_push);
        builder.add_func(ReplyTo, Funcs::reply_to);
        builder.add_func(SignalFrom, Funcs::signal_from);
        builder.add_func(ReplyWGas, Funcs::reply_wgas);
        builder.add_func(ReplyInput, Funcs::reply_input);
        builder.add_func(ReplyPushInput, Funcs::reply_push_input);
        builder.add_func(ReplyInputWGas, Funcs::reply_input_wgas);
        builder.add_func(Send, Funcs::send);
        builder.add_func(SendCommit, Funcs::send_commit);
        builder.add_func(SendCommitWGas, Funcs::send_commit_wgas);
        builder.add_func(SendInit, Funcs::send_init);
        builder.add_func(SendPush, Funcs::send_push);
        builder.add_func(SendWGas, Funcs::send_wgas);
        builder.add_func(SendInput, Funcs::send_input);
        builder.add_func(SendPushInput, Funcs::send_push_input);
        builder.add_func(SendInputWGas, Funcs::send_input_wgas);
        builder.add_func(Size, Funcs::size);
        builder.add_func(Source, Funcs::source);
        builder.add_func(Value, Funcs::value);
        builder.add_func(ValueAvailable, Funcs::value_available);
        builder.add_func(Wait, Funcs::wait);
        builder.add_func(WaitFor, Funcs::wait_for);
        builder.add_func(WaitUpTo, Funcs::wait_up_to);
        builder.add_func(Wake, Funcs::wake);
        builder.add_func(SystemReserveGas, Funcs::system_reserve_gas);
        builder.add_func(ReservationReply, Funcs::reservation_reply);
        builder.add_func(ReservationReplyCommit, Funcs::reservation_reply_commit);
        builder.add_func(ReservationSend, Funcs::reservation_send);
        builder.add_func(ReservationSendCommit, Funcs::reservation_send_commit);

        let memory: DefaultExecutorMemory = match SandboxMemory::new(mem_size.0, None) {
            Ok(mem) => mem,
            Err(e) => return Err((ext.gas_amount(), CreateEnvMemory(e)).into()),
        };

        builder.add_memory(memory.clone());
        builder.add_func(Alloc, Funcs::alloc);
        builder.add_func(Free, Funcs::free);
        builder.add_func(OutOfGas, Funcs::out_of_gas);
        builder.add_func(OutOfAllowance, Funcs::out_of_allowance);

        // Check that we have implementations for all the sys-calls.
        // This is intended to panic during any testing, when the
        // condition is not met.
        assert_eq!(
            builder.funcs_count,
            SysCallName::count(),
            "Not all existing sys-calls were added to the module's env."
        );

        let env_builder: EnvironmentDefinitionBuilder<_> = builder.into();

        let mut runtime = Runtime {
            ext,
            memory: MemoryWrap::new(memory),
            err: FuncError::Terminated(TerminationReason::Success),
            globals: Default::default(),
        };

        match Instance::new(binary, &env_builder, &mut runtime) {
            Ok(instance) => Ok(Self {
                instance,
                runtime,
                entries,
                entry_point,
            }),
            Err(e) => Err((runtime.ext.gas_amount(), ModuleInstantiation(e)).into()),
        }
    }

    fn execute<F, T>(
        self,
        pre_execution_handler: F,
    ) -> Result<BackendReport<Self::Memory, E>, Self::Error>
    where
        F: FnOnce(&mut Self::Memory, Option<WasmPageNumber>) -> Result<(), T>,
        T: fmt::Display,
    {
        use SandboxEnvironmentError::*;

        let Self {
            mut instance,
            mut runtime,
            entries,
            entry_point,
        } = self;

        let stack_end = instance
            .get_global_val(STACK_END_EXPORT_NAME)
            .and_then(|global| global.as_i32());
        let stack_end_page = match calc_stack_end(stack_end) {
            Ok(s) => s,
            Err(e) => return Err((runtime.ext.gas_amount(), StackEnd(e)).into()),
        };

        runtime.globals = instance
            .instance_globals()
            .ok_or((runtime.ext.gas_amount(), MutableGlobalsNotSupported))?;

        let (gas, allowance) = runtime.ext.counters();

        runtime
            .globals
            .set_global_val(GLOBAL_NAME_GAS, Value::I64(gas as i64))
            .map_err(|_| (runtime.ext.gas_amount(), WrongInjectedGas))?;

        runtime
            .globals
            .set_global_val(GLOBAL_NAME_ALLOWANCE, Value::I64(allowance as i64))
            .map_err(|_| (runtime.ext.gas_amount(), WrongInjectedAllowance))?;

        match pre_execution_handler(&mut runtime.memory, stack_end_page) {
            Ok(_) => (),
            Err(e) => {
                return Err((runtime.ext.gas_amount(), PreExecutionHandler(e.to_string())).into());
            }
        }

        let res = if entries.contains(&entry_point) {
            instance.invoke(entry_point.into_entry(), &[], &mut runtime)
        } else {
            Ok(ReturnValue::Unit)
        };

        let gas = runtime
            .globals
            .get_global_val(GLOBAL_NAME_GAS)
            .and_then(runtime::as_i64)
            .ok_or((runtime.ext.gas_amount(), WrongInjectedGas))?;

        let allowance = runtime
            .globals
            .get_global_val(GLOBAL_NAME_ALLOWANCE)
            .and_then(runtime::as_i64)
            .ok_or((runtime.ext.gas_amount(), WrongInjectedAllowance))?;

        let Runtime {
            err: trap,
            mut ext,
            memory,
            ..
        } = runtime;

        ext.update_counters(gas as u64, allowance as u64);

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
            memory_wrap: memory,
            ext,
        })
    }
}
