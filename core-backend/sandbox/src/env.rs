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
    funcs::FuncsHandler as Funcs,
    memory::MemoryWrap,
    runtime::{self, Runtime},
};
use alloc::{collections::BTreeSet, format};
use core::{convert::Infallible, fmt::Display};
use gear_backend_common::{
    lazy_pages::{GlobalsAccessConfig, GlobalsAccessMod},
    ActorTerminationReason, BackendAllocExtError, BackendExt, BackendExtError, BackendReport,
    BackendTermination, Environment, EnvironmentExecutionError, EnvironmentExecutionResult,
};
use gear_core::{
    gas::GasLeft,
    memory::{PageU32Size, WasmPage},
    message::{DispatchKind, WasmEntry},
};
use gear_wasm_instrument::{
    syscalls::SysCallName::{self, *},
    GLOBAL_NAME_ALLOWANCE, GLOBAL_NAME_GAS, STACK_END_EXPORT_NAME,
};
use sp_sandbox::{
    default_executor::{EnvironmentDefinitionBuilder, Instance, Memory as DefaultExecutorMemory},
    HostFuncType, InstanceGlobals, ReturnValue, SandboxEnvironmentBuilder, SandboxInstance,
    SandboxMemory, Value,
};

#[derive(Debug, derive_more::Display)]
pub enum SandboxEnvironmentError {
    #[display(fmt = "Failed to create env memory: {_0:?}")]
    CreateEnvMemory(sp_sandbox::Error),
    #[display(fmt = "Globals are not supported")]
    GlobalsNotSupported,
    #[display(fmt = "Gas counter not found or has wrong type")]
    WrongInjectedGas,
    #[display(fmt = "Allowance counter not found or has wrong type")]
    WrongInjectedAllowance,
}

/// Environment to run one module at a time providing Ext.
pub struct SandboxEnvironment<E, EP = DispatchKind>
where
    E: BackendExt,
    EP: WasmEntry,
{
    instance: Instance<Runtime<E>>,
    runtime: Runtime<E>,
    entries: BTreeSet<DispatchKind>,
    entry_point: EP,
}

// A helping wrapper for `EnvironmentDefinitionBuilder` and `forbidden_funcs`.
// It makes adding functions to `EnvironmentDefinitionBuilder` shorter.
struct EnvBuilder<E: BackendExt> {
    env_def_builder: EnvironmentDefinitionBuilder<Runtime<E>>,
    forbidden_funcs: BTreeSet<SysCallName>,
    funcs_count: usize,
}

impl<E> EnvBuilder<E>
where
    E: BackendExt + 'static,
    E::Error: BackendExtError,
    E::AllocError: BackendAllocExtError<ExtError = E::Error>,
{
    fn add_func(&mut self, name: SysCallName, f: HostFuncType<Runtime<E>>) {
        if self.forbidden_funcs.contains(&name) {
            self.env_def_builder
                .add_host_func("env", name.to_str(), Funcs::forbidden);
        } else {
            self.env_def_builder.add_host_func("env", name.to_str(), f);
        }

        self.funcs_count += 1;
    }

    fn add_memory(&mut self, memory: DefaultExecutorMemory) {
        self.env_def_builder.add_memory("env", "memory", memory);
    }
}

impl<E: BackendExt> From<EnvBuilder<E>> for EnvironmentDefinitionBuilder<Runtime<E>> {
    fn from(builder: EnvBuilder<E>) -> Self {
        builder.env_def_builder
    }
}

impl<E, EP> Environment<EP> for SandboxEnvironment<E, EP>
where
    E: BackendExt + 'static,
    E::Error: BackendExtError,
    E::AllocError: BackendAllocExtError<ExtError = E::Error>,
    EP: WasmEntry,
{
    type Ext = E;
    type Memory = MemoryWrap;
    type Error = SandboxEnvironmentError;

    fn new(
        ext: Self::Ext,
        binary: &[u8],
        entry_point: EP,
        entries: BTreeSet<DispatchKind>,
        mem_size: WasmPage,
    ) -> Result<Self, EnvironmentExecutionError<Self::Error, Infallible>> {
        use EnvironmentExecutionError::*;
        use SandboxEnvironmentError::*;

        let entry_forbidden = entry_point
            .try_into_kind()
            .as_ref()
            .map(DispatchKind::forbidden_funcs)
            .unwrap_or_default();

        let mut builder = EnvBuilder::<E> {
            env_def_builder: EnvironmentDefinitionBuilder::new(),
            forbidden_funcs: ext
                .forbidden_funcs()
                .iter()
                .copied()
                .chain(entry_forbidden)
                .collect(),
            funcs_count: 0,
        };

        builder.add_func(BlockHeight, Funcs::block_height);
        builder.add_func(BlockTimestamp, Funcs::block_timestamp);
        builder.add_func(CreateProgram, Funcs::create_program);
        builder.add_func(CreateProgramWGas, Funcs::create_program_wgas);
        builder.add_func(Debug, Funcs::debug);
        builder.add_func(Panic, Funcs::panic);
        builder.add_func(OomPanic, Funcs::oom_panic);
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

        let memory: DefaultExecutorMemory = match SandboxMemory::new(mem_size.raw(), None) {
            Ok(mem) => mem,
            Err(e) => return Err(System(CreateEnvMemory(e))),
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
            globals: Default::default(),
            memory_manager: Default::default(),
            fallible_syscall_error: None,
            termination_reason: ActorTerminationReason::Success.into(),
        };

        match Instance::new(binary, &env_builder, &mut runtime) {
            Ok(instance) => Ok(Self {
                instance,
                runtime,
                entries,
                entry_point,
            }),
            Err(e) => Err(Actor(runtime.ext.gas_amount(), format!("{e:?}"))),
        }
    }

    fn execute<F, T>(self, pre_execution_handler: F) -> EnvironmentExecutionResult<T, Self, EP>
    where
        F: FnOnce(&mut Self::Memory, Option<u32>, GlobalsAccessConfig) -> Result<(), T>,
        T: Display,
    {
        use EnvironmentExecutionError::*;
        use SandboxEnvironmentError::*;

        let Self {
            mut instance,
            mut runtime,
            entries,
            entry_point,
        } = self;

        let stack_end = instance
            .get_global_val(STACK_END_EXPORT_NAME)
            .and_then(|global| global.as_i32())
            .map(|global| global as u32);

        runtime.globals = instance
            .instance_globals()
            .ok_or(System(GlobalsNotSupported))?;

        let GasLeft { gas, allowance } = runtime.ext.gas_left();

        runtime
            .globals
            .set_global_val(GLOBAL_NAME_GAS, Value::I64(gas as i64))
            .map_err(|_| System(WrongInjectedGas))?;

        runtime
            .globals
            .set_global_val(GLOBAL_NAME_ALLOWANCE, Value::I64(allowance as i64))
            .map_err(|_| System(WrongInjectedAllowance))?;

        let globals_config = if cfg!(not(feature = "std")) {
            GlobalsAccessConfig {
                access_ptr: instance.get_instance_ptr(),
                access_mod: GlobalsAccessMod::WasmRuntime,
            }
        } else {
            unreachable!("We cannot use sandbox backend in std environment currently");
        };

        match pre_execution_handler(&mut runtime.memory, stack_end, globals_config) {
            Ok(_) => (),
            Err(e) => {
                return Err(PrepareMemory(runtime.ext.gas_amount(), e));
            }
        }

        let needs_execution = entry_point
            .try_into_kind()
            .map(|kind| entries.contains(&kind))
            .unwrap_or(true);

        let res = needs_execution
            .then(|| instance.invoke(entry_point.as_entry(), &[], &mut runtime))
            .unwrap_or(Ok(ReturnValue::Unit));

        let gas = runtime
            .globals
            .get_global_val(GLOBAL_NAME_GAS)
            .and_then(runtime::as_i64)
            .ok_or(System(WrongInjectedGas))?;

        let allowance = runtime
            .globals
            .get_global_val(GLOBAL_NAME_ALLOWANCE)
            .and_then(runtime::as_i64)
            .ok_or(System(WrongInjectedAllowance))?;

        let (ext, memory_wrap, termination_reason) = runtime.terminate(res, gas, allowance);

        Ok(BackendReport {
            termination_reason,
            memory_wrap,
            ext,
        })
    }
}
