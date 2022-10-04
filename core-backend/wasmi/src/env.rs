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
use core::fmt;
use gear_backend_common::{
    calc_stack_end, error_processor::IntoExtError, AsTerminationReason, BackendReport, Environment,
    GetGasAmount, IntoExtInfo, StackEndError, TerminationReason, TrapExplanation,
    STACK_END_EXPORT_NAME,
};
use gear_core::{env::Ext, gas::GasAmount, memory::WasmPageNumber, message::DispatchKind};
use wasmi::{Engine, Extern, Linker, Memory as WasmiMemory, MemoryType, Module, Store};

#[derive(Debug, derive_more::Display, derive_more::From)]
pub enum WasmiEnvironmentError {
    #[display(fmt = "Failed to create env memory: {:?}", _0)]
    CreateEnvMemory(wasmi::errors::MemoryError),
    #[display(fmt = "Unable to link item: {:?}", _0)]
    Linking(wasmi::errors::LinkerError),
    #[display(fmt = "Unable to instantiate module: {:?}", _0)]
    ModuleInstantiation(wasmi::Error),
    #[display(fmt = "Unable to get wasm module exports: {}", _0)]
    GetWasmExports(String),
    #[display(fmt = "Entry point has wrong type: {}", _0)]
    EntryPointWrongType(String),
    #[display(fmt = "{}", _0)]
    PreExecutionHandler(String),
    #[from]
    StackEnd(StackEndError),
}

#[derive(Debug)]
pub struct Error {
    gas_amount: GasAmount,
    error: WasmiEnvironmentError,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        fmt::Display::fmt(&self.error, f)
    }
}

impl GetGasAmount for Error {
    fn gas_amount(&self) -> GasAmount {
        self.gas_amount.clone()
    }
}

impl From<(GasAmount, WasmiEnvironmentError)> for Error {
    fn from((gas_amount, error): (GasAmount, WasmiEnvironmentError)) -> Self {
        Self { gas_amount, error }
    }
}

/// Environment to run one module at a time providing Ext.
pub struct WasmiEnvironment;

impl<E> Environment<E> for WasmiEnvironment
where
    E: Ext + IntoExtInfo + GetGasAmount + 'static,
    E::Error: AsTerminationReason + IntoExtError,
{
    type Memory = MemoryWrap<E>;
    type Error = Error;

    fn execute<F, T>(
        ext: E,
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
        use WasmiEnvironmentError::*;

        let engine = Engine::default();
        let mut store: Store<HostState<E>> = Store::new(&engine, None);

        let mut linker: Linker<HostState<E>> = Linker::new();

        let memory_type = MemoryType::new(mem_size.0, None);
        let memory = WasmiMemory::new(&mut store, memory_type)
            .map_err(|e| Error::from((ext.gas_amount(), CreateEnvMemory(e))))?;

        linker
            .define("env", "memory", memory)
            .map_err(|e| Error::from((ext.gas_amount(), Linking(e))))?;

        let forbidden_funcs =
            (!ext.forbidden_funcs().is_empty()).then(|| ext.forbidden_funcs().clone());
        let functions = funcs_tree::build(&mut store, memory, forbidden_funcs);
        for (name, function) in functions {
            linker
                .define("env", name, function)
                .map_err(|e| Error::from((ext.gas_amount(), Linking(e))))?;
        }

        let module = Module::new(store.engine(), &mut &binary[..])
            .map_err(|e| Error::from((ext.gas_amount(), ModuleInstantiation(e))))?;

        let runtime = State {
            ext,
            err: FuncError::Terminated(TerminationReason::Success),
        };

        *store.state_mut() = Some(runtime);

        let (ext, memory_wrap, termination) = {
            let instance_pre = match linker.instantiate(&mut store, &module) {
                Ok(i) => i,
                Err(e) => {
                    let gas_amount = store
                        .state()
                        .as_ref()
                        .expect("set before; qed")
                        .ext
                        .gas_amount();
                    return Err((gas_amount, ModuleInstantiation(e)).into());
                }
            };

            let instance = match instance_pre.ensure_no_start(&mut store) {
                Ok(i) => i,
                Err(e) => {
                    let gas_amount = store
                        .state()
                        .as_ref()
                        .expect("set before; qed")
                        .ext
                        .gas_amount();
                    return Err((gas_amount, ModuleInstantiation(e.into())).into());
                }
            };

            let stack_end = instance
                .get_export(&store, STACK_END_EXPORT_NAME)
                .and_then(Extern::into_global)
                .and_then(|g| g.get(&store).try_into::<i32>());
            let stack_end_page = match calc_stack_end(stack_end) {
                Ok(s) => s,
                Err(e) => {
                    let gas_amount = store
                        .state()
                        .as_ref()
                        .expect("set before; qed")
                        .ext
                        .gas_amount();
                    return Err((gas_amount, StackEnd(e)).into());
                }
            };

            let mut memory_wrap = MemoryWrap::new(memory, store);
            match pre_execution_handler(&mut memory_wrap, stack_end_page) {
                Ok(_) => (),
                Err(e) => {
                    let gas_amount = memory_wrap
                        .store
                        .state()
                        .as_ref()
                        .expect("set before; qed")
                        .ext
                        .gas_amount();
                    return Err((gas_amount, PreExecutionHandler(e.to_string())).into());
                }
            };

            let res = if entries.contains(entry_point) {
                let func = match instance
                    .get_export(&memory_wrap.store, entry_point.into_entry())
                    .and_then(Extern::into_func)
                {
                    Some(f) => f,
                    None => {
                        let gas_amount = memory_wrap
                            .store
                            .state()
                            .as_ref()
                            .expect("set before; qed")
                            .ext
                            .gas_amount();
                        return Err((
                            gas_amount,
                            GetWasmExports(entry_point.into_entry().to_string()),
                        )
                            .into());
                    }
                };

                let entry_func = match func.typed::<(), (), _>(&mut memory_wrap.store) {
                    Ok(f) => f,
                    Err(_) => {
                        let gas_amount = memory_wrap
                            .store
                            .state()
                            .as_ref()
                            .expect("set before; qed")
                            .ext
                            .gas_amount();
                        return Err((
                            gas_amount,
                            EntryPointWrongType(entry_point.into_entry().to_string()),
                        )
                            .into());
                    }
                };

                entry_func.call(&mut memory_wrap.store, ())
            } else {
                Ok(())
            };

            let runtime = memory_wrap
                .store
                .state_mut()
                .take()
                .expect("set before the block; qed");

            let State { ext, err: trap, .. } = runtime;

            log::debug!("WasmiEnvironment::execute result = {res:?}");

            let trap_explanation = ext.trap_explanation();

            let termination = if res.is_err() {
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

            (ext, memory_wrap, termination)
        };

        Ok(BackendReport {
            termination_reason: termination,
            memory_wrap,
            ext,
        })
    }
}
