// This file is part of Gear.

// Copyright (C) Gear Technologies Inc.
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

//! Wasmer specific impls for sandbox

use gear_sandbox_env::{GLOBAL_NAME_GAS, HostError, Instantiate, WasmReturnValue};
use parity_scale_codec::{Decode, Encode};
use sp_wasm_interface_common::{Pointer, ReturnValue, Value, WordSize, util};
use std::{cell::RefCell, path::PathBuf, rc::Rc};
use wasmer::{AsStoreMut, RuntimeError, Store};
use wasmer_types::TrapCode;

use crate::{
    error::{Error, Result},
    sandbox::{
        BackendInstanceBundle, GuestEnvironment, InstantiationError, Memory, SandboxInstance,
        SupervisorContext, SupervisorFuncIndex,
    },
    store_refcell,
    util::MemoryTransfer,
};

pub type StoreRefCell = store_refcell::StoreRefCell<wasmer::Store>;

environmental::environmental!(SupervisorContextStore: trait SupervisorContext);

mod store_refcell_ctx {
    use std::rc::Rc;

    use wasmer::StoreMut;

    use super::{StoreRefCell, store_refcell::BorrowScopeError};

    // We cannot store `StoreRefCell` in `wasmer::FunctionEnv` because it doesn't implement Send/Sync,
    // so we have to use `environment!` to access it from `dispatch_function` functions.
    environmental::environmental!(StoreRefCellEnv: Rc<StoreRefCell>);

    /// Convenience wrapper of `environment::using` function.
    pub fn using<R, F: FnOnce() -> R>(protected: &mut Rc<StoreRefCell>, f: F) -> R {
        StoreRefCellEnv::using(protected, f)
    }

    /// Creates re-borrow scope with `StoreRefCell` stored in `environment!` and provided mutable store reference.
    pub fn with_borrow_scope<R, F: FnOnce() -> R>(
        storemut: &mut StoreMut,
        f: F,
    ) -> Option<Result<R, BorrowScopeError>> {
        StoreRefCellEnv::with(|store_refcell: &mut Rc<StoreRefCell>| {
            store_refcell.borrow_scope(storemut, f)
        })
    }
}

pub struct Env {
    // Gas global is optional because it will be initialized after instance creation.
    // See `instantiate` function.
    gas_global: Option<wasmer::Global>,
}

/// Wasmer specific context
pub struct Backend {
    store: Rc<StoreRefCell>,
}

impl Default for Backend {
    fn default() -> Self {
        Self::new()
    }
}

impl Backend {
    pub fn new() -> Self {
        let compiler = wasmer::sys::Singlepass::default();
        Backend {
            store: Rc::new(StoreRefCell::new(wasmer::Store::new(compiler))),
        }
    }

    pub fn store(&self) -> &Rc<StoreRefCell> {
        &self.store
    }
}

/// Invoke a function within a sandboxed module
pub fn invoke(
    instance: &wasmer::Instance,
    store: &Rc<StoreRefCell>,
    export_name: &str,
    args: &[Value],
    supervisor_context: &mut dyn SupervisorContext,
) -> std::result::Result<Option<Value>, Error> {
    let function = instance
        .exports
        .get_function(export_name)
        .map_err(|error| Error::Sandbox(error.to_string()))?;

    let args: Vec<wasmer::Value> = args.iter().map(into_wasmer_val).collect();

    let wasmer_result = SupervisorContextStore::using(supervisor_context, || {
        store_refcell_ctx::using(&mut store.clone(), || {
            function
                .call(&mut store.borrow_mut(), &args)
                .map_err(|error| {
                    if error.clone().to_trap() == Some(TrapCode::StackOverflow) {
                        // Panic stops process queue execution in that case.
                        // This allows to avoid error lead to consensus failures, that must be handled
                        // in node binaries forever. If this panic occur, then we must increase stack memory size,
                        // or tune stack limit injection.
                        // see also https://github.com/wasmerio/wasmer/issues/4181
                        let err_msg = format!(
                            "invoke: Suppose that this can not happen, because we have a stack limit instrumentation in programs. \
                            Export name - {export_name}, args - {args:?}",
                        );

                        log::error!("{err_msg}");
                        unreachable!("{err_msg}")
                    }
                    Error::Sandbox(error.to_string())
                })
        })
    })?;

    match wasmer_result.as_ref() {
        [] => Ok(None),

        [wasm_value] => match into_value(wasm_value) {
            None => Err(Error::Sandbox(format!(
                "Unsupported return value: {wasm_value:?}",
            ))),
            Some(v) => Ok(Some(v)),
        },

        _ => Err(Error::Sandbox(
            "multiple return types are not supported yet".into(),
        )),
    }
}

#[cfg(feature = "gear-wasmer-cache")]
fn cache_base_path() -> PathBuf {
    use std::sync::OnceLock;
    use tempfile::TempDir;

    static CACHE_DIR: OnceLock<TempDir> = OnceLock::new();
    CACHE_DIR
        .get_or_init(|| {
            tempfile::tempdir().expect("Cannot create temporary directory for wasmer caches")
        })
        .path()
        .into()
}

/// Instantiate a module within a sandbox context
pub fn instantiate(
    version: Instantiate,
    context: &Backend,
    wasm: &[u8],
    guest_env: GuestEnvironment,
    supervisor_context: &mut dyn SupervisorContext,
) -> std::result::Result<SandboxInstance, InstantiationError> {
    #[cfg(feature = "gear-wasmer-cache")]
    let module = gear_wasmer_cache::get(context.store().borrow().engine(), wasm, cache_base_path())
        .inspect_err(|e| log::trace!("Failed to create module: {e}"))
        .map_err(|_| InstantiationError::ModuleDecoding)?;

    #[cfg(not(feature = "gear-wasmer-cache"))]
    let module = Module::new(&context.store().borrow(), wasm)
        .map_err(|_| InstantiationError::ModuleDecoding)?;

    let mut exports = wasmer::Exports::new();

    let func_env =
        wasmer::FunctionEnv::new(&mut context.store().borrow_mut(), Env { gas_global: None });

    for import in module.imports() {
        match import.ty() {
            // Nothing to do here
            wasmer::ExternType::Global(_)
            | wasmer::ExternType::Table(_)
            | wasmer::ExternType::Tag(_) => (),

            wasmer::ExternType::Memory(_) => {
                let memory = guest_env
                    .imports
                    .memory_by_name(import.module(), import.name())
                    .ok_or(InstantiationError::ModuleDecoding)?;

                let wasmer_memory_ref = memory.as_wasmer().expect(
                    "memory is created by wasmer; \
					exported by the same module and backend; \
					thus the operation can't fail; \
					qed",
                );

                // This is safe since we're only instantiating the module and populating
                // the export table, so no memory access can happen at this time.
                // All subsequent memory accesses should happen through the wrapper,
                // that enforces the memory access protocol.
                //
                // We take exclusive lock to ensure that we're the only one here,
                // since during instantiation phase the memory should only be created
                // and not yet accessed.
                let wasmer_memory = wasmer_memory_ref
                    .buffer
                    .try_borrow_mut()
                    .map_err(|_| InstantiationError::EnvironmentDefinitionCorrupted)?
                    .clone();

                exports.insert(import.name(), wasmer::Extern::Memory(wasmer_memory));
            }

            wasmer::ExternType::Function(func_ty) => {
                let guest_func_index = guest_env
                    .imports
                    .func_by_name(import.module(), import.name());

                let guest_func_index = if let Some(index) = guest_func_index {
                    index
                } else {
                    // Missing import (should we abort here?)
                    continue;
                };

                let supervisor_func_index = guest_env
                    .guest_to_supervisor_mapping
                    .func_by_guest_index(guest_func_index)
                    .ok_or(InstantiationError::ModuleDecoding)?;

                let function = match version {
                    Instantiate::Version1 => dispatch_function(
                        supervisor_func_index,
                        &mut context.store().borrow_mut(),
                        &func_env,
                        func_ty,
                    ),
                    Instantiate::Version2 => dispatch_function_v2(
                        supervisor_func_index,
                        &mut context.store().borrow_mut(),
                        &func_env,
                        func_ty,
                    ),
                };

                exports.insert(import.name(), wasmer::Extern::Function(function));
            }
        }
    }

    let mut import_object = wasmer::Imports::new();
    import_object.register_namespace("env", exports);

    let instance = SupervisorContextStore::using(supervisor_context, || {
        wasmer::Instance::new(&mut context.store().borrow_mut(), &module, &import_object).map_err(
            |error| {
                log::trace!("Failed to call wasmer::Instance::new: {error:?}");

                match error {
                    wasmer::InstantiationError::Link(_) => InstantiationError::Instantiation,
                    wasmer::InstantiationError::Start(_) => InstantiationError::StartTrapped,
                    wasmer::InstantiationError::CpuFeature(_) => InstantiationError::CpuFeature,
                    wasmer::InstantiationError::DifferentStores
                    | wasmer::InstantiationError::DifferentArchOS => {
                        InstantiationError::EnvironmentDefinitionCorrupted
                    }
                }
            },
        )
    })?;

    // Initialize function environment with gas global after instance creation.
    // NOTE: The gas global could still be `None`,
    // because it is not set for non-instrumented programs (used in tests and benchmarks).
    let gas_global = instance.exports.get_global(GLOBAL_NAME_GAS).ok().cloned();
    func_env
        .as_mut(&mut context.store().borrow_mut())
        .gas_global = gas_global;

    Ok(SandboxInstance {
        backend_instance: BackendInstanceBundle::Wasmer {
            instance,
            store: context.store().clone(),
        },
    })
}

fn dispatch_common(
    supervisor_func_index: SupervisorFuncIndex,
    supervisor_context: &mut dyn SupervisorContext,
    invoke_args_data: Vec<u8>,
) -> std::result::Result<Vec<u8>, RuntimeError> {
    // Move serialized arguments inside the memory, invoke dispatch thunk and
    // then free allocated memory.
    let invoke_args_len = invoke_args_data.len() as WordSize;
    let invoke_args_ptr = supervisor_context
        .allocate_memory(invoke_args_len)
        .map_err(|_| RuntimeError::new("Can't allocate memory in supervisor for the arguments"))?;

    let deallocate = |fe: &mut dyn SupervisorContext, ptr, fail_msg| {
        fe.deallocate_memory(ptr)
            .map_err(|_| RuntimeError::new(fail_msg))
    };

    if supervisor_context
        .write_memory(invoke_args_ptr, &invoke_args_data)
        .is_err()
    {
        deallocate(
            supervisor_context,
            invoke_args_ptr,
            "Failed deallocation after failed write of invoke arguments",
        )?;

        return Err(RuntimeError::new("Can't write invoke args into memory"));
    }

    // Perform the actual call
    let serialized_result = supervisor_context
        .invoke(invoke_args_ptr, invoke_args_len, supervisor_func_index)
        .map_err(|e| RuntimeError::new(e.to_string()));

    deallocate(
        supervisor_context,
        invoke_args_ptr,
        "Failed deallocation after invoke",
    )?;

    let serialized_result = serialized_result?;

    // TODO #3038
    // dispatch_thunk returns pointer to serialized arguments.
    // Unpack pointer and len of the serialized result data.
    let (serialized_result_val_ptr, serialized_result_val_len) = {
        // Cast to u64 to use zero-extension.
        let v = serialized_result as u64;
        let ptr = (v >> 32) as u32;
        let len = (v & 0xFFFFFFFF) as u32;
        (Pointer::new(ptr), len)
    };

    let serialized_result_val = supervisor_context
        .read_memory(serialized_result_val_ptr, serialized_result_val_len)
        .map_err(|_| RuntimeError::new("Can't read the serialized result from dispatch thunk"));

    deallocate(
        supervisor_context,
        serialized_result_val_ptr,
        "Can't deallocate memory for dispatch thunk's result",
    )?;

    serialized_result_val
}

fn into_wasmer_val(value: &Value) -> wasmer::Value {
    match value {
        Value::I32(val) => wasmer::Value::I32(*val),
        Value::I64(val) => wasmer::Value::I64(*val),
        Value::F32(val) => wasmer::Value::F32(f32::from_bits(*val)),
        Value::F64(val) => wasmer::Value::F64(f64::from_bits(*val)),
    }
}

fn into_wasmer_result(value: ReturnValue) -> Vec<wasmer::Value> {
    match value {
        ReturnValue::Value(v) => vec![into_wasmer_val(&v)],
        ReturnValue::Unit => vec![],
    }
}

fn into_value(value: &wasmer::Value) -> Option<Value> {
    match value {
        wasmer::Value::I32(val) => Some(Value::I32(*val)),
        wasmer::Value::I64(val) => Some(Value::I64(*val)),
        wasmer::Value::F32(val) => Some(Value::F32(f32::to_bits(*val))),
        wasmer::Value::F64(val) => Some(Value::F64(f64::to_bits(*val))),
        _ => None,
    }
}

fn dispatch_function(
    supervisor_func_index: SupervisorFuncIndex,
    store: &mut Store,
    func_env: &wasmer::FunctionEnv<Env>,
    func_ty: &wasmer::FunctionType,
) -> wasmer::Function {
    wasmer::Function::new_with_env(store, func_env, func_ty, move |mut env, params| {
        SupervisorContextStore::with(|supervisor_context| {
            let mut storemut = env.as_store_mut();

            // Creates a scope that allows the previously mutably borrowed StoreRefCell
            // to be borrowed mutably or immutably again higher up in the call stack.
            // Check doc-comments in `store_refcell` module for more details.
            let deserialized_result = store_refcell_ctx::with_borrow_scope(&mut storemut, || {
                // Serialize arguments into a byte vector.
                let invoke_args_data = params
                    .iter()
                    .map(|value| {
                        into_value(value).ok_or_else(|| {
                            RuntimeError::new(format!("Unsupported function argument: {value:?}"))
                        })
                    })
                    .collect::<std::result::Result<Vec<_>, _>>()?
                    .encode();

                let serialized_result_val =
                    dispatch_common(supervisor_func_index, supervisor_context, invoke_args_data)?;

                std::result::Result::<ReturnValue, HostError>::decode(
                    &mut serialized_result_val.as_slice(),
                )
                .map_err(|_| RuntimeError::new("Decoding Result<ReturnValue, HostError> failed!"))?
                .map_err(|_| RuntimeError::new("Supervisor function returned sandbox::HostError"))
            })
            .expect("store refcell ctx is set when invoking sandboxed functions; qed")
            .map_err(|_| RuntimeError::new("StoreRefCell borrow scope error"))??;

            Ok(into_wasmer_result(deserialized_result))
        })
        .expect("SandboxContextStore is set when invoking sandboxed functions; qed")
    })
}

fn dispatch_function_v2(
    supervisor_func_index: SupervisorFuncIndex,
    store: &mut Store,
    func_env: &wasmer::FunctionEnv<Env>,
    func_ty: &wasmer::FunctionType,
) -> wasmer::Function {
    wasmer::Function::new_with_env(store, func_env, func_ty, move |mut env, params| {
        SupervisorContextStore::with(|supervisor_context| {
            let (env, mut storemut) = env.data_and_store_mut();
            let gas_global = env
                .gas_global
                .as_ref()
                .ok_or_else(|| RuntimeError::new("Cannot get gas global from store environment"))?;
            let gas = gas_global.get(&mut storemut);

            // Creates a scope that allows the previously mutably borrowed StoreRefCell
            // to be borrowed mutably or immutably again higher up in the call stack.
            // Check doc-comments in `store_refcell` module for more details.
            let deserialized_result = store_refcell_ctx::with_borrow_scope(&mut storemut, || {
                // Serialize arguments into a byte vector.
                let invoke_args_data = [gas]
                    .iter()
                    .chain(params.iter())
                    .map(|value| {
                        into_value(value).ok_or_else(|| {
                            RuntimeError::new(format!("Unsupported function argument: {value:?}"))
                        })
                    })
                    .collect::<std::result::Result<Vec<_>, _>>()?
                    .encode();

                let serialized_result_val =
                    dispatch_common(supervisor_func_index, supervisor_context, invoke_args_data)?;

                std::result::Result::<WasmReturnValue, HostError>::decode(
                    &mut serialized_result_val.as_slice(),
                )
                .map_err(|_| {
                    RuntimeError::new("Decoding Result<WasmReturnValue, HostError> failed!")
                })?
                .map_err(|_| RuntimeError::new("Supervisor function returned sandbox::HostError"))
            })
            .expect("store refcell ctx is set when invoking sandboxed functions; qed")
            .map_err(|_| RuntimeError::new("StoreRefCell borrow scope error"))??;

            gas_global
                .set(&mut storemut, wasmer::Value::I64(deserialized_result.gas))
                .map_err(|_| RuntimeError::new("Cannot set gas global from store environment"))?;

            Ok(into_wasmer_result(deserialized_result.inner))
        })
        .expect("SandboxContextStore is set when invoking sandboxed functions; qed")
    })
}

/// Allocate new memory region
pub fn new_memory(
    store: Rc<StoreRefCell>,
    initial: u32,
    maximum: Option<u32>,
) -> crate::error::Result<Memory> {
    let ty = wasmer::MemoryType::new(initial, maximum, false);
    let memory = wasmer::Memory::new(&mut store.borrow_mut(), ty)
        .map_err(|_| Error::InvalidMemoryReference)?;

    Ok(Memory::Wasmer(MemoryWrapper::new(memory, store)))
}

/// In order to enforce memory access protocol to the backend memory
/// we wrap it with `RefCell` and encapsulate all memory operations.
#[derive(Debug, Clone)]
pub struct MemoryWrapper {
    buffer: Rc<RefCell<wasmer::Memory>>,
    store: Rc<StoreRefCell>,
}

impl MemoryWrapper {
    /// Take ownership of the memory region and return a wrapper object
    pub fn new(memory: wasmer::Memory, store: Rc<StoreRefCell>) -> Self {
        Self {
            buffer: Rc::new(RefCell::new(memory)),
            store,
        }
    }

    /// Returns linear memory of the wasm instance as a slice.
    ///
    /// # Safety
    ///
    /// Wasmer doesn't provide comprehensive documentation about the exact behavior of the data
    /// pointer. If a dynamic style heap is used the base pointer of the heap can change. Since
    /// growing, we cannot guarantee the lifetime of the returned slice reference.
    unsafe fn memory_as_slice<'m>(memory: &'m wasmer::Memory, store: &wasmer::Store) -> &'m [u8] {
        let memory_view = memory.view(store);
        let ptr = memory_view.data_ptr() as *const _;

        let len: usize = memory_view.data_size().try_into().expect(
            "maximum memory object size never exceeds pointer size on any architecture; \
			usize by design and definition is enough to store any memory object size \
			possible on current architecture; thus the conversion can not fail; qed",
        );

        if len == 0 {
            &[]
        } else {
            unsafe { core::slice::from_raw_parts(ptr, len) }
        }
    }

    /// Returns linear memory of the wasm instance as a slice.
    ///
    /// # Safety
    ///
    /// See `[memory_as_slice]`. In addition to those requirements, since a mutable reference is
    /// returned it must be ensured that only one mutable and no shared references to memory
    /// exists at the same time.
    unsafe fn memory_as_slice_mut<'m>(
        memory: &'m mut wasmer::Memory,
        store: &wasmer::Store,
    ) -> &'m mut [u8] {
        let memory_view = memory.view(store);
        let ptr = memory_view.data_ptr();

        let len: usize = memory_view.data_size().try_into().expect(
            "maximum memory object size never exceeds pointer size on any architecture; \
			usize by design and definition is enough to store any memory object size \
			possible on current architecture; thus the conversion can not fail; qed",
        );

        if len == 0 {
            &mut []
        } else {
            unsafe { core::slice::from_raw_parts_mut(ptr, len) }
        }
    }
}

impl MemoryTransfer for MemoryWrapper {
    fn read(&self, source_addr: Pointer<u8>, size: usize) -> Result<Vec<u8>> {
        let memory = self.buffer.borrow();

        let data_size: usize = memory
            .view(&*self.store.borrow())
            .data_size()
            .try_into()
            .expect(
                "maximum memory object size never exceeds pointer size on any architecture; \
			usize by design and definition is enough to store any memory object size \
			possible on current architecture; thus the conversion can not fail; qed",
            );

        let range = util::checked_range(source_addr.into(), size, data_size)
            .ok_or_else(|| Error::Other("memory read is out of bounds".into()))?;

        let mut buffer = vec![0; range.len()];
        self.read_into(source_addr, &mut buffer)?;

        Ok(buffer)
    }

    fn read_into(&self, source_addr: Pointer<u8>, destination: &mut [u8]) -> Result<()> {
        unsafe {
            let memory = self.buffer.borrow();

            // This should be safe since we don't grow up memory while caching this reference
            // and we give up the reference before returning from this function.
            let source = Self::memory_as_slice(&memory, &self.store.borrow());

            let range = util::checked_range(source_addr.into(), destination.len(), source.len())
                .ok_or_else(|| Error::Other("memory read is out of bounds".into()))?;

            destination.copy_from_slice(&source[range]);
            Ok(())
        }
    }

    fn write_from(&self, dest_addr: Pointer<u8>, source: &[u8]) -> Result<()> {
        unsafe {
            let memory = &mut self.buffer.borrow_mut();

            // This should be safe since we don't grow up memory while caching this reference
            // and we give up the reference before returning from this function.
            let destination = Self::memory_as_slice_mut(memory, &self.store.borrow());

            let range = util::checked_range(dest_addr.into(), source.len(), destination.len())
                .ok_or_else(|| Error::Other("memory write is out of bounds".into()))?;

            destination[range].copy_from_slice(source);
            Ok(())
        }
    }

    fn memory_grow(&mut self, pages: u32) -> Result<u32> {
        let memory = &self.buffer.borrow();
        memory
            .grow(&mut self.store.borrow_mut(), pages)
            .map_err(|e| {
                Error::Sandbox(format!(
                    "Cannot grow memory in wasmer sandbox executor: {e}",
                ))
            })
            .map(|p| p.0)
    }

    fn memory_size(&mut self) -> u32 {
        let store = self.store.borrow();
        let memory = &mut self.buffer.borrow().view(&store);
        memory.size().0
    }

    fn get_buff(&mut self) -> *mut u8 {
        self.buffer.borrow().view(&*self.store.borrow()).data_ptr()
    }
}

/// Get global value by name
pub fn get_global(
    instance: &wasmer::Instance,
    store: &mut wasmer::Store,
    name: &str,
) -> Option<Value> {
    let global = instance.exports.get_global(name).ok()?;

    into_value(&global.get(store))
}

/// Set global value by name
pub fn set_global(
    instance: &wasmer::Instance,
    mut store: &mut wasmer::Store,
    name: &str,
    value: Value,
) -> core::result::Result<Option<()>, crate::error::Error> {
    let global = match instance.exports.get_global(name) {
        Ok(g) => g,
        Err(_) => return Ok(None),
    };

    let value = into_wasmer_val(&value);
    global
        .set(&mut store, value)
        .map(|_| Some(()))
        .map_err(|e| crate::error::Error::Sandbox(e.message()))
}
