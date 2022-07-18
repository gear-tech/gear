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

use crate::memory::MemoryWrapExternal;
use alloc::{
    collections::BTreeSet,
    string::{String, ToString},
    vec::Vec,
};
use gear_backend_common::{
    error_processor::IntoExtError, AsTerminationReason, BackendError, BackendReport, Environment,
    ExtInfo, IntoExtInfo, TerminationReason, TrapExplanation,
};
use gear_core::{env::Ext, gas::GasAmount, memory::WasmPageNumber, message::DispatchKind};
use gear_core_errors::MemoryError;
use wasmtime::{Engine, Extern, Instance, Memory as WasmtimeMemory, MemoryType, Module, Store};

/// Data type in wasmtime store
pub struct StoreData<E: Ext> {
    pub ext: E,
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
    PostExecutionHandler(String),
}

/// Environment to run one module at a time providing Ext.
pub struct WasmtimeEnvironment<E: Ext + 'static> {
    ext: E,
    memory_wrap: MemoryWrapExternal<E>,
    instance: Instance,
}

// impl<E> Environment<E> for WasmtimeEnvironment<E>
// where
//     E: Ext + IntoExtInfo,
//     E::Error: AsTerminationReason + IntoExtError,
// {
//     type Memory = MemoryWrapExternal<E>;
//     type Error = WasmtimeEnvironmentError;
//
//     fn new(
//         ext: E,
//         binary: &[u8],
//         _entries: BTreeSet<DispatchKind>,
//         mem_size: WasmPageNumber,
//     ) -> Result<Self, BackendError<Self::Error>> {
//         let forbidden_funcs = ext.forbidden_funcs().clone();
//         let ext_carrier = ExtCarrier::new(ext);
//
//         let engine = Engine::default();
//         let store_data = StoreData {
//             ext: ext_carrier,
//             termination_reason: TerminationReason::Success,
//         };
//         let mut store = Store::<StoreData<E>>::new(&engine, store_data);
//
//         // Creates new wasm memory
//         let memory = match WasmtimeMemory::new(&mut store, MemoryType::new(mem_size.0, None)) {
//             Ok(mem) => mem,
//             Err(e) => {
//                 return Err(BackendError {
//                     reason: WasmtimeEnvironmentError::CreateEnvMemory(e),
//                     gas_amount: ext_carrier.into_inner().into_gas_amount(),
//                 })
//             }
//         };
//
//         let funcs = funcs_tree::build(&mut store, memory, Some(forbidden_funcs));
//         let module = match Module::new(&engine, binary) {
//             Ok(module) => module,
//             Err(e) => {
//                 return Err(BackendError {
//                     reason: WasmtimeEnvironmentError::ModuleCreation(e),
//                     gas_amount: ext_carrier.into_inner().into_gas_amount(),
//                 })
//             }
//         };
//
//         let mut imports = Vec::with_capacity(module.imports().len());
//         for import in module.imports() {
//             if import.module() != "env" {
//                 return Err(BackendError {
//                     reason: WasmtimeEnvironmentError::NonEnvImport(import.name().map(Into::into)),
//                     gas_amount: ext_carrier.into_inner().into_gas_amount(),
//                 });
//             }
//             imports.push((import.name(), Option::<Extern>::None));
//         }
//
//         for (import_name, ref mut ext) in imports.iter_mut() {
//             if let Some(name) = import_name {
//                 *ext = match *name {
//                     "memory" => Some(Extern::Memory(memory)),
//                     key if funcs.contains_key(key) => Some(funcs[key].into()),
//                     _ => continue,
//                 }
//             }
//         }
//
//         let mut externs = Vec::with_capacity(imports.len());
//         for (name, host_function) in imports {
//             if let Some(host_function) = host_function {
//                 externs.push(host_function);
//             } else {
//                 return Err(BackendError {
//                     reason: WasmtimeEnvironmentError::MissingImport(name.map(Into::into)),
//                     gas_amount: ext_carrier.into_inner().into_gas_amount(),
//                 });
//             }
//         }
//
//         let instance = match Instance::new(&mut store, &module, &externs) {
//             Ok(instance) => instance,
//             Err(e) => {
//                 return Err(BackendError {
//                     reason: WasmtimeEnvironmentError::InstanceCreation(e),
//                     gas_amount: ext_carrier.into_inner().into_gas_amount(),
//                 })
//             }
//         };
//
//         let memory_wrap = MemoryWrapExternal { mem: memory, store };
//
//         Ok(WasmtimeEnvironment {
//             ext: ext_carrier,
//             memory_wrap,
//             instance,
//         })
//     }
//
//     fn get_stack_mem_end(&mut self) -> Option<WasmPageNumber> {
//         // `__gear_stack_end` export is inserted in wasm-proc or wasm-builder
//         let global = self
//             .instance
//             .get_global(&mut self.memory_wrap.store, "__gear_stack_end")?;
//         global
//             .get(&mut self.memory_wrap.store)
//             .i32()
//             .and_then(|addr| {
//                 if addr < 0 {
//                     None
//                 } else {
//                     Some(WasmPageNumber(
//                         (addr as usize / WasmPageNumber::size()) as u32,
//                     ))
//                 }
//             })
//     }
//
//     fn get_mem(&self) -> &Self::Memory {
//         &self.memory_wrap
//     }
//
//     fn get_mem_mut(&mut self) -> &mut Self::Memory {
//         &mut self.memory_wrap
//     }
//
//     fn execute<F, T>(
//         mut self,
//         entry_point: &DispatchKind,
//         post_execution_handler: F,
//     ) -> Result<BackendReport, BackendError<Self::Error>>
//     where
//         F: FnOnce(&Self::Memory) -> Result<(), T>,
//         T: fmt::Display,
//     {
//         struct PreparedInfo<E: Ext> {
//             info: ExtInfo,
//             trap_explanation: Option<TrapExplanation>,
//             memory_wrap: MemoryWrapExternal<E>,
//         }
//
//         let func = self
//             .instance
//             .get_func(&mut self.memory_wrap.store, entry_point.into_entry());
//
//         let prepare_info = |this: Self| -> Result<PreparedInfo<E>, BackendError<Self::Error>> {
//             let WasmtimeEnvironment {
//                 ext, memory_wrap, ..
//             } = this;
//             ext.into_inner()
//                 .into_ext_info(&memory_wrap)
//                 .map_err(|(reason, gas_amount)| BackendError {
//                     reason: WasmtimeEnvironmentError::MemoryAccess(reason),
//                     gas_amount,
//                 })
//                 .map(|(info, trap_explanation)| PreparedInfo {
//                     info,
//                     trap_explanation,
//                     memory_wrap,
//                 })
//         };
//
//         let entry_func = if let Some(f) = func {
//             // Entry function found
//             f
//         } else {
//             let PreparedInfo {
//                 info,
//                 trap_explanation: _,
//                 memory_wrap,
//             } = prepare_info(self)?;
//
//             // Entry function not found, so we mean this as empty function
//             return match post_execution_handler(&memory_wrap) {
//                 Ok(_) => Ok(BackendReport {
//                     termination: TerminationReason::Success,
//                     info,
//                 }),
//                 Err(e) => Err(BackendError {
//                     reason: WasmtimeEnvironmentError::PostExecutionHandler(e.to_string()),
//                     gas_amount: info.gas_amount,
//                 }),
//             };
//         };
//
//         let res = entry_func.call(&mut self.memory_wrap.store, &[], &mut []);
//         log::debug!("execution result: {:?}", res);
//
//         let termination_reason = self.memory_wrap.store.data().termination_reason.clone();
//
//         let PreparedInfo {
//             info,
//             trap_explanation,
//             memory_wrap,
//         } = prepare_info(self)?;
//
//         let termination = if res.is_err() {
//             let reason = trap_explanation
//                 .map(TerminationReason::Trap)
//                 .unwrap_or(termination_reason);
//
//             // success is unacceptable when there is error
//             if let TerminationReason::Success = reason {
//                 TerminationReason::Trap(TrapExplanation::Unknown)
//             } else {
//                 reason
//             }
//         } else {
//             TerminationReason::Success
//         };
//
//         match post_execution_handler(&memory_wrap) {
//             Ok(_) => Ok(BackendReport { termination, info }),
//             Err(e) => Err(BackendError {
//                 reason: WasmtimeEnvironmentError::PostExecutionHandler(e.to_string()),
//                 gas_amount: info.gas_amount,
//             }),
//         }
//     }
//
//     fn into_gas_amount(self) -> GasAmount {
//         self.ext.into_inner().into_gas_amount()
//     }
// }
