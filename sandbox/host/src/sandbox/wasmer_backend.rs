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

use std::{cell::RefCell, collections::HashMap, rc::Rc};

use sandbox_wasmer::{Exportable, RuntimeError};
use sandbox_wasmer_types::TrapCode;

use codec::{Decode, Encode};
use gear_sandbox_env::{HostError, Instantiate, WasmReturnValue, GLOBAL_NAME_GAS};
use sp_wasm_interface_common::{util, Pointer, ReturnValue, Value, WordSize};

use crate::{
    error::{Error, Result},
    sandbox::{
        BackendInstance, GuestEnvironment, InstantiationError, Memory, SandboxContext,
        SandboxInstance, SupervisorFuncIndex,
    },
    util::MemoryTransfer,
};

environmental::environmental!(SandboxContextStore: trait SandboxContext);

#[cfg(feature = "wasmer-cache")]
enum CachedModuleErr {
    FileSystemErr,
    ModuleLoadErr(FileSystemCache, Hash),
}

#[cfg(feature = "wasmer-cache")]
use {
    sandbox_wasmer::Module,
    std::sync::OnceLock,
    tempfile::TempDir,
    wasmer_cache::{Cache, FileSystemCache, Hash},
    CachedModuleErr::*,
};

#[cfg(feature = "wasmer-cache")]
static CACHE_DIR: OnceLock<TempDir> = OnceLock::new();

/// Wasmer specific context
pub struct Backend {
    store: sandbox_wasmer::Store,
}

impl Backend {
    pub fn new() -> Self {
        let compiler = sandbox_wasmer::Singlepass::default();
        Backend {
            store: sandbox_wasmer::Store::new(&sandbox_wasmer::Universal::new(compiler).engine()),
        }
    }
}

#[derive(Default)]
pub struct Env {
    gas: Option<sandbox_wasmer::Global>,
}

// WARNING: intentionally to avoid cyclic refs
impl Clone for Env {
    fn clone(&self) -> Self {
        Self {
            gas: self.gas.clone().map(|mut global| {
                global.into_weak_instance_ref();

                global
            }),
        }
    }
}

impl sandbox_wasmer::WasmerEnv for Env {
    fn init_with_instance(
        &mut self,
        instance: &sandbox_wasmer::Instance,
    ) -> std::result::Result<(), sandbox_wasmer::HostEnvInitError> {
        let gas: sandbox_wasmer::Global =
            instance.exports.get_with_generics_weak(GLOBAL_NAME_GAS)?;
        self.gas = Some(gas);

        Ok(())
    }
}

/// Invoke a function within a sandboxed module
pub fn invoke(
    instance: &sandbox_wasmer::Instance,
    export_name: &str,
    args: &[Value],
    sandbox_context: &mut dyn SandboxContext,
) -> std::result::Result<Option<Value>, Error> {
    let function = instance
        .exports
        .get_function(export_name)
        .map_err(|error| Error::Sandbox(error.to_string()))?;

    let args: Vec<sandbox_wasmer::Val> = args.iter().map(into_wasmer_val).collect();

    let wasmer_result = SandboxContextStore::using(sandbox_context, || {
        function
            .call(&args)
            .map_err(|error| {
                if error.clone().to_trap() == Some(TrapCode::StackOverflow) {
                    // Panic stops process queue execution in that case.
                    // This allows to avoid error lead to consensus failures, that must be handled
                    // in node binaries forever. If this panic occur, then we must increase stack memory size,
                    // or tune stack limit injection.
                    // see also https://github.com/wasmerio/wasmer/issues/4181
                    unreachable!("Suppose that this can not happen, because we have a stack limit instrumentation in programs");
                }
                Error::Sandbox(error.to_string())
            })
    })?;

    match wasmer_result.as_ref() {
        [] => Ok(None),

        [wasm_value] => match into_value(wasm_value) {
            None => Err(Error::Sandbox(format!(
                "Unsupported return value: {:?}",
                wasm_value,
            ))),
            Some(v) => Ok(Some(v)),
        },

        _ => Err(Error::Sandbox(
            "multiple return types are not supported yet".into(),
        )),
    }
}

#[cfg(feature = "wasmer-cache")]
fn get_cached_module(
    wasm: &[u8],
    store: &sandbox_wasmer::Store,
) -> core::result::Result<Module, CachedModuleErr> {
    let cache_path = CACHE_DIR
        .get_or_init(|| {
            tempfile::tempdir().expect("Cannot create temporary directory for wasmer caches")
        })
        .path();
    log::trace!("Wasmer sandbox cache dir is: {cache_path:?}");

    let fs_cache = FileSystemCache::new(cache_path).map_err(|_| FileSystemErr)?;
    let code_hash = Hash::generate(wasm);
    unsafe {
        fs_cache
            .load(store, code_hash)
            .map_err(|_| ModuleLoadErr(fs_cache, code_hash))
    }
}

#[cfg(feature = "wasmer-cache")]
fn try_to_store_module_in_cache(mut fs_cache: FileSystemCache, code_hash: Hash, module: &Module) {
    let res = fs_cache.store(code_hash, &module.clone());
    log::trace!("Store module cache with result: {:?}", res);
}

/// Instantiate a module within a sandbox context
pub fn instantiate(
    version: Instantiate,
    context: &Backend,
    wasm: &[u8],
    guest_env: GuestEnvironment,
    sandbox_context: &mut dyn SandboxContext,
) -> std::result::Result<SandboxInstance, InstantiationError> {
    #[cfg(feature = "wasmer-cache")]
    let module = match get_cached_module(wasm, &context.store) {
        Ok(module) => {
            log::trace!("Found cached module for current program");
            module
        }
        Err(err) => {
            log::trace!("Cache for program has not been found, so compile it now");
            let module = sandbox_wasmer::Module::new(&context.store, wasm)
                .map_err(|_| InstantiationError::ModuleDecoding)?;
            match err {
                CachedModuleErr::FileSystemErr => log::error!("Cannot open fs cache"),
                CachedModuleErr::ModuleLoadErr(fs_cache, code_hash) => {
                    try_to_store_module_in_cache(fs_cache, code_hash, &module)
                }
            };
            module
        }
    };

    #[cfg(not(feature = "wasmer-cache"))]
    let module = sandbox_wasmer::Module::new(&context.store, wasm)
        .map_err(|_| InstantiationError::ModuleDecoding)?;

    type Exports = HashMap<String, sandbox_wasmer::Exports>;
    let mut exports_map = Exports::new();

    for import in module.imports() {
        match import.ty() {
            // Nothing to do here
            sandbox_wasmer::ExternType::Global(_) | sandbox_wasmer::ExternType::Table(_) => (),

            sandbox_wasmer::ExternType::Memory(_) => {
                let exports = exports_map.entry(import.module().to_string()).or_default();

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

                exports.insert(import.name(), sandbox_wasmer::Extern::Memory(wasmer_memory));
            }

            sandbox_wasmer::ExternType::Function(func_ty) => {
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
                    Instantiate::Version1 => {
                        dispatch_function(supervisor_func_index, &context.store, func_ty)
                    }
                    Instantiate::Version2 => {
                        dispatch_function_v2(supervisor_func_index, &context.store, func_ty)
                    }
                };

                let exports = exports_map.entry(import.module().to_string()).or_default();

                exports.insert(import.name(), sandbox_wasmer::Extern::Function(function));
            }
        }
    }

    let mut import_object = sandbox_wasmer::ImportObject::new();
    for (module_name, exports) in exports_map.into_iter() {
        import_object.register(module_name, exports);
    }

    let instance = SandboxContextStore::using(sandbox_context, || {
        sandbox_wasmer::Instance::new(&module, &import_object).map_err(|error| {
            log::trace!("Failed to call sandbox_wasmer::Instance::new: {error:?}");

            match error {
                sandbox_wasmer::InstantiationError::Link(_) => InstantiationError::Instantiation,
                sandbox_wasmer::InstantiationError::Start(_) => InstantiationError::StartTrapped,
                sandbox_wasmer::InstantiationError::HostEnvInitialization(_) => {
                    InstantiationError::EnvironmentDefinitionCorrupted
                }
                sandbox_wasmer::InstantiationError::CpuFeature(_) => InstantiationError::CpuFeature,
            }
        })
    })?;

    Ok(SandboxInstance {
        backend_instance: BackendInstance::Wasmer(instance),
        guest_to_supervisor_mapping: guest_env.guest_to_supervisor_mapping,
    })
}

fn dispatch_common(
    supervisor_func_index: SupervisorFuncIndex,
    sandbox_context: &mut dyn SandboxContext,
    invoke_args_data: Vec<u8>,
) -> std::result::Result<Vec<u8>, RuntimeError> {
    // Move serialized arguments inside the memory, invoke dispatch thunk and
    // then free allocated memory.
    let invoke_args_len = invoke_args_data.len() as WordSize;
    let invoke_args_ptr = sandbox_context
        .allocate_memory(invoke_args_len)
        .map_err(|_| RuntimeError::new("Can't allocate memory in supervisor for the arguments"))?;

    let deallocate = |fe: &mut dyn SandboxContext, ptr, fail_msg| {
        fe.deallocate_memory(ptr)
            .map_err(|_| RuntimeError::new(fail_msg))
    };

    if sandbox_context
        .write_memory(invoke_args_ptr, &invoke_args_data)
        .is_err()
    {
        deallocate(
            sandbox_context,
            invoke_args_ptr,
            "Failed deallocation after failed write of invoke arguments",
        )?;

        return Err(RuntimeError::new("Can't write invoke args into memory"));
    }

    // Perform the actual call
    let serialized_result = sandbox_context
        .invoke(invoke_args_ptr, invoke_args_len, supervisor_func_index)
        .map_err(|e| RuntimeError::new(e.to_string()));

    deallocate(
        sandbox_context,
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

    let serialized_result_val = sandbox_context
        .read_memory(serialized_result_val_ptr, serialized_result_val_len)
        .map_err(|_| RuntimeError::new("Can't read the serialized result from dispatch thunk"));

    deallocate(
        sandbox_context,
        serialized_result_val_ptr,
        "Can't deallocate memory for dispatch thunk's result",
    )?;

    serialized_result_val
}

fn into_wasmer_val(value: &Value) -> sandbox_wasmer::Val {
    match value {
        Value::I32(val) => sandbox_wasmer::Val::I32(*val),
        Value::I64(val) => sandbox_wasmer::Val::I64(*val),
        Value::F32(val) => sandbox_wasmer::Val::F32(f32::from_bits(*val)),
        Value::F64(val) => sandbox_wasmer::Val::F64(f64::from_bits(*val)),
    }
}

fn into_wasmer_result(value: ReturnValue) -> Vec<sandbox_wasmer::Val> {
    match value {
        ReturnValue::Value(v) => vec![into_wasmer_val(&v)],
        ReturnValue::Unit => vec![],
    }
}

fn into_value(value: &sandbox_wasmer::Val) -> Option<Value> {
    match value {
        sandbox_wasmer::Val::I32(val) => Some(Value::I32(*val)),
        sandbox_wasmer::Val::I64(val) => Some(Value::I64(*val)),
        sandbox_wasmer::Val::F32(val) => Some(Value::F32(f32::to_bits(*val))),
        sandbox_wasmer::Val::F64(val) => Some(Value::F64(f64::to_bits(*val))),
        _ => None,
    }
}

fn dispatch_function(
    supervisor_func_index: SupervisorFuncIndex,
    store: &sandbox_wasmer::Store,
    func_ty: &sandbox_wasmer::FunctionType,
) -> sandbox_wasmer::Function {
    sandbox_wasmer::Function::new(store, func_ty, move |params| {
        SandboxContextStore::with(|sandbox_context| {
            // Serialize arguments into a byte vector.
            let invoke_args_data = params
                .iter()
                .map(|value| {
                    into_value(value).ok_or_else(|| {
                        RuntimeError::new(format!("Unsupported function argument: {:?}", value))
                    })
                })
                .collect::<std::result::Result<Vec<_>, _>>()?
                .encode();

            let serialized_result_val =
                dispatch_common(supervisor_func_index, sandbox_context, invoke_args_data)?;

            let deserialized_result = std::result::Result::<ReturnValue, HostError>::decode(
                &mut serialized_result_val.as_slice(),
            )
            .map_err(|_| RuntimeError::new("Decoding Result<ReturnValue, HostError> failed!"))?
            .map_err(|_| RuntimeError::new("Supervisor function returned sandbox::HostError"))?;

            Ok(into_wasmer_result(deserialized_result))
        })
        .expect("SandboxContextStore is set when invoking sandboxed functions; qed")
    })
}

fn dispatch_function_v2(
    supervisor_func_index: SupervisorFuncIndex,
    store: &sandbox_wasmer::Store,
    func_ty: &sandbox_wasmer::FunctionType,
) -> sandbox_wasmer::Function {
    sandbox_wasmer::Function::new_with_env(store, func_ty, Env::default(), move |env, params| {
        SandboxContextStore::with(|sandbox_context| {
            let gas = env
                .gas
                .as_ref()
                .ok_or_else(|| RuntimeError::new("gas global should be set"))?;

            // Serialize arguments into a byte vector.
            let invoke_args_data = [gas.get()]
                .iter()
                .chain(params.iter())
                .map(|value| {
                    into_value(value).ok_or_else(|| {
                        RuntimeError::new(format!("Unsupported function argument: {:?}", value))
                    })
                })
                .collect::<std::result::Result<Vec<_>, _>>()?
                .encode();

            let serialized_result_val =
                dispatch_common(supervisor_func_index, sandbox_context, invoke_args_data)?;

            let deserialized_result = std::result::Result::<WasmReturnValue, HostError>::decode(
                &mut serialized_result_val.as_slice(),
            )
            .map_err(|_| RuntimeError::new("Decoding Result<WasmReturnValue, HostError> failed!"))?
            .map_err(|_| RuntimeError::new("Supervisor function returned sandbox::HostError"))?;

            gas.set(sandbox_wasmer::Val::I64(deserialized_result.gas))?;

            Ok(into_wasmer_result(deserialized_result.inner))
        })
        .expect("SandboxContextStore is set when invoking sandboxed functions; qed")
    })
}

/// Allocate new memory region
pub fn new_memory(
    context: &Backend,
    initial: u32,
    maximum: Option<u32>,
) -> crate::error::Result<Memory> {
    let ty = sandbox_wasmer::MemoryType::new(initial, maximum, false);
    let memory = Memory::Wasmer(MemoryWrapper::new(
        sandbox_wasmer::Memory::new(&context.store, ty)
            .map_err(|_| Error::InvalidMemoryReference)?,
    ));

    Ok(memory)
}

/// In order to enforce memory access protocol to the backend memory
/// we wrap it with `RefCell` and encapsulate all memory operations.
#[derive(Debug, Clone)]
pub struct MemoryWrapper {
    buffer: Rc<RefCell<sandbox_wasmer::Memory>>,
}

impl MemoryWrapper {
    /// Take ownership of the memory region and return a wrapper object
    pub fn new(memory: sandbox_wasmer::Memory) -> Self {
        Self {
            buffer: Rc::new(RefCell::new(memory)),
        }
    }

    /// Returns linear memory of the wasm instance as a slice.
    ///
    /// # Safety
    ///
    /// Wasmer doesn't provide comprehensive documentation about the exact behavior of the data
    /// pointer. If a dynamic style heap is used the base pointer of the heap can change. Since
    /// growing, we cannot guarantee the lifetime of the returned slice reference.
    unsafe fn memory_as_slice(memory: &sandbox_wasmer::Memory) -> &[u8] {
        let ptr = memory.data_ptr() as *const _;

        let len: usize = memory.data_size().try_into().expect(
            "maximum memory object size never exceeds pointer size on any architecture; \
			usize by design and definition is enough to store any memory object size \
			possible on current architecture; thus the conversion can not fail; qed",
        );

        if len == 0 {
            &[]
        } else {
            core::slice::from_raw_parts(ptr, len)
        }
    }

    /// Returns linear memory of the wasm instance as a slice.
    ///
    /// # Safety
    ///
    /// See `[memory_as_slice]`. In addition to those requirements, since a mutable reference is
    /// returned it must be ensured that only one mutable and no shared references to memory
    /// exists at the same time.
    #[allow(clippy::needless_pass_by_ref_mut)]
    unsafe fn memory_as_slice_mut(memory: &mut sandbox_wasmer::Memory) -> &mut [u8] {
        let ptr = memory.data_ptr();

        let len: usize = memory.data_size().try_into().expect(
            "maximum memory object size never exceeds pointer size on any architecture; \
			usize by design and definition is enough to store any memory object size \
			possible on current architecture; thus the conversion can not fail; qed",
        );

        if len == 0 {
            &mut []
        } else {
            core::slice::from_raw_parts_mut(ptr, len)
        }
    }
}

impl MemoryTransfer for MemoryWrapper {
    fn read(&self, source_addr: Pointer<u8>, size: usize) -> Result<Vec<u8>> {
        let memory = self.buffer.borrow();

        let data_size: usize = memory.data_size().try_into().expect(
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
            let source = Self::memory_as_slice(&memory);

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
            let destination = Self::memory_as_slice_mut(memory);

            let range = util::checked_range(dest_addr.into(), source.len(), destination.len())
                .ok_or_else(|| Error::Other("memory write is out of bounds".into()))?;

            destination[range].copy_from_slice(source);
            Ok(())
        }
    }

    fn memory_grow(&mut self, pages: u32) -> Result<u32> {
        let memory = &mut self.buffer.borrow_mut();
        memory
            .grow(pages)
            .map_err(|e| {
                Error::Sandbox(format!(
                    "Cannot grow memory in wasmer sandbox executor: {}",
                    e
                ))
            })
            .map(|p| p.0)
    }

    fn memory_size(&mut self) -> u32 {
        let memory = &mut self.buffer.borrow_mut();
        memory.size().0
    }

    fn get_buff(&mut self) -> *mut u8 {
        self.buffer.borrow_mut().data_ptr()
    }
}

/// Get global value by name
pub fn get_global(instance: &sandbox_wasmer::Instance, name: &str) -> Option<Value> {
    let global = instance.exports.get_global(name).ok()?;

    into_value(&global.get())
}

/// Set global value by name
pub fn set_global(
    instance: &sandbox_wasmer::Instance,
    name: &str,
    value: Value,
) -> core::result::Result<Option<()>, crate::error::Error> {
    let global = match instance.exports.get_global(name) {
        Ok(g) => g,
        Err(_) => return Ok(None),
    };

    let value = into_wasmer_val(&value);
    global
        .set(value)
        .map(|_| Some(()))
        .map_err(|e| crate::error::Error::Sandbox(e.message()))
}
