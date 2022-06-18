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

use alloc::{
    collections::BTreeMap,
    format,
    string::{String, ToString},
    vec::Vec,
};
use gear_backend_common::{
    error_processor::IntoExtError, AsTerminationReason, BackendError, BackendReport, Environment,
    ExtInfo, IntoExtInfo, TerminationReason,
};
use gear_core::{
    env::{ClonedExtCarrier, Ext, ExtCarrier},
    gas::GasAmount,
    memory::{Memory, WasmPageNumber},
};
use gear_core_errors::MemoryError;
use wasmtime::{
    Engine, Extern, Func, Instance, Memory as WasmtimeMemory, MemoryType, Module, Store, Trap,
};

use crate::{funcs::FuncsHandler as Funcs, memory::MemoryWrapExternal};

/// Data type in wasmtime store
pub struct StoreData<E: Ext> {
    pub ext: ClonedExtCarrier<E>,
    pub termination_reason: Option<TerminationReason>,
}

#[derive(Debug, derive_more::Display)]
pub enum WasmtimeEnvironmentError {
    #[display(fmt = "Non-env imports are not supported")]
    NonEnvImports,
    #[display(fmt = "Missing import")]
    MissingImport,
    #[display(fmt = "Unable to create module")]
    ModuleCreation,
    #[display(fmt = "Unable to create instance")]
    InstanceCreation,
    #[display(fmt = "Unable to set module memory data")]
    SetModuleMemoryData,
    #[display(fmt = "Unable to save static pages initial data")]
    SaveStaticPagesInitialData,
    #[display(fmt = "Failed to create env memory")]
    CreateEnvMemory,
    #[display(fmt = "{}", _0)]
    MemoryAccess(MemoryError),
    #[display(fmt = "{}", _0)]
    PostExecutionHandler(String),
}

/// Environment to run one module at a time providing Ext.
pub struct WasmtimeEnvironment<E: Ext + 'static> {
    ext: ExtCarrier<E>,
    memory_wrap: MemoryWrapExternal<E>,
    instance: Instance,
}

impl<E> Environment<E> for WasmtimeEnvironment<E>
where
    E: Ext + IntoExtInfo,
    E::Error: AsTerminationReason + IntoExtError,
{
    type Error = WasmtimeEnvironmentError;

    fn new(
        ext: E,
        binary: &[u8],
        mem_size: WasmPageNumber,
    ) -> Result<Self, BackendError<Self::Error>> {
        let forbidden_funcs = ext.forbidden_funcs().clone();
        let ext_carrier = ExtCarrier::new(ext);

        let engine = Engine::default();
        let store_data = StoreData {
            ext: ext_carrier.cloned(),
            termination_reason: None,
        };
        let mut store = Store::<StoreData<E>>::new(&engine, store_data);

        // Creates new wasm memory
        let memory = match WasmtimeMemory::new(&mut store, MemoryType::new(mem_size.0, None)) {
            Ok(mem) => mem,
            Err(e) => {
                return Err(BackendError {
                    reason: WasmtimeEnvironmentError::CreateEnvMemory,
                    description: Some(e.to_string().into()),
                    gas_amount: ext_carrier.into_inner().into_gas_amount(),
                })
            }
        };

        let mut funcs = BTreeMap::<&'static str, Func>::new();
        funcs.insert("alloc", Funcs::alloc(&mut store, memory));
        funcs.insert("free", Funcs::free(&mut store));
        funcs.insert("gas", Funcs::gas(&mut store));
        funcs.insert("gr_block_height", Funcs::block_height(&mut store));
        funcs.insert("gr_block_timestamp", Funcs::block_timestamp(&mut store));
        funcs.insert(
            "gr_create_program",
            Funcs::create_program(&mut store, memory),
        );
        funcs.insert(
            "gr_create_program_wgas",
            Funcs::create_program_wgas(&mut store, memory),
        );
        funcs.insert("gr_exit_code", Funcs::exit_code(&mut store));
        funcs.insert("gr_gas_available", Funcs::gas_available(&mut store));
        funcs.insert("gr_debug", Funcs::debug(&mut store, memory));
        funcs.insert("gr_exit", Funcs::exit(&mut store, memory));
        funcs.insert("gr_origin", Funcs::origin(&mut store, memory));
        funcs.insert("gr_msg_id", Funcs::msg_id(&mut store, memory));
        funcs.insert("gr_program_id", Funcs::program_id(&mut store, memory));
        funcs.insert("gr_read", Funcs::read(&mut store, memory));
        funcs.insert("gr_reply", Funcs::reply(&mut store, memory));
        funcs.insert("gr_reply_wgas", Funcs::reply_wgas(&mut store, memory));
        funcs.insert("gr_reply_commit", Funcs::reply_commit(&mut store, memory));
        funcs.insert(
            "gr_reply_commit_wgas",
            Funcs::reply_commit_wgas(&mut store, memory),
        );
        funcs.insert("gr_reply_push", Funcs::reply_push(&mut store, memory));
        funcs.insert("gr_reply_to", Funcs::reply_to(&mut store, memory));
        funcs.insert("gr_send_wgas", Funcs::send_wgas(&mut store, memory));
        funcs.insert("gr_send", Funcs::send(&mut store, memory));
        funcs.insert(
            "gr_send_commit_wgas",
            Funcs::send_commit_wgas(&mut store, memory),
        );
        funcs.insert("gr_send_commit", Funcs::send_commit(&mut store, memory));
        funcs.insert("gr_send_init", Funcs::send_init(&mut store, memory));
        funcs.insert("gr_send_push", Funcs::send_push(&mut store, memory));
        funcs.insert("gr_size", Funcs::size(&mut store));
        funcs.insert("gr_source", Funcs::source(&mut store, memory));
        funcs.insert("gr_value", Funcs::value(&mut store, memory));
        funcs.insert(
            "gr_value_available",
            Funcs::value_available(&mut store, memory),
        );
        funcs.insert("gr_leave", Funcs::leave(&mut store));
        funcs.insert("gr_wait", Funcs::wait(&mut store));
        funcs.insert("gr_wake", Funcs::wake(&mut store, memory));
        funcs.insert("gr_error", Funcs::error(&mut store, memory));

        for name in forbidden_funcs {
            funcs.insert(name, Funcs::forbidden(&mut store));
        }

        let module = match Module::new(&engine, binary) {
            Ok(module) => module,
            Err(e) => {
                return Err(BackendError {
                    reason: WasmtimeEnvironmentError::ModuleCreation,
                    description: Some(e.to_string().into()),
                    gas_amount: ext_carrier.into_inner().into_gas_amount(),
                })
            }
        };

        let mut imports = Vec::with_capacity(module.imports().len());
        for import in module.imports() {
            if import.module() != "env" {
                return Err(BackendError {
                    reason: WasmtimeEnvironmentError::NonEnvImports,
                    description: import
                        .name()
                        .map(|v| format!("Function {:?} is not env", v).into()),
                    gas_amount: ext_carrier.into_inner().into_gas_amount(),
                });
            }
            imports.push((import.name(), Option::<Extern>::None));
        }

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
                    reason: WasmtimeEnvironmentError::MissingImport,
                    description: name
                        .map(|v| format!("Function {:?} definition wasn't found", v).into()),
                    gas_amount: ext_carrier.into_inner().into_gas_amount(),
                });
            }
        }

        let instance = match Instance::new(&mut store, &module, &externs) {
            Ok(instance) => instance,
            Err(e) => {
                return Err(BackendError {
                    reason: WasmtimeEnvironmentError::InstanceCreation,
                    description: Some(e.to_string().into()),
                    gas_amount: ext_carrier.into_inner().into_gas_amount(),
                })
            }
        };

        let memory_wrap = MemoryWrapExternal { mem: memory, store };

        Ok(WasmtimeEnvironment {
            ext: ext_carrier,
            memory_wrap,
            instance,
        })
    }

    fn get_stack_mem_end(&mut self) -> Option<WasmPageNumber> {
        // `__gear_stack_end` export is inserted in wasm-proc or wasm-builder
        let global = self
            .instance
            .get_global(&mut self.memory_wrap.store, "__gear_stack_end")?;
        global
            .get(&mut self.memory_wrap.store)
            .i32()
            .and_then(|addr| {
                if addr < 0 {
                    None
                } else {
                    Some(WasmPageNumber(
                        (addr as usize / WasmPageNumber::size()) as u32,
                    ))
                }
            })
    }

    fn get_mem(&self) -> &dyn Memory {
        &self.memory_wrap
    }

    fn get_mem_mut(&mut self) -> &mut dyn Memory {
        &mut self.memory_wrap
    }

    fn execute<F, T>(
        mut self,
        entry_point: &str,
        post_execution_handler: F,
    ) -> Result<BackendReport, BackendError<Self::Error>>
    where
        F: FnOnce(&dyn Memory) -> Result<(), T>,
        T: fmt::Display,
    {
        let func = self
            .instance
            .get_func(&mut self.memory_wrap.store, entry_point);

        let prepare_info =
            |this: Self| -> Result<(ExtInfo, MemoryWrapExternal<E>), BackendError<Self::Error>> {
                let WasmtimeEnvironment {
                    ext, memory_wrap, ..
                } = this;
                ext.into_inner()
                    .into_ext_info(&memory_wrap)
                    .map_err(|(reason, gas_amount)| BackendError {
                        reason: WasmtimeEnvironmentError::MemoryAccess(reason),
                        description: None,
                        gas_amount,
                    })
                    .map(|info| (info, memory_wrap))
            };

        let entry_func = if let Some(f) = func {
            // Entry function found
            f
        } else {
            let (info, memory_wrap) = prepare_info(self)?;

            // Entry function not found, so we mean this as empty function
            return match post_execution_handler(&memory_wrap) {
                Ok(_) => Ok(BackendReport {
                    termination: TerminationReason::Success,
                    info,
                }),
                Err(e) => Err(BackendError {
                    reason: WasmtimeEnvironmentError::PostExecutionHandler(e.to_string()),
                    description: None,
                    gas_amount: info.gas_amount,
                }),
            };
        };

        let res = entry_func.call(&mut self.memory_wrap.store, &[], &mut []);

        let termination_reason = self.memory_wrap.store.data().termination_reason.clone();

        let (info, memory_wrap) = prepare_info(self)?;

        let termination = if let Err(e) = &res {
            let reason = if let Some(_trap) = e.downcast_ref::<Trap>() {
                info.exit_argument
                    .map(TerminationReason::Exit)
                    .or(termination_reason)
            } else {
                None
            };

            reason.unwrap_or_else(|| TerminationReason::Trap {
                explanation: info.trap_explanation.clone(),
                description: Some(e.to_string().into()),
            })
        } else {
            TerminationReason::Success
        };

        match post_execution_handler(&memory_wrap) {
            Ok(_) => Ok(BackendReport { termination, info }),
            Err(e) => Err(BackendError {
                reason: WasmtimeEnvironmentError::PostExecutionHandler(e.to_string()),
                description: None,
                gas_amount: info.gas_amount,
            }),
        }
    }

    fn into_gas_amount(self) -> GasAmount {
        self.ext.into_inner().into_gas_amount()
    }
}
