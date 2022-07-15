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

//! Wasmtime environment for running a module.

use core::fmt;

use crate::{funcs_tree, memory::MemoryWrapExternal};
use alloc::{
    collections::BTreeSet,
    string::{String, ToString},
    vec::Vec,
};
use gear_backend_common::{
    error_processor::IntoExtError, AsTerminationReason, BackendError, BackendReport, Environment,
    IntoExtInfo, TerminationReason, TrapExplanation,
};
use gear_core::{
    env::{ClonedExtCarrier, Ext, ExtCarrier},
    memory::WasmPageNumber,
    message::DispatchKind,
};
use gear_core_errors::MemoryError;
use wasmtime::{Engine, Extern, Instance, Memory as WasmtimeMemory, MemoryType, Module, Store};

/// Data type in wasmtime store
pub struct StoreData<E: Ext> {
    pub ext: ClonedExtCarrier<E>,
    pub termination_reason: TerminationReason,
}

#[derive(Debug, derive_more::Display)]
pub enum WasmtimeEnvironmentError {
    #[display(fmt = "Function {:?} is not env", _0)]
    NonEnvImport(Option<String>),
    #[display(fmt = "Function {:?} definition wasn't found", _0)]
    MissingImport(Option<String>),
    #[display(fmt = "Unable to create module: {}", _0)]
    ModuleCreation(anyhow::Error),
    #[display(fmt = "Unable to create instance: {}", _0)]
    InstanceCreation(anyhow::Error),
    #[display(fmt = "Unable to set module memory data")]
    SetModuleMemoryData,
    #[display(fmt = "Unable to save static pages initial data")]
    SaveStaticPagesInitialData,
    #[display(fmt = "Failed to create env memory: {}", _0)]
    CreateEnvMemory(anyhow::Error),
    #[display(fmt = "{}", _0)]
    MemoryAccess(MemoryError),
    #[display(fmt = "{}", _0)]
    PreExecutionHandler(String),
}

/// Environment to run one module at a time providing Ext.
pub struct WasmtimeEnvironment;

impl<E> Environment<E> for WasmtimeEnvironment
where
    E: Ext + IntoExtInfo + 'static,
    E::Error: AsTerminationReason + IntoExtError,
{
    type Memory = MemoryWrapExternal<E>;
    type Error = WasmtimeEnvironmentError;

    fn execute<F, T>(
        ext_carrier: &mut ExtCarrier<E>,
        binary: &[u8],
        _entries: BTreeSet<DispatchKind>,
        mem_size: WasmPageNumber,
        entry_point: &DispatchKind,
        pre_execution_handler: F,
    ) -> Result<BackendReport<Self::Memory>, BackendError<Self::Error>>
    where
        F: FnOnce(&mut Self::Memory) -> Result<(), T>,
        T: fmt::Display,
    {
        let forbidden_funcs = ext_carrier
            .with(|ext| ext.forbidden_funcs().clone())
            .expect("");

        let engine = Engine::default();
        let store_data = StoreData {
            ext: ext_carrier.cloned(),
            termination_reason: TerminationReason::Success,
        };
        let mut store = Store::<StoreData<E>>::new(&engine, store_data);

        // Creates new wasm memory
        let memory = match WasmtimeMemory::new(&mut store, MemoryType::new(mem_size.0, None)) {
            Ok(mem) => mem,
            Err(e) => {
                return Err(BackendError {
                    reason: WasmtimeEnvironmentError::CreateEnvMemory(e),
                })
            }
        };

        let module = match Module::new(store.engine(), binary) {
            Ok(module) => module,
            Err(e) => {
                return Err(BackendError {
                    reason: WasmtimeEnvironmentError::ModuleCreation(e),
                })
            }
        };

        let mut imports = Vec::with_capacity(module.imports().len());
        for import in module.imports() {
            if import.module() != "env" {
                return Err(BackendError {
                    reason: WasmtimeEnvironmentError::NonEnvImport(import.name().map(Into::into)),
                });
            }
            imports.push((import.name(), Option::<Extern>::None));
        }

        let funcs = funcs_tree::build(&mut store, memory, Some(forbidden_funcs));
        for (import_name, ref mut ext) in imports.iter_mut() {
            if let Some(name) = import_name {
                *ext = match *name {
                    "memory" => Some(Extern::Memory(memory)),
                    key if funcs.contains_key(key) => Some(funcs[key].into()),
                    _ => continue,
                }
            }
        }

        let mut externs = Vec::with_capacity(imports.len());
        for (name, host_function) in imports {
            if let Some(host_function) = host_function {
                externs.push(host_function);
            } else {
                return Err(BackendError {
                    reason: WasmtimeEnvironmentError::MissingImport(name.map(Into::into)),
                });
            }
        }

        let instance = match Instance::new(&mut store, &module, &externs) {
            Ok(instance) => instance,
            Err(e) => {
                return Err(BackendError {
                    reason: WasmtimeEnvironmentError::InstanceCreation(e),
                })
            }
        };

        // `__gear_stack_end` export is inserted in wasm-proc or wasm-builder
        let stack_end_page = instance
            .get_global(&mut store, "__gear_stack_end")
            .and_then(|global| {
                global.get(&mut store).i32().and_then(|addr| {
                    if addr < 0 {
                        None
                    } else {
                        Some(WasmPageNumber(
                            (addr as usize / WasmPageNumber::size()) as u32,
                        ))
                    }
                })
            });

        let mut memory_wrap = MemoryWrapExternal { mem: memory, store };

        pre_execution_handler(&mut memory_wrap).map_err(|e| BackendError {
            reason: WasmtimeEnvironmentError::PreExecutionHandler(e.to_string()),
        })?;

        let func = instance.get_func(&mut memory_wrap.store, entry_point.into_entry());

        let entry_func = if let Some(f) = func {
            // Entry function found
            f
        } else {
            // Entry function not found, so we mean this as empty function
            return Ok((TerminationReason::Success, memory_wrap, stack_end_page));
        };

        let res = entry_func.call(&mut memory_wrap.store, &[], &mut []);
        log::debug!("execution result: {:?}", res);

        let termination_reason = memory_wrap.store.data().termination_reason.clone();

        // let PreparedInfo {
        //     info,
        //     trap_explanation,
        //     memory_wrap,
        // } = prepare_info(ext_carrier.into_inner(), memory_wrap)?;

        let termination = if res.is_err() {
            let reason = ext_carrier
                .with(|ext| ext.trap_explanation())
                .unwrap()
                .map(TerminationReason::Trap)
                .unwrap_or(termination_reason);

            // success is unacceptable when there is error
            if let TerminationReason::Success = reason {
                TerminationReason::Trap(TrapExplanation::Unknown)
            } else {
                reason
            }
        } else {
            TerminationReason::Success
        };

        Ok((termination, memory_wrap, stack_end_page))
    }
}
