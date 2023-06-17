// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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
    memory::MemoryWrap,
    runtime::{self, Runtime},
};
use alloc::{collections::BTreeSet, format};
use core::{convert::Infallible, fmt::Display};
use gear_backend_common::{
    funcs::FuncsHandler,
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
    HostError, HostFuncType, InstanceGlobals, ReturnValue, SandboxEnvironmentBuilder,
    SandboxInstance, SandboxMemory, Value,
};

#[derive(Clone, Copy)]
struct SandboxValue(Value);

impl From<i32> for SandboxValue {
    fn from(value: i32) -> Self {
        SandboxValue(Value::I32(value))
    }
}

impl From<u32> for SandboxValue {
    fn from(value: u32) -> Self {
        SandboxValue(Value::I32(value as i32))
    }
}

impl From<i64> for SandboxValue {
    fn from(value: i64) -> Self {
        SandboxValue(Value::I64(value))
    }
}

impl TryFrom<SandboxValue> for u32 {
    type Error = HostError;

    fn try_from(val: SandboxValue) -> Result<u32, HostError> {
        if let Value::I32(val) = val.0 {
            Ok(val as u32)
        } else {
            Err(HostError)
        }
    }
}

impl TryFrom<SandboxValue> for u64 {
    type Error = HostError;

    fn try_from(val: SandboxValue) -> Result<u64, HostError> {
        if let Value::I64(val) = val.0 {
            Ok(val as u64)
        } else {
            Err(HostError)
        }
    }
}

macro_rules! wrap_common_func_internal_ret{
    ($func:path, $($arg_no:expr),*) => {
        |ctx, args| -> Result<ReturnValue, HostError> {
            $func(ctx, $(SandboxValue(args[$arg_no]).try_into()?,)*).map(|ret| Into::<SandboxValue>::into(ret).0.into())
        }
    }
}

macro_rules! wrap_common_func_internal_no_ret{
    ($func:path, $($arg_no:expr),*) => {
        |ctx, _args| -> Result<ReturnValue, HostError> {
            $func(ctx, $(SandboxValue(_args[$arg_no]).try_into()?,)*).map(|_| ReturnValue::Unit)
        }
    }
}

#[rustfmt::skip]
macro_rules! wrap_common_func {
    ($func:path, () -> ()) =>   { wrap_common_func_internal_no_ret!($func,) };
    ($func:path, (1) -> ()) =>  { wrap_common_func_internal_no_ret!($func, 0) };
    ($func:path, (2) -> ()) =>  { wrap_common_func_internal_no_ret!($func, 0, 1) };
    ($func:path, (3) -> ()) =>  { wrap_common_func_internal_no_ret!($func, 0, 1, 2) };
    ($func:path, (4) -> ()) =>  { wrap_common_func_internal_no_ret!($func, 0, 1, 2, 3) };
    ($func:path, (5) -> ()) =>  { wrap_common_func_internal_no_ret!($func, 0, 1, 2, 3, 4) };
    ($func:path, (6) -> ()) =>  { wrap_common_func_internal_no_ret!($func, 0, 1, 2, 3, 4, 5) };
    ($func:path, (7) -> ()) =>  { wrap_common_func_internal_no_ret!($func, 0, 1, 2, 3, 4, 5, 6) };
    ($func:path, (8) -> ()) =>  { wrap_common_func_internal_no_ret!($func, 0, 1, 2, 3, 4, 5, 6, 7) };

    ($func:path, () -> (1)) =>  { wrap_common_func_internal_ret!($func,) };
    ($func:path, (1) -> (1)) => { wrap_common_func_internal_ret!($func, 0) };
    ($func:path, (2) -> (1)) => { wrap_common_func_internal_ret!($func, 0, 1) };
    ($func:path, (3) -> (1)) => { wrap_common_func_internal_ret!($func, 0, 1, 2) };
    ($func:path, (4) -> (1)) => { wrap_common_func_internal_ret!($func, 0, 1, 2, 3) };
    ($func:path, (5) -> (1)) => { wrap_common_func_internal_ret!($func, 0, 1, 2, 3, 4) };
    ($func:path, (6) -> (1)) => { wrap_common_func_internal_ret!($func, 0, 1, 2, 3, 4, 5) };
    ($func:path, (7) -> (1)) => { wrap_common_func_internal_ret!($func, 0, 1, 2, 3, 4, 5, 6) };
}

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
            self.env_def_builder.add_host_func(
                "env",
                name.to_str(),
                wrap_common_func!(FuncsHandler::forbidden, () -> ()),
            );
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

impl<E, EP> SandboxEnvironment<E, EP>
where
    E: BackendExt + 'static,
    E::Error: BackendExtError,
    E::AllocError: BackendAllocExtError<ExtError = E::Error>,
    EP: WasmEntry,
{
    #[rustfmt::skip]
    fn bind_funcs(builder: &mut EnvBuilder<E>) {
        builder.add_func(BlockHeight, wrap_common_func!(FuncsHandler::block_height, (1) -> ()));
        builder.add_func(BlockTimestamp,wrap_common_func!(FuncsHandler::block_timestamp, (1) -> ()));
        builder.add_func(CreateProgram, wrap_common_func!(FuncsHandler::create_program, (7) -> ()));
        builder.add_func(CreateProgramWGas, wrap_common_func!(FuncsHandler::create_program_wgas, (8) -> ()));
        builder.add_func(Debug, wrap_common_func!(FuncsHandler::debug, (2) -> ()));
        builder.add_func(Panic, wrap_common_func!(FuncsHandler::panic, (2) -> ()));
        builder.add_func(OomPanic, wrap_common_func!(FuncsHandler::oom_panic, () -> ()));
        builder.add_func(Error, wrap_common_func!(FuncsHandler::error, (2) -> ()));
        builder.add_func(Exit, wrap_common_func!(FuncsHandler::exit, (1) -> ()));
        builder.add_func(ReplyCode, wrap_common_func!(FuncsHandler::reply_code, (1) -> ()));
        builder.add_func(SignalCode, wrap_common_func!(FuncsHandler::signal_code, (1) -> ()));
        builder.add_func(ReserveGas, wrap_common_func!(FuncsHandler::reserve_gas, (3) -> ()));
        builder.add_func(ReplyDeposit, wrap_common_func!(FuncsHandler::reply_deposit, (3) -> ()));
        builder.add_func(UnreserveGas, wrap_common_func!(FuncsHandler::unreserve_gas, (2) -> ()));
        builder.add_func(GasAvailable, wrap_common_func!(FuncsHandler::gas_available, (1) -> ()));
        builder.add_func(Leave, wrap_common_func!(FuncsHandler::leave, () -> ()));
        builder.add_func(MessageId, wrap_common_func!(FuncsHandler::message_id, (1) -> ()));
        builder.add_func(Origin, wrap_common_func!(FuncsHandler::origin, (1) -> ()));
        builder.add_func(PayProgramRent, wrap_common_func!(FuncsHandler::pay_program_rent, (2) -> ()));
        builder.add_func(ProgramId, wrap_common_func!(FuncsHandler::program_id, (1) -> ()));
        builder.add_func(Random, wrap_common_func!(FuncsHandler::random, (2) -> ()));
        builder.add_func(Read, wrap_common_func!(FuncsHandler::read, (4) -> ()));
        builder.add_func(Reply, wrap_common_func!(FuncsHandler::reply, (5) -> ()));
        builder.add_func(ReplyCommit, wrap_common_func!(FuncsHandler::reply_commit, (3) -> ()));
        builder.add_func(ReplyCommitWGas, wrap_common_func!(FuncsHandler::reply_commit_wgas, (4) -> ()));
        builder.add_func(ReplyPush, wrap_common_func!(FuncsHandler::reply_push, (3) -> ()));
        builder.add_func(ReplyTo, wrap_common_func!(FuncsHandler::reply_to, (1) -> ()));
        builder.add_func(SignalFrom, wrap_common_func!(FuncsHandler::signal_from, (1) -> ()));
        builder.add_func(ReplyWGas, wrap_common_func!(FuncsHandler::reply_wgas, (6) -> ()));
        builder.add_func(ReplyInput, wrap_common_func!(FuncsHandler::reply_input, (5) -> ()));
        builder.add_func(ReplyPushInput, wrap_common_func!(FuncsHandler::reply_push_input, (3) -> ()));
        builder.add_func(ReplyInputWGas, wrap_common_func!(FuncsHandler::reply_input_wgas, (6) -> ()));
        builder.add_func(Send, wrap_common_func!(FuncsHandler::send, (5) -> ()));
        builder.add_func(SendCommit, wrap_common_func!(FuncsHandler::send_commit, (4) -> ()));
        builder.add_func(SendCommitWGas, wrap_common_func!(FuncsHandler::send_commit_wgas, (5) -> ()));
        builder.add_func(SendInit, wrap_common_func!(FuncsHandler::send_init, (1) -> ()));
        builder.add_func(SendPush, wrap_common_func!(FuncsHandler::send_push, (4) -> ()));
        builder.add_func(SendWGas, wrap_common_func!(FuncsHandler::send_wgas, (6) -> ()));
        builder.add_func(SendInput, wrap_common_func!(FuncsHandler::send_input, (5) -> ()));
        builder.add_func(SendPushInput, wrap_common_func!(FuncsHandler::send_push_input, (4) -> ()));
        builder.add_func(SendInputWGas, wrap_common_func!(FuncsHandler::send_input_wgas, (6) -> ()));
        builder.add_func(Size, wrap_common_func!(FuncsHandler::size, (1) -> ()));
        builder.add_func(Source, wrap_common_func!(FuncsHandler::source, (1) -> ()));
        builder.add_func(Value, wrap_common_func!(FuncsHandler::value, (1) -> ()));
        builder.add_func(ValueAvailable, wrap_common_func!(FuncsHandler::value_available, (1) -> ()));
        builder.add_func(Wait, wrap_common_func!(FuncsHandler::wait, () -> ()));
        builder.add_func(WaitFor, wrap_common_func!(FuncsHandler::wait_for, (1) -> ()));
        builder.add_func(WaitUpTo, wrap_common_func!(FuncsHandler::wait_up_to, (1) -> ()));
        builder.add_func(Wake, wrap_common_func!(FuncsHandler::wake, (3) -> ()));
        builder.add_func(SystemReserveGas, wrap_common_func!(FuncsHandler::system_reserve_gas, (2) -> ()));
        builder.add_func(ReservationReply, wrap_common_func!(FuncsHandler::reservation_reply, (5) -> ()));
        builder.add_func(ReservationReplyCommit, wrap_common_func!(FuncsHandler::reservation_reply_commit, (3) -> ()));
        builder.add_func(ReservationSend, wrap_common_func!(FuncsHandler::reservation_send, (5) -> ()));
        builder.add_func(ReservationSendCommit, wrap_common_func!(FuncsHandler::reservation_send_commit, (4) -> ()));
        builder.add_func(OutOfGas, wrap_common_func!(FuncsHandler::out_of_gas, () -> ()));
        builder.add_func(OutOfAllowance, wrap_common_func!(FuncsHandler::out_of_allowance, () -> ()));

        builder.add_func(Alloc, wrap_common_func!(FuncsHandler::alloc, (1) -> (1)));
        builder.add_func(Free, wrap_common_func!(FuncsHandler::free, (1) -> (1)));
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

        let memory: DefaultExecutorMemory = match SandboxMemory::new(mem_size.raw(), None) {
            Ok(mem) => mem,
            Err(e) => return Err(System(CreateEnvMemory(e))),
        };

        builder.add_memory(memory.clone());

        Self::bind_funcs(&mut builder);

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
