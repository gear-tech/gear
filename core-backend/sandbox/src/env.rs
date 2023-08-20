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

use crate::{memory::MemoryWrap, runtime, runtime::CallerWrap};
use alloc::{collections::BTreeSet, format};
use core::{any::Any, convert::Infallible, fmt::Display};
use gear_backend_common::{
    funcs::FuncsHandler,
    lazy_pages::{GlobalsAccessConfig, GlobalsAccessError, GlobalsAccessMod, GlobalsAccessor},
    runtime::RunFallibleError,
    state::{HostState, State},
    ActorTerminationReason, BackendAllocSyscallError, BackendExternalities, BackendReport,
    BackendSyscallError, BackendTermination, Environment, EnvironmentError,
    EnvironmentExecutionResult, LimitedStr,
};
use gear_core::{
    env::Externalities,
    memory::HostPointer,
    message::{DispatchKind, WasmEntryPoint},
    pages::{PageNumber, WasmPage},
};
use gear_sandbox::{
    default_executor::{
        EnvironmentDefinitionBuilder, Instance, Memory as DefaultExecutorMemory, Store,
    },
    AsContextExt, HostError, HostFuncType, IntoValue, ReturnValue, SandboxEnvironmentBuilder,
    SandboxInstance, SandboxMemory, SandboxStore, Value,
};
use gear_sandbox_env::WasmReturnValue;
use gear_wasm_instrument::{
    syscalls::SysCallName::{self, *},
    GLOBAL_NAME_GAS, STACK_END_EXPORT_NAME,
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
        |ctx, args: &[Value]| -> Result<WasmReturnValue, HostError> {
            let mut ctx = CallerWrap::prepare(ctx).map_err(|_| HostError)?;
            $func(&mut ctx, $(SandboxValue(args[$arg_no]).try_into()?,)*)
            .map(|(gas, r)| WasmReturnValue {
                gas: gas as i64,
                inner: r.into_value().into(),
            })
        }
    }
}

macro_rules! wrap_common_func_internal_no_ret{
    ($func:path, $($arg_no:expr),*) => {
        |ctx, _args: &[Value]| -> Result<WasmReturnValue, HostError> {
            let mut ctx = CallerWrap::prepare(ctx).map_err(|_| HostError)?;
            $func(&mut ctx, $(SandboxValue(_args[$arg_no]).try_into()?,)*)
            .map(|(gas, _)| WasmReturnValue {
                gas: gas as i64,
                inner: ReturnValue::Unit,
            })
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
    ($func:path, (9) -> ()) =>  { wrap_common_func_internal_no_ret!($func, 0, 1, 2, 3, 4, 5, 6, 7, 8) };
    ($func:path, (10) -> ()) =>  { wrap_common_func_internal_no_ret!($func, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9) };

    ($func:path, () -> (1)) =>  { wrap_common_func_internal_ret!($func,) };
    ($func:path, (1) -> (1)) => { wrap_common_func_internal_ret!($func, 0) };
    ($func:path, (2) -> (1)) => { wrap_common_func_internal_ret!($func, 0, 1) };
    ($func:path, (3) -> (1)) => { wrap_common_func_internal_ret!($func, 0, 1, 2) };
    ($func:path, (4) -> (1)) => { wrap_common_func_internal_ret!($func, 0, 1, 2, 3) };
    ($func:path, (5) -> (1)) => { wrap_common_func_internal_ret!($func, 0, 1, 2, 3, 4) };
    ($func:path, (6) -> (1)) => { wrap_common_func_internal_ret!($func, 0, 1, 2, 3, 4, 5) };
    ($func:path, (7) -> (1)) => { wrap_common_func_internal_ret!($func, 0, 1, 2, 3, 4, 5, 6) };
    ($func:path, (8) -> (1)) => { wrap_common_func_internal_ret!($func, 0, 1, 2, 3, 4, 5, 6, 7) };
    ($func:path, (9) -> (1)) => { wrap_common_func_internal_ret!($func, 0, 1, 2, 3, 4, 5, 6, 7, 8) };
}

fn store_host_state_mut<Ext>(
    store: &mut Store<HostState<Ext, DefaultExecutorMemory>>,
) -> &mut State<Ext, DefaultExecutorMemory> {
    store
        .data_mut()
        .as_mut()
        .unwrap_or_else(|| unreachable!("State must be set in `WasmiEnvironment::new`; qed"))
}

#[derive(Debug, derive_more::Display)]
pub enum SandboxEnvironmentError {
    #[display(fmt = "Failed to create env memory: {_0:?}")]
    CreateEnvMemory(gear_sandbox::Error),
    #[display(fmt = "Globals are not supported")]
    GlobalsNotSupported,
    #[display(fmt = "Gas counter not found or has wrong type")]
    WrongInjectedGas,
}

/// Environment to run one module at a time providing Ext.
pub struct SandboxEnvironment<Ext, EntryPoint = DispatchKind>
where
    Ext: BackendExternalities,
    EntryPoint: WasmEntryPoint,
{
    instance: Instance<HostState<Ext, DefaultExecutorMemory>>,
    entries: BTreeSet<DispatchKind>,
    entry_point: EntryPoint,
    store: Store<HostState<Ext, DefaultExecutorMemory>>,
    memory: DefaultExecutorMemory,
}

// A helping wrapper for `EnvironmentDefinitionBuilder` and `forbidden_funcs`.
// It makes adding functions to `EnvironmentDefinitionBuilder` shorter.
struct EnvBuilder<Ext: BackendExternalities> {
    env_def_builder: EnvironmentDefinitionBuilder<HostState<Ext, DefaultExecutorMemory>>,
    forbidden_funcs: BTreeSet<SysCallName>,
    funcs_count: usize,
}

impl<Ext> EnvBuilder<Ext>
where
    Ext: BackendExternalities + 'static,
    Ext::UnrecoverableError: BackendSyscallError,
    RunFallibleError: From<Ext::FallibleError>,
    Ext::AllocError: BackendAllocSyscallError<ExtError = Ext::UnrecoverableError>,
{
    fn add_func(
        &mut self,
        name: SysCallName,
        f: HostFuncType<HostState<Ext, DefaultExecutorMemory>>,
    ) {
        if self.forbidden_funcs.contains(&name) {
            self.env_def_builder.add_host_func(
                "env",
                name.to_str(),
                wrap_common_func!(FuncsHandler::forbidden, (1) -> ()),
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

impl<Ext: BackendExternalities> From<EnvBuilder<Ext>>
    for EnvironmentDefinitionBuilder<HostState<Ext, DefaultExecutorMemory>>
{
    fn from(builder: EnvBuilder<Ext>) -> Self {
        builder.env_def_builder
    }
}

impl<Ext, EntryPoint> SandboxEnvironment<Ext, EntryPoint>
where
    Ext: BackendExternalities + 'static,
    Ext::UnrecoverableError: BackendSyscallError,
    RunFallibleError: From<Ext::FallibleError>,
    Ext::AllocError: BackendAllocSyscallError<ExtError = Ext::UnrecoverableError>,
    EntryPoint: WasmEntryPoint,
{
    #[rustfmt::skip]
    fn bind_funcs(builder: &mut EnvBuilder<Ext>) {
        builder.add_func(BlockHeight, wrap_common_func!(FuncsHandler::block_height, (2) -> ()));
        builder.add_func(BlockTimestamp,wrap_common_func!(FuncsHandler::block_timestamp, (2) -> ()));
        builder.add_func(CreateProgram, wrap_common_func!(FuncsHandler::create_program, (8) -> ()));
        builder.add_func(CreateProgramWGas, wrap_common_func!(FuncsHandler::create_program_wgas, (9) -> ()));
        builder.add_func(Debug, wrap_common_func!(FuncsHandler::debug, (3) -> ()));
        builder.add_func(Panic, wrap_common_func!(FuncsHandler::panic, (3) -> ()));
        builder.add_func(OomPanic, wrap_common_func!(FuncsHandler::oom_panic, (1) -> ()));
        builder.add_func(Exit, wrap_common_func!(FuncsHandler::exit, (2) -> ()));
        builder.add_func(ReplyCode, wrap_common_func!(FuncsHandler::reply_code, (2) -> ()));
        builder.add_func(SignalCode, wrap_common_func!(FuncsHandler::signal_code, (2) -> ()));
        builder.add_func(ReserveGas, wrap_common_func!(FuncsHandler::reserve_gas, (4) -> ()));
        builder.add_func(ReplyDeposit, wrap_common_func!(FuncsHandler::reply_deposit, (4) -> ()));
        builder.add_func(UnreserveGas, wrap_common_func!(FuncsHandler::unreserve_gas, (3) -> ()));
        builder.add_func(GasAvailable, wrap_common_func!(FuncsHandler::gas_available, (2) -> ()));
        builder.add_func(Leave, wrap_common_func!(FuncsHandler::leave, (1) -> ()));
        builder.add_func(MessageId, wrap_common_func!(FuncsHandler::message_id, (2) -> ()));
        builder.add_func(PayProgramRent, wrap_common_func!(FuncsHandler::pay_program_rent, (3) -> ()));
        builder.add_func(ProgramId, wrap_common_func!(FuncsHandler::program_id, (2) -> ()));
        builder.add_func(Random, wrap_common_func!(FuncsHandler::random, (3) -> ()));
        builder.add_func(Read, wrap_common_func!(FuncsHandler::read, (5) -> ()));
        builder.add_func(Reply, wrap_common_func!(FuncsHandler::reply, (5) -> ()));
        builder.add_func(ReplyCommit, wrap_common_func!(FuncsHandler::reply_commit, (3) -> ()));
        builder.add_func(ReplyCommitWGas, wrap_common_func!(FuncsHandler::reply_commit_wgas, (4) -> ()));
        builder.add_func(ReplyPush, wrap_common_func!(FuncsHandler::reply_push, (4) -> ()));
        builder.add_func(ReplyTo, wrap_common_func!(FuncsHandler::reply_to, (2) -> ()));
        builder.add_func(SignalFrom, wrap_common_func!(FuncsHandler::signal_from, (2) -> ()));
        builder.add_func(ReplyWGas, wrap_common_func!(FuncsHandler::reply_wgas, (6) -> ()));
        builder.add_func(ReplyInput, wrap_common_func!(FuncsHandler::reply_input, (5) -> ()));
        builder.add_func(ReplyPushInput, wrap_common_func!(FuncsHandler::reply_push_input, (4) -> ()));
        builder.add_func(ReplyInputWGas, wrap_common_func!(FuncsHandler::reply_input_wgas, (6) -> ()));
        builder.add_func(Send, wrap_common_func!(FuncsHandler::send, (6) -> ()));
        builder.add_func(SendCommit, wrap_common_func!(FuncsHandler::send_commit, (5) -> ()));
        builder.add_func(SendCommitWGas, wrap_common_func!(FuncsHandler::send_commit_wgas, (6) -> ()));
        builder.add_func(SendInit, wrap_common_func!(FuncsHandler::send_init, (2) -> ()));
        builder.add_func(SendPush, wrap_common_func!(FuncsHandler::send_push, (5) -> ()));
        builder.add_func(SendWGas, wrap_common_func!(FuncsHandler::send_wgas, (7) -> ()));
        builder.add_func(SendInput, wrap_common_func!(FuncsHandler::send_input, (6) -> ()));
        builder.add_func(SendPushInput, wrap_common_func!(FuncsHandler::send_push_input, (5) -> ()));
        builder.add_func(SendInputWGas, wrap_common_func!(FuncsHandler::send_input_wgas, (7) -> ()));
        builder.add_func(Size, wrap_common_func!(FuncsHandler::size, (2) -> ()));
        builder.add_func(Source, wrap_common_func!(FuncsHandler::source, (2) -> ()));
        builder.add_func(Value, wrap_common_func!(FuncsHandler::value, (2) -> ()));
        builder.add_func(ValueAvailable, wrap_common_func!(FuncsHandler::value_available, (2) -> ()));
        builder.add_func(Wait, wrap_common_func!(FuncsHandler::wait, (1) -> ()));
        builder.add_func(WaitFor, wrap_common_func!(FuncsHandler::wait_for, (2) -> ()));
        builder.add_func(WaitUpTo, wrap_common_func!(FuncsHandler::wait_up_to, (2) -> ()));
        builder.add_func(Wake, wrap_common_func!(FuncsHandler::wake, (4) -> ()));
        builder.add_func(SystemReserveGas, wrap_common_func!(FuncsHandler::system_reserve_gas, (3) -> ()));
        builder.add_func(ReservationReply, wrap_common_func!(FuncsHandler::reservation_reply, (5) -> ()));
        builder.add_func(ReservationReplyCommit, wrap_common_func!(FuncsHandler::reservation_reply_commit, (3) -> ()));
        builder.add_func(ReservationSend, wrap_common_func!(FuncsHandler::reservation_send, (6) -> ()));
        builder.add_func(ReservationSendCommit, wrap_common_func!(FuncsHandler::reservation_send_commit, (5) -> ()));
        builder.add_func(OutOfGas, wrap_common_func!(FuncsHandler::out_of_gas, (1) -> ()));

        builder.add_func(Alloc, wrap_common_func!(FuncsHandler::alloc, (2) -> (1)));
        builder.add_func(Free, wrap_common_func!(FuncsHandler::free, (2) -> (1)));
    }
}

struct GlobalsAccessProvider<Ext: Externalities> {
    instance: Instance<HostState<Ext, DefaultExecutorMemory>>,
    store: Option<Store<HostState<Ext, DefaultExecutorMemory>>>,
}

impl<Ext: Externalities + 'static> GlobalsAccessor for GlobalsAccessProvider<Ext> {
    fn get_i64(&self, name: &LimitedStr) -> Result<i64, GlobalsAccessError> {
        let store = self.store.as_ref().ok_or(GlobalsAccessError)?;
        self.instance
            .get_global_val(store, name.as_str())
            .and_then(runtime::as_i64)
            .ok_or(GlobalsAccessError)
    }

    fn set_i64(&mut self, name: &LimitedStr, value: i64) -> Result<(), GlobalsAccessError> {
        let store = self.store.as_mut().ok_or(GlobalsAccessError)?;
        self.instance
            .set_global_val(store, name.as_str(), Value::I64(value))
            .map_err(|_| GlobalsAccessError)
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl<EnvExt, EntryPoint> Environment<EntryPoint> for SandboxEnvironment<EnvExt, EntryPoint>
where
    EnvExt: BackendExternalities + 'static,
    EnvExt::UnrecoverableError: BackendSyscallError,
    RunFallibleError: From<EnvExt::FallibleError>,
    EnvExt::AllocError: BackendAllocSyscallError<ExtError = EnvExt::UnrecoverableError>,
    EntryPoint: WasmEntryPoint,
{
    type Ext = EnvExt;
    type Memory = MemoryWrap<EnvExt>;
    type SystemError = SandboxEnvironmentError;

    fn new(
        ext: Self::Ext,
        binary: &[u8],
        entry_point: EntryPoint,
        entries: BTreeSet<DispatchKind>,
        mem_size: WasmPage,
    ) -> Result<Self, EnvironmentError<Self::SystemError, Infallible>> {
        use EnvironmentError::*;
        use SandboxEnvironmentError::*;

        let entry_forbidden = entry_point
            .try_into_kind()
            .as_ref()
            .map(DispatchKind::forbidden_funcs)
            .unwrap_or_default();

        let mut store = Store::new(None);

        let mut builder = EnvBuilder::<EnvExt> {
            env_def_builder: EnvironmentDefinitionBuilder::new(),
            forbidden_funcs: ext
                .forbidden_funcs()
                .iter()
                .copied()
                .chain(entry_forbidden)
                .collect(),
            funcs_count: 0,
        };

        let memory: DefaultExecutorMemory =
            match SandboxMemory::new(&mut store, mem_size.raw(), None) {
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

        *store.data_mut() = Some(State {
            ext,
            memory: memory.clone(),
            termination_reason: ActorTerminationReason::Success.into(),
        });

        let instance = Instance::new(&mut store, binary, &env_builder).map_err(|e| {
            Actor(
                store_host_state_mut(&mut store).ext.gas_amount(),
                format!("{e:?}"),
            )
        })?;

        Ok(Self {
            instance,
            entries,
            entry_point,
            store,
            memory,
        })
    }

    fn execute<PrepareMemory, PrepareMemoryError>(
        self,
        prepare_memory: PrepareMemory,
    ) -> EnvironmentExecutionResult<PrepareMemoryError, Self, EntryPoint>
    where
        PrepareMemory: FnOnce(
            &mut Self::Memory,
            Option<u32>,
            GlobalsAccessConfig,
        ) -> Result<(), PrepareMemoryError>,
        PrepareMemoryError: Display,
    {
        use EnvironmentError::*;
        use SandboxEnvironmentError::*;

        let Self {
            mut instance,
            entries,
            entry_point,
            mut store,
            memory,
        } = self;

        let stack_end = instance
            .get_global_val(&store, STACK_END_EXPORT_NAME)
            .and_then(|global| global.as_i32())
            .map(|global| global as u32);

        let gas = store_host_state_mut(&mut store)
            .ext
            .define_current_counter();

        instance
            .set_global_val(&mut store, GLOBAL_NAME_GAS, Value::I64(gas as i64))
            .map_err(|_| System(WrongInjectedGas))?;

        let mut globals_provider = GlobalsAccessProvider {
            instance: instance.clone(),
            store: None,
        };
        let globals_provider_dyn_ref = &mut globals_provider as &mut dyn GlobalsAccessor;

        // Pointer to the globals access provider is valid until the end of `invoke` method.
        // So, we can safely use it inside lazy-pages and be sure that it points to the valid object.
        // We cannot guaranty that `store` (and so globals also) will be in a valid state,
        // because executor mut-borrows `store` during execution. But if it's in a valid state
        // each moment when protect memory signal can occur, than this trick is pretty safe.
        let globals_access_ptr = &globals_provider_dyn_ref as *const _ as HostPointer;

        let globals_config = if cfg!(not(feature = "std")) {
            GlobalsAccessConfig {
                access_ptr: instance.get_instance_ptr(),
                access_mod: GlobalsAccessMod::WasmRuntime,
            }
        } else {
            GlobalsAccessConfig {
                access_ptr: globals_access_ptr,
                access_mod: GlobalsAccessMod::NativeRuntime,
            }
        };

        let mut memory_wrap = MemoryWrap::new(memory.clone(), store);
        prepare_memory(&mut memory_wrap, stack_end, globals_config).map_err(|e| {
            let store = &mut memory_wrap.store;
            PrepareMemory(store_host_state_mut(store).ext.gas_amount(), e)
        })?;

        let needs_execution = entry_point
            .try_into_kind()
            .map(|kind| entries.contains(&kind))
            .unwrap_or(true);

        let mut store = memory_wrap.into_store();
        let res = if needs_execution {
            let store_option = &mut globals_provider_dyn_ref
                .as_any_mut()
                .downcast_mut::<GlobalsAccessProvider<EnvExt>>()
                // Provider is `GlobalsAccessProvider`, so panic is impossible.
                .unwrap_or_else(|| unreachable!("Provider must be `GlobalsAccessProvider`"))
                .store;

            store_option.replace(store);

            let res = instance.invoke(
                store_option
                    .as_mut()
                    // We set store above, so panic is impossible.
                    .unwrap_or_else(|| unreachable!("Store must be set before")),
                entry_point.as_entry(),
                &[],
            );

            store = globals_provider.store.take().unwrap();

            res
        } else {
            Ok(ReturnValue::Unit)
        };

        // Fetching global value.
        let gas = instance
            .get_global_val(&store, GLOBAL_NAME_GAS)
            .and_then(runtime::as_i64)
            .ok_or(System(WrongInjectedGas))? as u64;

        let state = store
            .data_mut()
            .take()
            .unwrap_or_else(|| unreachable!("State must be set in `WasmiEnvironment::new`; qed"));

        let (ext, termination_reason) = state.terminate(res, gas);

        Ok(BackendReport {
            termination_reason,
            memory_wrap: MemoryWrap::new(memory, store),
            ext,
        })
    }
}
