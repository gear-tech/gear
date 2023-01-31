// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-lat&er WITH Classpath-exception-2.0

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

//! wasmi environment for running a module.

use crate::{
    funcs_tree,
    memory::MemoryWrap,
    state::{HostState, State},
};
use alloc::{
    collections::BTreeSet,
    string::{String, ToString},
};
use core::{any::Any, convert::Infallible, fmt::Display};
use gear_backend_common::{
    lazy_pages::{GlobalsAccessError, GlobalsAccessMod, GlobalsAccessor, GlobalsConfig},
    ActorSyscallFuncError, BackendExt, BackendExtError, BackendReport, Environment,
    EnvironmentExecutionError, EnvironmentExecutionResult, GetGasAmount, TerminationReason,
    STACK_END_EXPORT_NAME,
};
use gear_core::{
    env::Ext,
    gas::GasAmount,
    memory::{HostPointer, PageU32Size, WasmPage},
    message::{DispatchKind, WasmEntry},
};
use gear_wasm_instrument::{GLOBAL_NAME_ALLOWANCE, GLOBAL_NAME_FLAGS, GLOBAL_NAME_GAS};
use wasmi::{
    core::Value, Engine, Extern, Global, Instance, Linker, Memory, MemoryType, Module, Store,
};

#[derive(Debug, derive_more::Display, derive_more::From)]
pub enum WasmiEnvironmentError {
    #[from]
    #[display(fmt = "Failed to create env memory: {_0:?}")]
    CreateEnvMemory(wasmi::errors::MemoryError),
    #[from]
    #[display(fmt = "Unable to link item: {_0:?}")]
    Linking(wasmi::errors::LinkerError),
    #[from]
    #[display(fmt = "Unable to instantiate module: {_0:?}")]
    ModuleInstantiation(wasmi::Error),
    #[display(fmt = "Unable to get wasm module exports: {_0}")]
    GetWasmExports(String),
    #[display(fmt = "Entry point has wrong type: {_0}")]
    EntryPointWrongType(String),
    #[display(fmt = "Gas counter not found or has wrong type")]
    WrongInjectedGas,
    #[display(fmt = "Allowance counter not found or has wrong type")]
    WrongInjectedAllowance,
}

#[derive(Debug, derive_more::Display, derive_more::From)]
#[display(fmt = "{error}")]
pub struct Error {
    gas_amount: GasAmount,
    error: WasmiEnvironmentError,
}

impl GetGasAmount for Error {
    fn gas_amount(&self) -> GasAmount {
        self.gas_amount.clone()
    }
}

macro_rules! gas_amount {
    ($store:ident) => {
        $store
            .state()
            .as_ref()
            .unwrap_or_else(|| unreachable!("State must be set in `WasmiEnvironment::new`; qed"))
            .ext
            .gas_amount()
    };
}

/// Environment to run one module at a time providing Ext.
pub struct WasmiEnvironment<E, EP = DispatchKind>
where
    E: Ext,
    EP: WasmEntry,
{
    instance: Instance,
    store: Store<HostState<E>>,
    memory: Memory,
    entries: BTreeSet<DispatchKind>,
    entry_point: EP,
}

struct GlobalsAccessProvider<E: Ext> {
    pub instance: Instance,
    pub store: Option<Store<HostState<E>>>,
}

impl<E: Ext> GlobalsAccessProvider<E> {
    fn get_global(&self, name: &str) -> Option<Global> {
        let store = self.store.as_ref()?;
        self.instance
            .get_export(store, name)
            .and_then(|export| export.into_global())
    }
}

impl<E: Ext + 'static> GlobalsAccessor for GlobalsAccessProvider<E> {
    fn get_i64(&self, name: &str) -> Result<i64, GlobalsAccessError> {
        self.get_global(name)
            .and_then(|global| {
                let store = self.store.as_ref()?;
                if let Value::I64(val) = global.get(store) {
                    Some(val)
                } else {
                    None
                }
            })
            .ok_or(GlobalsAccessError)
    }

    fn set_i64(&mut self, name: &str, value: i64) -> Result<(), GlobalsAccessError> {
        self.get_global(name)
            .and_then(|global| {
                let store = self.store.as_mut()?;
                global.set(store, Value::I64(value)).ok()
            })
            .ok_or(GlobalsAccessError)
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl<E, EP> Environment<EP> for WasmiEnvironment<E, EP>
where
    E: BackendExt + GetGasAmount + 'static,
    E::Error: BackendExtError,
    EP: WasmEntry,
{
    type Ext = E;
    type Memory = MemoryWrap<E>;
    type Error = Error;

    fn new(
        ext: Self::Ext,
        binary: &[u8],
        entry_point: EP,
        entries: BTreeSet<DispatchKind>,
        mem_size: WasmPage,
    ) -> Result<Self, EnvironmentExecutionError<Self::Error, Infallible>> {
        use EnvironmentExecutionError::*;
        use WasmiEnvironmentError::*;

        let engine = Engine::default();
        let mut store: Store<HostState<E>> = Store::new(&engine, None);

        let mut linker: Linker<HostState<E>> = Linker::new();

        let memory_type = MemoryType::new(mem_size.raw(), None);
        let memory = Memory::new(&mut store, memory_type)
            .map_err(|e| Environment((ext.gas_amount(), CreateEnvMemory(e)).into()))?;

        linker
            .define("env", "memory", memory)
            .map_err(|e| Environment((ext.gas_amount(), Linking(e)).into()))?;

        let entry_forbidden = entry_point
            .try_into_kind()
            .as_ref()
            .map(DispatchKind::forbidden_funcs)
            .unwrap_or_default();

        let forbidden_funcs = ext
            .forbidden_funcs()
            .iter()
            .copied()
            .chain(entry_forbidden)
            .collect();

        let functions = funcs_tree::build(&mut store, memory, forbidden_funcs);
        for (name, function) in functions {
            linker
                .define("env", name.to_str(), function)
                .map_err(|e| Environment((ext.gas_amount(), Linking(e)).into()))?;
        }

        let module = Module::new(store.engine(), &mut &binary[..])
            .map_err(|e| Environment((ext.gas_amount(), ModuleInstantiation(e)).into()))?;

        let runtime = State {
            ext,
            err: ActorSyscallFuncError::Terminated(TerminationReason::Success).into(),
        };

        *store.state_mut() = Some(runtime);

        let instance_pre = linker
            .instantiate(&mut store, &module)
            .map_err(|e| Environment((gas_amount!(store), ModuleInstantiation(e)).into()))?;

        let instance = instance_pre
            .ensure_no_start(&mut store)
            .map_err(|e| Environment((gas_amount!(store), ModuleInstantiation(e.into())).into()))?;

        Ok(Self {
            instance,
            store,
            memory,
            entries,
            entry_point,
        })
    }

    fn execute<F, T>(self, pre_execution_handler: F) -> EnvironmentExecutionResult<T, Self, EP>
    where
        F: FnOnce(&mut Self::Memory, Option<u32>, GlobalsConfig) -> Result<(), T>,
        T: Display,
    {
        use EnvironmentExecutionError::*;
        use WasmiEnvironmentError::*;

        let Self {
            instance,
            mut store,
            memory,
            entries,
            entry_point,
        } = self;

        let stack_end = instance
            .get_export(&store, STACK_END_EXPORT_NAME)
            .and_then(Extern::into_global)
            .and_then(|g| g.get(&store).try_into::<u32>());

        let (gas, allowance) = store
            .state()
            .as_ref()
            .unwrap_or_else(|| unreachable!("State must be set in `WasmiEnvironment::new`"))
            .ext
            .counters();

        let gear_gas = instance
            .get_export(&store, GLOBAL_NAME_GAS)
            .and_then(Extern::into_global)
            .and_then(|g| g.set(&mut store, Value::I64(gas as i64)).map(|_| g).ok())
            .ok_or(Environment((gas_amount!(store), WrongInjectedGas).into()))?;

        let gear_allowance = instance
            .get_export(&store, GLOBAL_NAME_ALLOWANCE)
            .and_then(Extern::into_global)
            .and_then(|g| {
                g.set(&mut store, Value::I64(allowance as i64))
                    .map(|_| g)
                    .ok()
            })
            .ok_or(Environment(
                (gas_amount!(store), WrongInjectedAllowance).into(),
            ))?;

        let mut globals_provider = GlobalsAccessProvider {
            instance,
            store: None,
        };
        let globals_provider_dyn_ref = &mut globals_provider as &mut dyn GlobalsAccessor;

        let needs_execution = entry_point
            .try_into_kind()
            .map(|kind| entries.contains(&kind))
            .unwrap_or(true);

        // Pointer to the globals access provider is valid until the end of `execute` method.
        // So, we can safely use it inside lazy-pages and be sure that it points to the valid object.
        // We cannot guaranty that `store` (and so globals also) will be in a valid state,
        // because executor mut-borrows `store` during execution. But if it's in a valid state
        // each moment when protect memory signal can occur, than this trick is pretty safe.
        let globals_access_ptr = &globals_provider_dyn_ref as *const _ as HostPointer;

        let globals_config = GlobalsConfig {
            global_gas_name: GLOBAL_NAME_GAS.to_string(),
            global_allowance_name: GLOBAL_NAME_ALLOWANCE.to_string(),
            global_flags_name: GLOBAL_NAME_FLAGS.to_string(),
            globals_access_ptr,
            globals_access_mod: GlobalsAccessMod::NativeRuntime,
        };

        let mut memory_wrap = MemoryWrap::new(memory, store);
        pre_execution_handler(&mut memory_wrap, stack_end, globals_config).map_err(|e| {
            let store = &memory_wrap.store;
            PrepareMemory(gas_amount!(store), e)
        })?;

        let mut store = memory_wrap.into_store();
        let res = if needs_execution {
            let func = instance
                .get_export(&store, entry_point.as_entry())
                .and_then(Extern::into_func)
                .ok_or({
                    Environment(
                        (
                            gas_amount!(store),
                            GetWasmExports(entry_point.as_entry().to_string()),
                        )
                            .into(),
                    )
                })?;

            let entry_func = func.typed::<(), (), _>(&mut store).map_err(|_| {
                Environment(
                    (
                        gas_amount!(store),
                        EntryPointWrongType(entry_point.as_entry().to_string()),
                    )
                        .into(),
                )
            })?;

            let store_option = &mut globals_provider_dyn_ref
                .as_any_mut()
                .downcast_mut::<GlobalsAccessProvider<E>>()
                // Provider is `GlobalsAccessProvider`, so panic is impossible.
                .unwrap_or_else(|| unreachable!("Provider must be `GlobalsAccessProvider`"))
                .store;

            store_option.replace(store);
            let res = entry_func.call(
                store_option
                    .as_mut()
                    // We set store above, so panic is impossible.
                    .unwrap_or_else(|| unreachable!("Store must be set before")),
                (),
            );
            store = globals_provider.store.take().unwrap();

            res
        } else {
            Ok(())
        };

        let gas = gear_gas
            .get(&store)
            .try_into::<i64>()
            .ok_or(Environment((gas_amount!(store), WrongInjectedGas).into()))?;
        let allowance = gear_allowance
            .get(&store)
            .try_into::<i64>()
            .ok_or(Environment(
                (gas_amount!(store), WrongInjectedAllowance).into(),
            ))?;

        let state = store
            .state_mut()
            .take()
            .unwrap_or_else(|| unreachable!("State must be set in `WasmiEnvironment::new`; qed"));

        let State {
            mut ext,
            err: runtime_err,
            ..
        } = state;

        ext.update_counters(gas as u64, allowance as u64);

        log::debug!("WasmiEnvironment::execute result = {res:?}");

        Ok(BackendReport {
            err: (res.is_err()).then_some(runtime_err),
            memory_wrap: MemoryWrap::new(memory, store),
            ext,
        })
    }
}
