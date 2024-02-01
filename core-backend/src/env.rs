// This file is part of Gear.

// Copyright (C) 2021-2024 Gear Technologies Inc.
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
    error::{
        ActorTerminationReason, BackendAllocSyscallError, BackendSyscallError, RunFallibleError,
        TerminationReason,
    },
    funcs::FuncsHandler,
    memory::MemoryWrap,
    runtime,
    state::{HostState, State},
    BackendExternalities,
};
use alloc::{collections::BTreeSet, format, string::String};
use core::{
    any::Any,
    convert::Infallible,
    fmt::{Debug, Display},
};
use gear_core::{
    env::Externalities,
    gas::GasAmount,
    memory::HostPointer,
    message::{DispatchKind, WasmEntryPoint},
    pages::{PageNumber, WasmPage},
    str::LimitedStr,
};
use gear_lazy_pages_common::{
    GlobalsAccessConfig, GlobalsAccessError, GlobalsAccessMod, GlobalsAccessor,
};
use gear_sandbox::{
    default_executor::{
        EnvironmentDefinitionBuilder, Instance, Memory as DefaultExecutorMemory, Store,
    },
    AsContextExt, HostFuncType, ReturnValue, SandboxEnvironmentBuilder, SandboxInstance,
    SandboxMemory, SandboxStore, Value,
};
use gear_wasm_instrument::{
    syscalls::SyscallName::{self, *},
    GLOBAL_NAME_GAS, STACK_END_EXPORT_NAME,
};

// we have requirement to pass function pointer for `gear_sandbox`
// so the only reason this macro exists is const function pointers are not stabilized yet
// so we create non-capturing closure that can be coerced into function pointer
macro_rules! wrap_syscall {
    ($func:ident) => {
        |caller, args| FuncsHandler::execute(caller, args, FuncsHandler::$func)
    };
}

fn store_host_state_mut<Ext>(
    store: &mut Store<HostState<Ext, DefaultExecutorMemory>>,
) -> &mut State<Ext, DefaultExecutorMemory> {
    store
        .data_mut()
        .as_mut()
        .unwrap_or_else(|| unreachable!("State must be set in `WasmiEnvironment::new`; qed"))
}

pub type EnvironmentExecutionResult<Ext, PrepareMemoryError> =
    Result<BackendReport<Ext>, EnvironmentError<PrepareMemoryError>>;

#[derive(Debug, derive_more::Display)]
pub enum EnvironmentError<PrepareMemoryError: Display> {
    #[display(fmt = "Actor backend error: {_1}")]
    Actor(GasAmount, String),
    #[display(fmt = "System backend error: {_0}")]
    System(SystemEnvironmentError),
    #[display(fmt = "Prepare error: {_1}")]
    PrepareMemory(GasAmount, PrepareMemoryError),
}

impl<PrepareMemoryError: Display> EnvironmentError<PrepareMemoryError> {
    pub fn from_infallible(err: EnvironmentError<Infallible>) -> Self {
        match err {
            EnvironmentError::System(err) => Self::System(err),
            EnvironmentError::PrepareMemory(_, err) => match err {},
            EnvironmentError::Actor(gas_amount, s) => Self::Actor(gas_amount, s),
        }
    }
}

#[derive(Debug, derive_more::Display)]
pub enum SystemEnvironmentError {
    #[display(fmt = "Failed to create env memory: {_0:?}")]
    CreateEnvMemory(gear_sandbox::Error),
    #[display(fmt = "Globals are not supported")]
    GlobalsNotSupported,
    #[display(fmt = "Gas counter not found or has wrong type")]
    WrongInjectedGas,
}

/// Environment to run one module at a time providing Ext.
pub struct Environment<Ext, EntryPoint = DispatchKind>
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

pub struct BackendReport<Ext>
where
    Ext: Externalities + 'static,
{
    pub termination_reason: TerminationReason,
    pub memory_wrap: MemoryWrap<Ext>,
    pub ext: Ext,
}

// A helping wrapper for `EnvironmentDefinitionBuilder` and `forbidden_funcs`.
// It makes adding functions to `EnvironmentDefinitionBuilder` shorter.
struct EnvBuilder<Ext: BackendExternalities> {
    env_def_builder: EnvironmentDefinitionBuilder<HostState<Ext, DefaultExecutorMemory>>,
    forbidden_funcs: BTreeSet<SyscallName>,
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
        name: SyscallName,
        f: HostFuncType<HostState<Ext, DefaultExecutorMemory>>,
    ) {
        if self.forbidden_funcs.contains(&name) {
            self.env_def_builder
                .add_host_func("env", name.to_str(), wrap_syscall!(forbidden));
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

impl<Ext, EntryPoint> Environment<Ext, EntryPoint>
where
    Ext: BackendExternalities + 'static,
    Ext::UnrecoverableError: BackendSyscallError,
    RunFallibleError: From<Ext::FallibleError>,
    Ext::AllocError: BackendAllocSyscallError<ExtError = Ext::UnrecoverableError>,
    EntryPoint: WasmEntryPoint,
{
    #[rustfmt::skip]
    fn bind_funcs(builder: &mut EnvBuilder<Ext>) {
        builder.add_func(EnvVars, wrap_syscall!(env_vars));
        builder.add_func(BlockHeight, wrap_syscall!(block_height));
        builder.add_func(BlockTimestamp,wrap_syscall!(block_timestamp));
        builder.add_func(CreateProgram, wrap_syscall!(create_program));
        builder.add_func(CreateProgramWGas, wrap_syscall!(create_program_wgas));
        builder.add_func(Debug, wrap_syscall!(debug));
        builder.add_func(Panic, wrap_syscall!(panic));
        builder.add_func(OomPanic, wrap_syscall!(oom_panic));
        builder.add_func(Exit, wrap_syscall!(exit));
        builder.add_func(ReplyCode, wrap_syscall!(reply_code));
        builder.add_func(SignalCode, wrap_syscall!(signal_code));
        builder.add_func(ReserveGas, wrap_syscall!(reserve_gas));
        builder.add_func(ReplyDeposit, wrap_syscall!(reply_deposit));
        builder.add_func(UnreserveGas, wrap_syscall!(unreserve_gas));
        builder.add_func(GasAvailable, wrap_syscall!(gas_available));
        builder.add_func(Leave, wrap_syscall!(leave));
        builder.add_func(MessageId, wrap_syscall!(message_id));
        builder.add_func(ProgramId, wrap_syscall!(program_id));
        builder.add_func(Random, wrap_syscall!(random));
        builder.add_func(Read, wrap_syscall!(read));
        builder.add_func(Reply, wrap_syscall!(reply));
        builder.add_func(ReplyCommit, wrap_syscall!(reply_commit));
        builder.add_func(ReplyCommitWGas, wrap_syscall!(reply_commit_wgas));
        builder.add_func(ReplyPush, wrap_syscall!(reply_push));
        builder.add_func(ReplyTo, wrap_syscall!(reply_to));
        builder.add_func(SignalFrom, wrap_syscall!(signal_from));
        builder.add_func(ReplyWGas, wrap_syscall!(reply_wgas));
        builder.add_func(ReplyInput, wrap_syscall!(reply_input));
        builder.add_func(ReplyPushInput, wrap_syscall!(reply_push_input));
        builder.add_func(ReplyInputWGas, wrap_syscall!(reply_input_wgas));
        builder.add_func(Send, wrap_syscall!(send));
        builder.add_func(SendCommit, wrap_syscall!(send_commit));
        builder.add_func(SendCommitWGas, wrap_syscall!(send_commit_wgas));
        builder.add_func(SendInit, wrap_syscall!(send_init));
        builder.add_func(SendPush, wrap_syscall!(send_push));
        builder.add_func(SendWGas, wrap_syscall!(send_wgas));
        builder.add_func(SendInput, wrap_syscall!(send_input));
        builder.add_func(SendPushInput, wrap_syscall!(send_push_input));
        builder.add_func(SendInputWGas, wrap_syscall!(send_input_wgas));
        builder.add_func(Size, wrap_syscall!(size));
        builder.add_func(Source, wrap_syscall!(source));
        builder.add_func(Value, wrap_syscall!(value));
        builder.add_func(ValueAvailable, wrap_syscall!(value_available));
        builder.add_func(Wait, wrap_syscall!(wait));
        builder.add_func(WaitFor, wrap_syscall!(wait_for));
        builder.add_func(WaitUpTo, wrap_syscall!(wait_up_to));
        builder.add_func(Wake, wrap_syscall!(wake));
        builder.add_func(SystemReserveGas, wrap_syscall!(system_reserve_gas));
        builder.add_func(ReservationReply, wrap_syscall!(reservation_reply));
        builder.add_func(ReservationReplyCommit, wrap_syscall!(reservation_reply_commit));
        builder.add_func(ReservationSend, wrap_syscall!(reservation_send));
        builder.add_func(ReservationSendCommit, wrap_syscall!(reservation_send_commit));
        builder.add_func(SystemBreak, wrap_syscall!(system_break));

        builder.add_func(Alloc, wrap_syscall!(alloc));
        builder.add_func(Free, wrap_syscall!(free));
        builder.add_func(FreeRange, wrap_syscall!(free_range));
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

impl<EnvExt, EntryPoint> Environment<EnvExt, EntryPoint>
where
    EnvExt: BackendExternalities + 'static,
    EnvExt::UnrecoverableError: BackendSyscallError,
    RunFallibleError: From<EnvExt::FallibleError>,
    EnvExt::AllocError: BackendAllocSyscallError<ExtError = EnvExt::UnrecoverableError>,
    EntryPoint: WasmEntryPoint,
{
    pub fn new(
        ext: EnvExt,
        binary: &[u8],
        entry_point: EntryPoint,
        entries: BTreeSet<DispatchKind>,
        mem_size: WasmPage,
    ) -> Result<Self, EnvironmentError<Infallible>> {
        use EnvironmentError::*;
        use SystemEnvironmentError::*;

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

        // Check that we have implementations for all the syscalls.
        // This is intended to panic during any testing, when the
        // condition is not met.
        assert_eq!(
            builder.funcs_count,
            SyscallName::count(),
            "Not all existing syscalls were added to the module's env."
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

    pub fn execute<PrepareMemory, PrepareMemoryError>(
        self,
        prepare_memory: PrepareMemory,
    ) -> EnvironmentExecutionResult<EnvExt, PrepareMemoryError>
    where
        PrepareMemory: FnOnce(
            &mut MemoryWrap<EnvExt>,
            Option<u32>,
            GlobalsAccessConfig,
        ) -> Result<(), PrepareMemoryError>,
        PrepareMemoryError: Display,
    {
        use EnvironmentError::*;
        use SystemEnvironmentError::*;

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

        #[cfg(feature = "std")]
        let mut globals_provider = GlobalsAccessProvider {
            instance: instance.clone(),
            store: None,
        };
        #[cfg(feature = "std")]
        let globals_provider_dyn_ref = &mut globals_provider as &mut dyn GlobalsAccessor;

        // Pointer to the globals access provider is valid until the end of `invoke` method.
        // So, we can safely use it inside lazy-pages and be sure that it points to the valid object.
        // We cannot guaranty that `store` (and so globals also) will be in a valid state,
        // because executor mut-borrows `store` during execution. But if it's in a valid state
        // each moment when protect memory signal can occur, than this trick is pretty safe.
        #[cfg(feature = "std")]
        let globals_access_ptr = &globals_provider_dyn_ref as *const _ as HostPointer;

        #[cfg(feature = "std")]
        let globals_config = GlobalsAccessConfig {
            access_ptr: globals_access_ptr,
            access_mod: GlobalsAccessMod::NativeRuntime,
        };

        #[cfg(not(feature = "std"))]
        let globals_config = GlobalsAccessConfig {
            access_ptr: instance.get_instance_ptr(),
            access_mod: GlobalsAccessMod::WasmRuntime,
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
            #[cfg(feature = "std")]
            let res = {
                let store_option = &mut globals_provider_dyn_ref
                    .as_any_mut()
                    .downcast_mut::<GlobalsAccessProvider<EnvExt>>()
                    // Provider is `GlobalsAccessProvider`, so panic is impossible.
                    .unwrap_or_else(|| unreachable!("Provider must be `GlobalsAccessProvider`"))
                    .store;

                store_option.replace(store);

                let store_ref = store_option
                    .as_mut()
                    // We set store above, so panic is impossible.
                    .unwrap_or_else(|| unreachable!("Store must be set before"));

                let res = instance.invoke(store_ref, entry_point.as_entry(), &[]);

                store = globals_provider.store.take().unwrap();

                res
            };

            #[cfg(not(feature = "std"))]
            let res = instance.invoke(&mut store, entry_point.as_entry(), &[]);

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
