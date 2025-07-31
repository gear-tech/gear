// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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
    BackendExternalities, MemoryStorer,
    error::{
        ActorTerminationReason, BackendAllocSyscallError, BackendSyscallError, RunFallibleError,
        TerminationReason,
    },
    funcs::FuncsHandler,
    memory::{BackendMemory, ExecutorMemory},
    state::{HostState, State},
};
use alloc::{collections::BTreeSet, format, string::String};
use core::{
    any::Any,
    fmt::Debug,
    marker::{PhantomData, Send},
};
use gear_core::{
    env::{Externalities, WasmEntryPoint},
    gas::GasAmount,
    memory::MemoryError,
    message::DispatchKind,
    pages::WasmPagesAmount,
};
use gear_lazy_pages_common::{GlobalsAccessConfig, GlobalsAccessMod};
use gear_sandbox::{
    AsContextExt, Error, HostFuncType, ReturnValue, SandboxEnvironmentBuilder, SandboxInstance,
    SandboxMemory, SandboxStore, TryFromValue, Value,
    default_executor::{EnvironmentDefinitionBuilder, Instance, Store},
};
use gear_wasm_instrument::{
    GLOBAL_NAME_GAS,
    syscalls::SyscallName::{self, *},
};
#[cfg(feature = "std")]
use {
    gear_core::memory::HostPointer, gear_core::str::LimitedStr,
    gear_lazy_pages_common::GlobalsAccessError, gear_lazy_pages_common::GlobalsAccessor,
};

#[cfg(feature = "std")]
use std::pin::Pin;

// we have requirement to pass function pointer for `gear_sandbox`
// so the only reason this macro exists is const function pointers are not stabilized yet
// so we create non-capturing closure that can be coerced into function pointer
#[rustfmt::skip]
macro_rules! wrap_syscall {
    ($func:ident, $syscall:ident) => {
        |caller, args| FuncsHandler::execute(caller, args, FuncsHandler::$func, $syscall)
    };
}

fn store_host_state_mut<Ext: Send + 'static>(
    store: &mut Store<HostState<Ext, BackendMemory<ExecutorMemory>>>,
) -> &mut State<Ext, BackendMemory<ExecutorMemory>> {
    store.data_mut().as_mut().unwrap_or_else(|| {
        let err_msg =
            "store_host_state_mut: State is not set, but it must be set in `Environment::new`";

        log::error!("{err_msg}");
        unreachable!("{err_msg}")
    })
}
pub type SetupMemoryResult<Ext> = Result<
    (
        Store<HostState<Ext, BackendMemory<ExecutorMemory>>>,
        BackendMemory<ExecutorMemory>,
        Instance<HostState<Ext, BackendMemory<ExecutorMemory>>>,
    ),
    EnvironmentError,
>;

#[derive(Debug, derive_more::Display)]
pub enum EnvironmentError {
    #[display("Actor backend error: {_1}")]
    Actor(GasAmount, String),
    #[display("System backend error: {_0}")]
    System(SystemEnvironmentError),
}

#[derive(Debug, derive_more::Display)]
pub enum SystemEnvironmentError {
    #[display("Failed to create env memory: {_0:?}")]
    CreateEnvMemory(gear_sandbox::Error),
    #[display("Gas counter not found or has wrong type")]
    WrongInjectedGas,
    #[display("Failed to access env memory during dump creation: {_0:?}")]
    DumpMemoryError(MemoryError),
}

pub struct ReadyToExecute;
pub struct FailedExecution;
pub struct SuccessExecution;

pub enum ExecutedEnvironment<'a, Ext: BackendExternalities> {
    SuccessExecution(Environment<'a, Ext, SuccessExecution>),
    FailedExecution(Environment<'a, Ext, FailedExecution>),
}

impl<'a, Ext> ExecutedEnvironment<'a, Ext>
where
    Ext: BackendExternalities + Send + 'static,
{
    fn from_execution_result(
        execution_result: Result<ReturnValue, Error>,
        instance: Instance<HostState<Ext, BackendMemory<ExecutorMemory>>>,
        entries: BTreeSet<DispatchKind>,
        store: Store<HostState<Ext, BackendMemory<ExecutorMemory>>>,
        memory: BackendMemory<ExecutorMemory>,
        code: &'a [u8],
        #[cfg(feature = "std")] globals_holder: Pin<Box<GlobalsHolder<Ext>>>,
    ) -> Self {
        match execution_result {
            Ok(_) => ExecutedEnvironment::SuccessExecution(Environment {
                instance,
                entries,
                store,
                memory,
                code,
                execution_result: Some(execution_result),
                #[cfg(feature = "std")]
                globals_holder,
                _phantom: PhantomData,
            }),
            Err(_) => ExecutedEnvironment::FailedExecution(Environment {
                instance,
                entries,
                store,
                memory,
                code,
                execution_result: Some(execution_result),
                #[cfg(feature = "std")]
                globals_holder,
                _phantom: PhantomData,
            }),
        }
    }

    pub fn report(self) -> BackendReport<Ext>
    where
        Ext: BackendExternalities + Send + 'static,
        Ext::UnrecoverableError: BackendSyscallError,
        RunFallibleError: From<Ext::FallibleError>,
        Ext::AllocError: BackendAllocSyscallError<ExtError = Ext::UnrecoverableError>,
    {
        match self {
            ExecutedEnvironment::FailedExecution(env) => env.report(),
            ExecutedEnvironment::SuccessExecution(env) => env.report(),
        }
    }

    pub fn expect(self, msg: &str) -> Environment<'a, Ext, SuccessExecution> {
        match self {
            ExecutedEnvironment::SuccessExecution(env) => env,
            ExecutedEnvironment::FailedExecution(_) => panic!("{}", msg),
        }
    }
}

/// Environment to run one module at a time providing Ext.
pub struct Environment<'a, Ext, State = ReadyToExecute>
where
    Ext: BackendExternalities,
{
    instance: Instance<HostState<Ext, BackendMemory<ExecutorMemory>>>,
    entries: BTreeSet<DispatchKind>,
    store: Store<HostState<Ext, BackendMemory<ExecutorMemory>>>,
    memory: BackendMemory<ExecutorMemory>,
    code: &'a [u8],
    execution_result: Option<Result<ReturnValue, Error>>,
    #[cfg(feature = "std")]
    globals_holder: Pin<Box<GlobalsHolder<Ext>>>,
    _phantom: PhantomData<State>,
}

#[cfg(feature = "std")]
struct GlobalsHolder<Ext>
where
    Ext: BackendExternalities,
{
    access_provider: Box<GlobalsAccessProvider<Ext>>,
    accessor_ref: *mut dyn GlobalsAccessor,
    access_ptr: HostPointer,
}

#[cfg(feature = "std")]
impl<Ext> GlobalsHolder<Ext>
where
    Ext: BackendExternalities + Send + 'static,
{
    fn new(instance: Instance<HostState<Ext, BackendMemory<ExecutorMemory>>>) -> Pin<Box<Self>> {
        let mut access_provider = Box::new(GlobalsAccessProvider {
            instance,
            store: None,
        });

        let accessor_ref: *mut dyn GlobalsAccessor = &mut *access_provider;

        let holder = GlobalsHolder {
            access_provider,
            accessor_ref,
            access_ptr: 0,
        };

        let mut boxed_holder = Box::pin(holder);

        let access_ptr = {
            let holder_ref: &*mut dyn GlobalsAccessor = &boxed_holder.accessor_ref;
            holder_ref as *const _ as HostPointer
        };

        unsafe {
            let mut_ref = Pin::get_unchecked_mut(boxed_holder.as_mut());
            mut_ref.access_ptr = access_ptr;
        }

        boxed_holder
    }

    fn access_provider_mut(&mut self) -> &mut GlobalsAccessProvider<Ext> {
        self.access_provider.as_mut()
    }

    fn accessor_ref(&mut self) -> &mut dyn GlobalsAccessor {
        self.access_provider.as_mut()
    }

    fn access_ptr(&self) -> HostPointer {
        self.access_ptr
    }
}

#[cfg(feature = "std")]
struct GlobalsAccessProvider<Ext: Externalities> {
    instance: Instance<HostState<Ext, BackendMemory<ExecutorMemory>>>,
    store: Option<Store<HostState<Ext, BackendMemory<ExecutorMemory>>>>,
}

#[cfg(feature = "std")]
impl<Ext: Externalities + Send + 'static> GlobalsAccessor for GlobalsAccessProvider<Ext> {
    fn get_i64(&mut self, name: &LimitedStr) -> Result<i64, GlobalsAccessError> {
        let store = self.store.as_mut().ok_or(GlobalsAccessError)?;
        self.instance
            .get_global_val(store, name.as_str())
            .and_then(i64::try_from_value)
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

pub struct BackendReport<Ext>
where
    Ext: Externalities + 'static,
{
    pub termination_reason: TerminationReason,
    pub store: Store<HostState<Ext, BackendMemory<ExecutorMemory>>>,
    pub memory: BackendMemory<ExecutorMemory>,
    pub ext: Ext,
}

// A helping wrapper for `EnvironmentDefinitionBuilder` and `forbidden_funcs`.
// It makes adding functions to `EnvironmentDefinitionBuilder` shorter.
struct EnvBuilder<Ext: BackendExternalities> {
    env_def_builder: EnvironmentDefinitionBuilder<HostState<Ext, BackendMemory<ExecutorMemory>>>,
    funcs_count: usize,
}

impl<Ext> EnvBuilder<Ext>
where
    Ext: BackendExternalities + Send + 'static,
    Ext::UnrecoverableError: BackendSyscallError,
    RunFallibleError: From<Ext::FallibleError>,
    Ext::AllocError: BackendAllocSyscallError<ExtError = Ext::UnrecoverableError>,
{
    fn add_func(
        &mut self,
        name: SyscallName,
        f: HostFuncType<HostState<Ext, BackendMemory<ExecutorMemory>>>,
    ) {
        self.env_def_builder.add_host_func("env", name.to_str(), f);

        self.funcs_count += 1;
    }

    fn add_memory(&mut self, memory: BackendMemory<ExecutorMemory>) {
        self.env_def_builder
            .add_memory("env", "memory", memory.into_inner());
    }
}

impl<Ext: BackendExternalities> From<EnvBuilder<Ext>>
    for EnvironmentDefinitionBuilder<HostState<Ext, BackendMemory<ExecutorMemory>>>
{
    fn from(builder: EnvBuilder<Ext>) -> Self {
        builder.env_def_builder
    }
}

impl<Ext, T> Environment<'_, Ext, T>
where
    Ext: BackendExternalities + Send + 'static,
    Ext::UnrecoverableError: BackendSyscallError,
    RunFallibleError: From<Ext::FallibleError>,
    Ext::AllocError: BackendAllocSyscallError<ExtError = Ext::UnrecoverableError>,
{
    #[rustfmt::skip]
    fn bind_funcs(builder: &mut EnvBuilder<Ext>) {
        macro_rules! add_function {
            ($syscall:ident, $func:ident) => {
                builder.add_func($syscall, wrap_syscall!($func, $syscall));
            };
        }

        add_function!(EnvVars, env_vars);
        add_function!(BlockHeight, block_height);
        add_function!(BlockTimestamp, block_timestamp);
        add_function!(CreateProgram, create_program);
        add_function!(CreateProgramWGas, create_program_wgas);
        add_function!(Debug, debug);
        add_function!(Panic, panic);
        add_function!(OomPanic, oom_panic);
        add_function!(Exit, exit);
        add_function!(ReplyCode, reply_code);
        add_function!(SignalCode, signal_code);
        add_function!(ReserveGas, reserve_gas);
        add_function!(ReplyDeposit, reply_deposit);
        add_function!(UnreserveGas, unreserve_gas);
        add_function!(GasAvailable, gas_available);
        add_function!(Leave, leave);
        add_function!(MessageId, message_id);
        add_function!(ProgramId, program_id);
        add_function!(Random, random);
        add_function!(Read, read);
        add_function!(Reply, reply);
        add_function!(ReplyCommit, reply_commit);
        add_function!(ReplyCommitWGas, reply_commit_wgas);
        add_function!(ReplyPush, reply_push);
        add_function!(ReplyTo, reply_to);
        add_function!(SignalFrom, signal_from);
        add_function!(ReplyWGas, reply_wgas);
        add_function!(ReplyInput, reply_input);
        add_function!(ReplyPushInput, reply_push_input);
        add_function!(ReplyInputWGas, reply_input_wgas);
        add_function!(Send, send);
        add_function!(SendCommit, send_commit);
        add_function!(SendCommitWGas, send_commit_wgas);
        add_function!(SendInit, send_init);
        add_function!(SendPush, send_push);
        add_function!(SendWGas, send_wgas);
        add_function!(SendInput, send_input);
        add_function!(SendPushInput, send_push_input);
        add_function!(SendInputWGas, send_input_wgas);
        add_function!(Size, size);
        add_function!(Source, source);
        add_function!(Value, value);
        add_function!(ValueAvailable, value_available);
        add_function!(Wait, wait);
        add_function!(WaitFor, wait_for);
        add_function!(WaitUpTo, wait_up_to);
        add_function!(Wake, wake);
        add_function!(SystemReserveGas, system_reserve_gas);
        add_function!(ReservationReply, reservation_reply);
        add_function!(ReservationReplyCommit, reservation_reply_commit);
        add_function!(ReservationSend, reservation_send);
        add_function!(ReservationSendCommit, reservation_send_commit);
        add_function!(SystemBreak, system_break);

        add_function!(Alloc, alloc);
        add_function!(Free, free);
        add_function!(FreeRange, free_range);
    }

    fn report_impl(mut self) -> BackendReport<Ext> {
        let state = self.store.data_mut().take().unwrap_or_else(|| {
            let err_msg = "Environment::report: State must be set";

            log::error!("{err_msg}");
            unreachable!("{err_msg}")
        });

        let gas = self
            .instance
            .get_global_val(&mut self.store, GLOBAL_NAME_GAS)
            .and_then(i64::try_from_value)
            .ok_or(SystemEnvironmentError::WrongInjectedGas)
            .unwrap() as u64;

        let execution_result = self.execution_result.take().unwrap_or_else(|| {
            let err_msg = "Environment::report: Execution result must be set";

            log::error!("{err_msg}");
            unreachable!("{err_msg}")
        });

        let (ext, termination_reason) = state.terminate(execution_result, gas);

        BackendReport {
            termination_reason,
            store: self.store,
            memory: self.memory,
            ext,
        }
    }
}

impl<'a, EnvExt> Environment<'a, EnvExt>
where
    EnvExt: BackendExternalities + Send + 'static,
    EnvExt::UnrecoverableError: BackendSyscallError,
    RunFallibleError: From<EnvExt::FallibleError>,
    EnvExt::AllocError: BackendAllocSyscallError<ExtError = EnvExt::UnrecoverableError>,
{
    pub fn new(
        ext: EnvExt,
        binary: &'a [u8],
        entries: BTreeSet<DispatchKind>,
        mem_size: WasmPagesAmount,
        prepare_memory: impl FnOnce(
            &mut Store<HostState<EnvExt, BackendMemory<ExecutorMemory>>>,
            &mut BackendMemory<ExecutorMemory>,
            GlobalsAccessConfig,
        ),
    ) -> Result<Environment<'a, EnvExt, ReadyToExecute>, EnvironmentError> {
        use EnvironmentError::*;
        use SystemEnvironmentError::*;

        let mut store = Store::new(None);

        let mut builder = EnvBuilder::<EnvExt> {
            env_def_builder: EnvironmentDefinitionBuilder::new(),
            funcs_count: 0,
        };

        let mut memory: BackendMemory<ExecutorMemory> =
            match ExecutorMemory::new(&mut store, mem_size.into(), None) {
                Ok(mem) => mem.into(),
                Err(e) => return Err(System(CreateEnvMemory(e))),
            };

        builder.add_memory(memory.clone());

        Self::bind_funcs(&mut builder);

        // Check that we have implementations for all the syscalls.
        // This is intended to panic during any testing, when the
        // condition is not met.
        assert_eq!(
            builder.funcs_count,
            SyscallName::all().count(),
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

        #[cfg(feature = "std")]
        let globals_holder = GlobalsHolder::new(instance.clone());

        #[cfg(feature = "std")]
        let globals_config = GlobalsAccessConfig {
            access_ptr: globals_holder.access_ptr(),
            access_mod: GlobalsAccessMod::NativeRuntime,
        };

        #[cfg(not(feature = "std"))]
        let globals_config = GlobalsAccessConfig {
            access_ptr: instance.get_instance_ptr(),
            access_mod: GlobalsAccessMod::WasmRuntime,
        };

        prepare_memory(&mut store, &mut memory, globals_config);

        Ok(Environment {
            instance,
            entries,
            store,
            memory,
            code: binary,
            execution_result: None,
            #[cfg(feature = "std")]
            globals_holder,
            _phantom: PhantomData,
        })
    }
}

impl<'a, EnvExt> Environment<'a, EnvExt, ReadyToExecute>
where
    EnvExt: BackendExternalities + Send + 'static,
    EnvExt::UnrecoverableError: BackendSyscallError,
    RunFallibleError: From<EnvExt::FallibleError>,
    EnvExt::AllocError: BackendAllocSyscallError<ExtError = EnvExt::UnrecoverableError>,
{
    pub fn execute<M: MemoryStorer>(
        self,
        entry_point: impl WasmEntryPoint,
        memory_storer: Option<&mut M>,
    ) -> Result<ExecutedEnvironment<'a, EnvExt>, EnvironmentError> {
        use EnvironmentError::*;
        use SystemEnvironmentError::*;

        let Self {
            mut instance,
            entries,
            mut store,
            memory,
            code,
            #[cfg(feature = "std")]
            mut globals_holder,
            ..
        } = self;

        let gas = store_host_state_mut(&mut store)
            .ext
            .define_current_counter();

        instance
            .set_global_val(&mut store, GLOBAL_NAME_GAS, Value::I64(gas as i64))
            .map_err(|_| System(WrongInjectedGas))?;

        let needs_execution = entry_point
            .try_into_kind()
            .map(|kind| entries.contains(&kind))
            .unwrap_or(true);

        let res = if needs_execution {
            #[cfg(feature = "std")]
            let globals_provider_dyn_ref = globals_holder.accessor_ref();

            #[cfg(feature = "std")]
            let res = {
                let store_option = &mut globals_provider_dyn_ref
                    .as_any_mut()
                    .downcast_mut::<GlobalsAccessProvider<EnvExt>>()
                    // Provider is `GlobalsAccessProvider`, so panic is impossible.
                    .unwrap_or_else(|| {
                        let err_msg =
                            "Environment::execute: Provider must be `GlobalsAccessProvider`";

                        log::error!("{err_msg}");
                        unreachable!("{err_msg}")
                    })
                    .store;

                store_option.replace(store);

                let store_ref = store_option
                    .as_mut()
                    // We set store above, so panic is impossible.
                    .unwrap_or_else(|| {
                        let err_msg = "Environment::execute: Store must be set before";

                        log::error!("{err_msg}");
                        unreachable!("{err_msg}")
                    });

                let res = instance.invoke(store_ref, entry_point.as_entry(), &[]);

                store = globals_holder.access_provider_mut().store.take().unwrap();

                res
            };

            #[cfg(not(feature = "std"))]
            let res = instance.invoke(&mut store, entry_point.as_entry(), &[]);

            res
        } else {
            Ok(ReturnValue::Unit)
        };

        if let Some(memory_storer) = memory_storer {
            // If we have a memory storer, we dump the memory after execution.
            memory_storer
                .dump_memory(&store, &memory)
                .map_err(|e| System(DumpMemoryError(e)))?;
        }

        Ok(ExecutedEnvironment::from_execution_result(
            res,
            instance,
            entries,
            store,
            memory,
            code,
            #[cfg(feature = "std")]
            globals_holder,
        ))
    }
}

impl<'a, EnvExt> Environment<'a, EnvExt, SuccessExecution>
where
    EnvExt: BackendExternalities + Send + 'static,
    EnvExt::UnrecoverableError: BackendSyscallError,
    RunFallibleError: From<EnvExt::FallibleError>,
    EnvExt::AllocError: BackendAllocSyscallError<ExtError = EnvExt::UnrecoverableError>,
{
    pub fn report(self) -> BackendReport<EnvExt> {
        self.report_impl()
    }

    pub fn set_ext(
        self,
        ext: EnvExt,
    ) -> Result<Environment<'a, EnvExt, ReadyToExecute>, EnvironmentError> {
        let Self {
            instance,
            entries,
            mut store,
            memory,
            code,
            #[cfg(feature = "std")]
            globals_holder,
            ..
        } = self;

        *store.data_mut() = Some(State {
            ext,
            memory: memory.clone(),
            termination_reason: ActorTerminationReason::Success.into(),
        });

        Ok(Environment {
            instance,
            entries,
            store,
            memory,
            code,
            execution_result: None,
            #[cfg(feature = "std")]
            globals_holder,
            _phantom: PhantomData,
        })
    }
}

impl<'a, EnvExt> Environment<'a, EnvExt, FailedExecution>
where
    EnvExt: BackendExternalities + Send + 'static,
    EnvExt::UnrecoverableError: BackendSyscallError,
    RunFallibleError: From<EnvExt::FallibleError>,
    EnvExt::AllocError: BackendAllocSyscallError<ExtError = EnvExt::UnrecoverableError>,
{
    pub fn report(self) -> BackendReport<EnvExt> {
        self.report_impl()
    }

    pub fn revert(
        self,
        memory_storer: &impl MemoryStorer,
    ) -> Result<Environment<'a, EnvExt, SuccessExecution>, MemoryError> {
        let Self {
            instance,
            entries,
            mut store,
            memory,
            code,
            #[cfg(feature = "std")]
            globals_holder,
            ..
        } = self;

        memory_storer.revert_memory(&mut store, &memory)?;

        Ok(Environment {
            instance,
            entries,
            store,
            memory,
            code,
            execution_result: None,
            #[cfg(feature = "std")]
            globals_holder,
            _phantom: PhantomData,
        })
    }
}
