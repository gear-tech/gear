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
    funcs::{FuncError, FuncsHandler as Funcs},
    memory::MemoryWrap,
};
use alloc::{
    collections::{BTreeMap, BTreeSet},
    string::{String, ToString},
};
use core::fmt;
use gear_backend_common::{
    error_processor::IntoExtError, AsTerminationReason, BackendError, BackendReport, Environment,
    IntoExtInfo, TerminationReason, TrapExplanation,
};
use gear_core::{
    env::{Ext, ExtCarrier},
    gas::GasAmount,
    memory::WasmPageNumber,
    message::DispatchKind,
};
use gear_core_errors::MemoryError;
use wasmi::{
    memory_units::Pages, Externals, FuncInstance, FuncRef, GlobalDescriptor, GlobalRef,
    ImportResolver, MemoryDescriptor, MemoryInstance, MemoryRef, ModuleInstance, ModuleRef,
    RuntimeArgs, RuntimeValue, Signature, TableDescriptor, TableRef, Trap,
};

#[derive(Debug, derive_more::Display)]
pub enum WasmiEnvironmentError {
    #[display(fmt = "Unable to instantiate module: {:?}", _0)]
    ModuleInstantiation(wasmi::Error),
    #[display(fmt = "Unable to get wasm module exports: {}", _0)]
    GetWasmExports(String),
    #[display(fmt = "Unable to set module memory data")]
    SetModuleMemoryData,
    #[display(fmt = "Unable to save static pages initial data")]
    SaveStaticPagesInitialData,
    #[display(fmt = "Failed to create env memory: {:?}", _0)]
    CreateEnvMemory(wasmi::Error),
    #[display(fmt = "{}", _0)]
    Memory(MemoryError),
    #[display(fmt = "{}", _0)]
    PostExecutionHandler(String),
}

/// Environment to run one module at a time providing Ext.
pub struct WasmiEnvironment<E: Ext + IntoExtInfo> {
    runtime: Runtime<E>,
    instance: ModuleRef,
    defined_host_functions: DefinedHostFunctions<Runtime<E>, E::Error>,
    entries: BTreeSet<DispatchKind>,
}

pub struct Runtime<E: Ext> {
    pub ext: ExtCarrier<E>,
    pub memory: MemoryWrap,
    pub err: FuncError<E::Error>,
}

struct HostFuncIndex(usize);

/// Function pointer for specifying functions by the
/// supervisor in [`EnvironmentDefinitionBuilder`].
pub type HostFuncType<T, E> = fn(&mut T, &[RuntimeValue]) -> Result<ReturnValue, FuncError<E>>;

pub struct DefinedHostFunctions<T, E> {
    funcs: Vec<HostFuncType<T, E>>,
}

impl<T, E> Clone for DefinedHostFunctions<T, E> {
    fn clone(&self) -> DefinedHostFunctions<T, E> {
        DefinedHostFunctions {
            funcs: self.funcs.clone(),
        }
    }
}

impl<T, E> DefinedHostFunctions<T, E> {
    fn new() -> DefinedHostFunctions<T, E> {
        DefinedHostFunctions { funcs: Vec::new() }
    }

    fn define(&mut self, f: HostFuncType<T, E>) -> HostFuncIndex {
        let idx = self.funcs.len();
        self.funcs.push(f);
        HostFuncIndex(idx)
    }
}

#[derive(Debug)]
struct DummyHostError;

impl fmt::Display for DummyHostError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "DummyHostError")
    }
}

impl wasmi::HostError for DummyHostError {}

pub struct GuestExternals<'a, T: 'a, E> {
    pub state: &'a mut T,
    pub defined_host_functions: &'a DefinedHostFunctions<T, E>,
}

impl<'a, T, E> Externals for GuestExternals<'a, T, E> {
    fn invoke_index(
        &mut self,
        index: usize,
        args: RuntimeArgs,
    ) -> Result<Option<RuntimeValue>, Trap> {
        let args = args.as_ref().to_vec();

        let result = (self.defined_host_functions.funcs[index])(self.state, &args);
        match result {
            Ok(value) => Ok(match value {
                ReturnValue::Value(v) => Some(v),
                ReturnValue::Unit => None,
            }),
            Err(_e) => Err(Trap::Host(Box::new(DummyHostError)).into()),
        }
    }
}

/// Typed value that can be returned from a function.
///
/// Basically a `TypedValue` plus `Unit`, for functions which return nothing.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ReturnValue {
    /// For returning nothing.
    Unit,
    /// For returning some concrete value.
    Value(RuntimeValue),
}

impl From<RuntimeValue> for ReturnValue {
    fn from(v: RuntimeValue) -> ReturnValue {
        ReturnValue::Value(v)
    }
}

enum ExternVal {
    HostFunc(HostFuncIndex),
    Memory(MemoryRef),
}

/// A builder for the environment of the WASM module.
pub struct EnvironmentDefinitionBuilder<T, E> {
    map: BTreeMap<(Vec<u8>, Vec<u8>), ExternVal>,
    pub defined_host_functions: DefinedHostFunctions<T, E>,
    pub forbidden_funcs: BTreeSet<String>,
}

impl<T, E> EnvironmentDefinitionBuilder<T, E> {
    pub fn new(forbidden_funcs: BTreeSet<String>) -> EnvironmentDefinitionBuilder<T, E> {
        EnvironmentDefinitionBuilder {
            map: BTreeMap::new(),
            defined_host_functions: DefinedHostFunctions::new(),
            forbidden_funcs,
        }
    }

    pub fn add_host_func<N1, N2>(&mut self, module: N1, field: N2, f: HostFuncType<T, E>)
    where
        N1: Into<Vec<u8>>,
        N2: Into<Vec<u8>>,
    {
        let idx = self.defined_host_functions.define(f);
        self.map
            .insert((module.into(), field.into()), ExternVal::HostFunc(idx));
    }

    pub fn add_memory<N1, N2>(&mut self, module: N1, field: N2, mem: MemoryRef)
    where
        N1: Into<Vec<u8>>,
        N2: Into<Vec<u8>>,
    {
        self.map
            .insert((module.into(), field.into()), ExternVal::Memory(mem));
    }
}

impl<T, E> ImportResolver for EnvironmentDefinitionBuilder<T, E> {
    fn resolve_func(
        &self,
        module_name: &str,
        field_name: &str,
        signature: &Signature,
    ) -> Result<FuncRef, wasmi::Error> {
        let key = (
            module_name.as_bytes().to_owned(),
            field_name.as_bytes().to_owned(),
        );

        let externval = if self.forbidden_funcs.contains(field_name) {
            self.map
                .get(&(b"env".to_vec(), b"forbidden".to_vec()))
                .ok_or_else(|| {
                    log::debug!(
                        target: "gwasm",
                        "Export {}:{} is forbidden",
                        module_name,
                        field_name
                    );
                    wasmi::Error::Instantiation(String::new())
                })?
        } else {
            self.map.get(&key).ok_or_else(|| {
                log::debug!(
                    target: "gwasm",
                    "Export {}:{} not found",
                    module_name,
                    field_name
                );
                wasmi::Error::Instantiation(String::new())
            })?
        };

        let host_func_idx = match *externval {
            ExternVal::HostFunc(ref idx) => idx,
            _ => {
                log::debug!(
                    target: "gwasm",
                    "Export {}:{} is not a host func",
                    module_name,
                    field_name,
                );
                return Err(wasmi::Error::Instantiation(String::new()));
            }
        };
        Ok(FuncInstance::alloc_host(signature.clone(), host_func_idx.0))
    }

    fn resolve_global(
        &self,
        _module_name: &str,
        _field_name: &str,
        _global_type: &GlobalDescriptor,
    ) -> Result<GlobalRef, wasmi::Error> {
        log::debug!(target: "gwasm", "Importing globals is not supported yet");
        Err(wasmi::Error::Instantiation(String::new()))
    }

    fn resolve_memory(
        &self,
        module_name: &str,
        field_name: &str,
        _memory_type: &MemoryDescriptor,
    ) -> Result<MemoryRef, wasmi::Error> {
        let key = (
            module_name.as_bytes().to_owned(),
            field_name.as_bytes().to_owned(),
        );
        let externval = self.map.get(&key).ok_or_else(|| {
            log::debug!(
                target: "gwasm",
                "Memory export {}:{} not found",
                module_name,
                field_name
            );
            wasmi::Error::Instantiation(String::new())
        })?;
        let memory = match *externval {
            ExternVal::Memory(ref m) => m,
            _ => {
                log::debug!(
                    target: "gwasm",
                    "Export {}:{} is not a memory",
                    module_name,
                    field_name,
                );
                return Err(wasmi::Error::Instantiation(String::new()));
            }
        };
        Ok(memory.clone())
    }

    fn resolve_table(
        &self,
        _module_name: &str,
        _field_name: &str,
        _table_type: &TableDescriptor,
    ) -> Result<TableRef, wasmi::Error> {
        log::debug!("Importing tables is not supported yet");
        Err(wasmi::Error::Instantiation(String::new()))
    }
}

impl<E> Environment<E> for WasmiEnvironment<E>
where
    E: Ext + IntoExtInfo + 'static,
    E::Error: AsTerminationReason + IntoExtError,
{
    type Memory = MemoryWrap;
    type Error = WasmiEnvironmentError;

    fn new(
        ext: E,
        binary: &[u8],
        entries: BTreeSet<DispatchKind>,
        mem_size: WasmPageNumber,
    ) -> Result<Self, BackendError<Self::Error>> {
        let mut builder = EnvironmentDefinitionBuilder::new(
            ext.forbidden_funcs()
                .clone()
                .into_iter()
                .map(|s| s.to_string())
                .collect(),
        );

        builder.add_host_func("env", "forbidden", Funcs::forbidden);
        builder.add_host_func("env", "gr_block_height", Funcs::block_height);
        builder.add_host_func("env", "gr_block_timestamp", Funcs::block_timestamp);
        builder.add_host_func("env", "gr_create_program", Funcs::create_program);
        builder.add_host_func("env", "gr_create_program_wgas", Funcs::create_program_wgas);
        builder.add_host_func("env", "gr_debug", Funcs::debug);
        builder.add_host_func("env", "gr_error", Funcs::error);
        builder.add_host_func("env", "gr_exit", Funcs::exit);
        builder.add_host_func("env", "gr_exit_code", Funcs::exit_code);
        builder.add_host_func("env", "gr_gas_available", Funcs::gas_available);
        builder.add_host_func("env", "gr_leave", Funcs::leave);
        builder.add_host_func("env", "gr_msg_id", Funcs::msg_id);
        builder.add_host_func("env", "gr_origin", Funcs::origin);
        builder.add_host_func("env", "gr_program_id", Funcs::program_id);
        builder.add_host_func("env", "gr_read", Funcs::read);
        builder.add_host_func("env", "gr_reply", Funcs::reply);
        builder.add_host_func("env", "gr_reply_commit", Funcs::reply_commit);
        builder.add_host_func("env", "gr_reply_commit_wgas", Funcs::reply_commit_wgas);
        builder.add_host_func("env", "gr_reply_push", Funcs::reply_push);
        builder.add_host_func("env", "gr_reply_to", Funcs::reply_to);
        builder.add_host_func("env", "gr_reply_wgas", Funcs::reply_wgas);
        builder.add_host_func("env", "gr_send", Funcs::send);
        builder.add_host_func("env", "gr_send_commit", Funcs::send_commit);
        builder.add_host_func("env", "gr_send_commit_wgas", Funcs::send_commit_wgas);
        builder.add_host_func("env", "gr_send_init", Funcs::send_init);
        builder.add_host_func("env", "gr_send_push", Funcs::send_push);
        builder.add_host_func("env", "gr_send_wgas", Funcs::send_wgas);
        builder.add_host_func("env", "gr_size", Funcs::size);
        builder.add_host_func("env", "gr_source", Funcs::source);
        builder.add_host_func("env", "gr_value", Funcs::value);
        builder.add_host_func("env", "gr_value_available", Funcs::value_available);
        builder.add_host_func("env", "gr_wait", Funcs::wait);
        builder.add_host_func("env", "gr_wake", Funcs::wake);

        let ext_carrier = ExtCarrier::new(ext);

        let mem: MemoryRef = match MemoryInstance::alloc(Pages(mem_size.0 as usize), None) {
            Ok(mem) => mem,
            Err(e) => {
                return Err(BackendError {
                    reason: WasmiEnvironmentError::CreateEnvMemory(e),
                    gas_amount: ext_carrier.into_inner().into_gas_amount(),
                })
            }
        };

        builder.add_memory("env", "memory", mem.clone());
        builder.add_host_func("env", "alloc", Funcs::alloc);
        builder.add_host_func("env", "free", Funcs::free);
        builder.add_host_func("env", "gas", Funcs::gas);

        let runtime = Runtime {
            ext: ext_carrier,
            memory: MemoryWrap::new(mem),
            err: FuncError::Terminated(TerminationReason::Success),
        };

        let defined_host_functions = builder.defined_host_functions.clone();
        let module = match wasmi::Module::from_buffer(binary) {
            Ok(module) => module,
            Err(e) => {
                return Err(BackendError {
                    reason: WasmiEnvironmentError::ModuleInstantiation(e),
                    gas_amount: runtime.ext.into_inner().into_gas_amount(),
                })
            }
        };
        let instance = match ModuleInstance::new(&module, &builder) {
            Ok(inst) => inst.not_started_instance().clone(),
            Err(e) => {
                return Err(BackendError {
                    reason: WasmiEnvironmentError::ModuleInstantiation(e),
                    gas_amount: runtime.ext.into_inner().into_gas_amount(),
                })
            }
        };

        Ok(WasmiEnvironment {
            runtime,
            instance,
            defined_host_functions,
            entries,
        })
    }

    fn get_stack_mem_end(&mut self) -> Option<WasmPageNumber> {
        // '__gear_stack_end' export is inserted in wasm-proc or wasm-builder
        let global = self
            .instance
            .export_by_name("__gear_stack_end")?
            .as_global()?
            .get();
        global.try_into::<i32>().and_then(|addr| {
            if addr < 0 {
                None
            } else {
                Some(WasmPageNumber(
                    (addr as usize / WasmPageNumber::size()) as u32,
                ))
            }
        })
    }

    fn get_mem(&self) -> &Self::Memory {
        &self.runtime.memory
    }

    fn get_mem_mut(&mut self) -> &mut Self::Memory {
        &mut self.runtime.memory
    }

    fn execute<F, T>(
        mut self,
        entry_point: &DispatchKind,
        post_execution_handler: F,
    ) -> Result<BackendReport, BackendError<Self::Error>>
    where
        F: FnOnce(&Self::Memory) -> Result<(), T>,
        T: fmt::Display,
    {
        let res = if self.entries.contains(entry_point) {
            let mut externals = GuestExternals {
                state: &mut self.runtime,
                defined_host_functions: &self.defined_host_functions,
            };
            self.instance
                .invoke_export(entry_point.into_entry(), &[], &mut externals)
                .map(|_| ())
        } else {
            Ok(())
        };

        // Page which is right after stack last page
        let stack_end_page = self.get_stack_mem_end();
        log::trace!("Stack end page = {stack_end_page:?}");

        let Runtime {
            ext,
            memory,
            err: trap,
        } = self.runtime;

        log::debug!("WasmiEnvironment::execute result = {res:?}");

        let (info, trap_explanation) = ext
            .into_inner()
            .into_ext_info(&memory, stack_end_page.unwrap_or_default())
            .map_err(|(reason, gas_amount)| BackendError {
                reason: WasmiEnvironmentError::Memory(reason),
                gas_amount,
            })?;

        let termination = if res.is_err() {
            let reason = trap_explanation
                .map(TerminationReason::Trap)
                .unwrap_or_else(|| trap.into_termination_reason());

            // success is unacceptable when there is error
            if let TerminationReason::Success = reason {
                TerminationReason::Trap(TrapExplanation::Unknown)
            } else {
                reason
            }
        } else {
            TerminationReason::Success
        };

        match post_execution_handler(&memory) {
            Ok(_) => Ok(BackendReport { termination, info }),
            Err(e) => Err(BackendError {
                reason: WasmiEnvironmentError::PostExecutionHandler(e.to_string()),
                gas_amount: info.gas_amount,
            }),
        }
    }

    fn into_gas_amount(self) -> GasAmount {
        self.runtime.ext.into_inner().into_gas_amount()
    }
}
