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
    funcs::FuncError,
    funcs_tree,
    memory::MemoryWrap,
    state::{HostState, State},
};
use alloc::{
    collections::BTreeSet,
    string::{String, ToString},
};
use codec::Encode;
use core::{any::Any, fmt};
use gear_backend_common::{
    calc_stack_end, error_processor::IntoExtError, AsTerminationReason, BackendReport, Environment,
    GetGasAmount, IntoExtInfo, StackEndError, TerminationReason, TrapExplanation,
    STACK_END_EXPORT_NAME,
};
use gear_core::{
    costs::RuntimeCosts,
    env::Ext,
    gas::GasAmount,
    lazy_pages::{
        GlobalsAccessError, GlobalsAccessMod, GlobalsAccessTrait, GlobalsCtx, LazyPagesWeights,
    },
    memory::{HostPointer, WasmPageNumber, PageU32Size},
    message::DispatchKind,
};
use gear_wasm_instrument::{GLOBAL_NAME_ALLOWANCE, GLOBAL_NAME_GAS};
use wasmi::{
    core::Value, Engine, Extern, Global, Instance, Linker, Memory, MemoryType, Module, Store,
};

#[derive(Debug, derive_more::Display, derive_more::From)]
pub enum WasmiEnvironmentError {
    #[display(fmt = "Failed to create env memory: {_0:?}")]
    CreateEnvMemory(wasmi::errors::MemoryError),
    #[display(fmt = "Unable to link item: {_0:?}")]
    Linking(wasmi::errors::LinkerError),
    #[display(fmt = "Unable to instantiate module: {_0:?}")]
    ModuleInstantiation(wasmi::Error),
    #[display(fmt = "Unable to get wasm module exports: {_0}")]
    GetWasmExports(String),
    #[display(fmt = "Entry point has wrong type: {_0}")]
    EntryPointWrongType(String),
    #[display(fmt = "{_0}")]
    PreExecutionHandler(String),
    #[from]
    StackEnd(StackEndError),
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
            .expect("set before the block; qed")
            .ext
            .gas_amount()
    };
}

/// Environment to run one module at a time providing Ext.
pub struct WasmiEnvironment<E: Ext> {
    instance: Instance,
    store: Store<HostState<E>>,
    memory: Memory,
    entries: BTreeSet<DispatchKind>,
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

impl<E: Ext + 'static> GlobalsAccessTrait for GlobalsAccessProvider<E> {
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

    fn get_i32(&self, _name: &str) -> Result<i32, GlobalsAccessError> {
        todo!("Currently useless")
    }

    fn set_i32(&mut self, _name: &str, _value: i32) -> Result<(), GlobalsAccessError> {
        todo!("Currently useless")
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl<E> Environment<E> for WasmiEnvironment<E>
where
    E: Ext + IntoExtInfo<E::Error> + GetGasAmount + 'static,
    E::Error: Encode + AsTerminationReason + IntoExtError,
{
    type Memory = MemoryWrap<E>;
    type Error = Error;

    fn new(
        ext: E,
        binary: &[u8],
        entries: BTreeSet<DispatchKind>,
        mem_size: WasmPageNumber,
    ) -> Result<Self, Self::Error> {
        use WasmiEnvironmentError::*;

        let engine = Engine::default();
        let mut store: Store<HostState<E>> = Store::new(&engine, None);

        let mut linker: Linker<HostState<E>> = Linker::new();

        let memory_type = MemoryType::new(mem_size.raw(), None);
        let memory = Memory::new(&mut store, memory_type)
            .map_err(|e| (ext.gas_amount(), CreateEnvMemory(e)))?;

        linker
            .define("env", "memory", memory)
            .map_err(|e| (ext.gas_amount(), Linking(e)))?;

        let forbidden_funcs =
            (!ext.forbidden_funcs().is_empty()).then(|| ext.forbidden_funcs().clone());
        let functions = funcs_tree::build(&mut store, memory, forbidden_funcs);
        for (name, function) in functions {
            linker
                .define("env", name, function)
                .map_err(|e| (ext.gas_amount(), Linking(e)))?;
        }

        let module = Module::new(store.engine(), &mut &binary[..])
            .map_err(|e| (ext.gas_amount(), ModuleInstantiation(e)))?;

        let runtime = State {
            ext,
            err: FuncError::Terminated(TerminationReason::Success),
        };

        *store.state_mut() = Some(runtime);

        let instance_pre = linker
            .instantiate(&mut store, &module)
            .map_err(|e| (gas_amount!(store), ModuleInstantiation(e)))?;

        let instance = instance_pre
            .ensure_no_start(&mut store)
            .map_err(|e| (gas_amount!(store), ModuleInstantiation(e.into())))?;

        Ok(Self {
            instance,
            store,
            memory,
            entries,
        })
    }

    fn execute<F, T>(
        self,
        entry_point: &DispatchKind,
        pre_execution_handler: F,
    ) -> Result<BackendReport<Self::Memory, E>, Self::Error>
    where
        F: FnOnce(&mut Self::Memory, Option<WasmPageNumber>, Option<GlobalsCtx>) -> Result<(), T>,
        T: fmt::Display,
    {
        use WasmiEnvironmentError::*;

        let Self {
            instance,
            mut store,
            memory,
            entries,
        } = self;

        let stack_end = instance
            .get_export(&store, STACK_END_EXPORT_NAME)
            .and_then(Extern::into_global)
            .and_then(|g| g.get(&store).try_into::<i32>());
        let stack_end_page =
            calc_stack_end(stack_end).map_err(|e| (gas_amount!(store), StackEnd(e)))?;

        let (gas, allowance) = store
            .state()
            .as_ref()
            .expect("should be set")
            .ext
            .counters();

        let gear_gas = instance
            .get_export(&store, GLOBAL_NAME_GAS)
            .and_then(Extern::into_global)
            .and_then(|g| g.set(&mut store, Value::I64(gas as i64)).map(|_| g).ok())
            .ok_or((gas_amount!(store), WrongInjectedGas))?;

        let gear_allowance = instance
            .get_export(&store, GLOBAL_NAME_ALLOWANCE)
            .and_then(Extern::into_global)
            .and_then(|g| {
                g.set(&mut store, Value::I64(allowance as i64))
                    .map(|_| g)
                    .ok()
            })
            .ok_or((gas_amount!(store), WrongInjectedAllowance))?;

        let mut globals_provider = GlobalsAccessProvider {
            instance,
            store: None,
        };
        let state = store.state_mut().as_ref().expect("is set in `new`; qed");
        let globals_provider_dyn_ref = &mut globals_provider as &mut dyn GlobalsAccessTrait;

        // Pointer to the globals access provider is valid until the end of `execute` method.
        // So, we can safely use it inside lazy-pages and be sure that it points to the valid object.
        // We cannot guaranty that `store` (and so globals also) will be in a valid state,
        // because executor mut-borrows `store` during execution. But if it's in a valid state
        // each moment when protect memory signal can occur, than this trick is pretty safe.
        let globals_access_ptr = &globals_provider_dyn_ref as *const _ as HostPointer;

        let globals_ctx = GlobalsCtx {
            global_gas_name: GLOBAL_NAME_GAS.to_string(),
            global_allowance_name: GLOBAL_NAME_ALLOWANCE.to_string(),
            global_state_name: "gear_status".to_string(),
            lazy_pages_weights: LazyPagesWeights {
                read: state.ext.get_runtime_cost(RuntimeCosts::LazyPagesRead),
                write: state.ext.get_runtime_cost(RuntimeCosts::LazyPagesWrite),
                write_after_read: state
                    .ext
                    .get_runtime_cost(RuntimeCosts::LazyPagesWriteAfterRead),
            },
            globals_access_ptr,
            globals_access_mod: GlobalsAccessMod::NativeRuntime,
        };

        let mut memory_wrap = MemoryWrap::new(memory, store);
        pre_execution_handler(&mut memory_wrap, stack_end_page, Some(globals_ctx)).map_err(
            |e| {
                let store = &memory_wrap.store;
                (gas_amount!(store), PreExecutionHandler(e.to_string()))
            },
        )?;

        let mut store = memory_wrap.into_store();
        let res = if entries.contains(entry_point) {
            let func = instance
                .get_export(&store, entry_point.into_entry())
                .and_then(Extern::into_func)
                .ok_or({
                    let store = &store;
                    (
                        gas_amount!(store),
                        GetWasmExports(entry_point.into_entry().to_string()),
                    )
                })?;

            let entry_func = func.typed::<(), (), _>(&mut store).map_err(|_| {
                let store = &store;
                (
                    gas_amount!(store),
                    EntryPointWrongType(entry_point.into_entry().to_string()),
                )
            })?;

            globals_provider_dyn_ref
                .as_any_mut()
                .downcast_mut::<GlobalsAccessProvider<E>>()
                .expect("Just make it, so this panic cannot occur")
                .store
                .replace(store);
            let res = entry_func.call(
                globals_provider_dyn_ref
                    .as_any_mut()
                    .downcast_mut::<GlobalsAccessProvider<E>>()
                    .expect("Just make it, so this panic cannot occur")
                    .store
                    .as_mut()
                    .expect("We just set store as Some(...), so this panic cannot occur"),
                (),
            );
            store = globals_provider.store.take().unwrap();

            res
        } else {
            Ok(())
        };

        let gas = gear_gas.get(&store).try_into::<i64>().ok_or({
            let store = &store;
            (gas_amount!(store), WrongInjectedGas)
        })?;
        let allowance = gear_allowance.get(&store).try_into::<i64>().ok_or({
            let store = &store;
            (gas_amount!(store), WrongInjectedAllowance)
        })?;

        let runtime = store
            .state_mut()
            .take()
            .expect("is set before in `new`; qed");

        let State {
            mut ext, err: trap, ..
        } = runtime;

        ext.update_counters(gas as u64, allowance as u64);

        log::debug!("WasmiEnvironment::execute result = {res:?}");

        let trap_explanation = ext.trap_explanation();

        let termination_reason = if res.is_err() {
            let reason = trap_explanation
                .map(TerminationReason::Trap)
                .unwrap_or_else(|| trap.into_termination_reason());

            // success is unacceptable when there is an error
            if let TerminationReason::Success = reason {
                TerminationReason::Trap(TrapExplanation::Unknown)
            } else {
                reason
            }
        } else {
            TerminationReason::Success
        };

        Ok(BackendReport {
            termination_reason,
            memory_wrap: MemoryWrap::new(memory, store),
            ext,
        })
    }
}
