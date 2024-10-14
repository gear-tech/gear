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

//! Wasmi specific impls for sandbox

use std::{
    rc::{Rc, Weak},
    slice,
};

use codec::{Decode, Encode};
use gear_sandbox_env::{HostError, Instantiate, WasmReturnValue, GLOBAL_NAME_GAS};
use region::{Allocation, Protection};
use sandbox_wasmi::{
    core::UntypedVal, AsContext, AsContextMut, Config, Engine, ExternType, Linker, MemoryType,
    Module, StackLimits, Val,
};

use sp_wasm_interface_common::{Pointer, ReturnValue, Value, WordSize};

use crate::{
    error::{self, Error},
    sandbox::{
        BackendInstanceBundle, GuestEnvironment, InstantiationError, Memory, SandboxInstance,
        SupervisorContext,
    },
    store_refcell,
    util::MemoryTransfer,
};

use super::SupervisorFuncIndex;

type Store = sandbox_wasmi::Store<Option<FuncEnv>>;
pub type StoreRefCell = store_refcell::StoreRefCell<Store>;

environmental::environmental!(SupervisorContextStore: trait SupervisorContext);

pub struct FuncEnv {
    store: Weak<StoreRefCell>,
    gas_global: sandbox_wasmi::Global,
}

impl FuncEnv {
    pub fn new(store: Weak<StoreRefCell>, gas_global: sandbox_wasmi::Global) -> Self {
        Self { store, gas_global }
    }
}

/// Construct trap error from specified message
fn host_trap(msg: impl Into<error::Error>) -> sandbox_wasmi::Error {
    sandbox_wasmi::Error::host(msg.into())
}

fn into_wasmi_val(value: Value) -> sandbox_wasmi::Val {
    match value {
        Value::I32(val) => sandbox_wasmi::Val::I32(val),
        Value::I64(val) => sandbox_wasmi::Val::I64(val),
        Value::F32(val) => sandbox_wasmi::Val::F32(val.into()),
        Value::F64(val) => sandbox_wasmi::Val::F64(val.into()),
    }
}

fn into_wasmi_result(value: ReturnValue) -> Vec<sandbox_wasmi::Val> {
    match value {
        ReturnValue::Value(v) => vec![into_wasmi_val(v)],
        ReturnValue::Unit => vec![],
    }
}

fn into_value(value: &sandbox_wasmi::Val) -> Option<Value> {
    match value {
        sandbox_wasmi::Val::I32(val) => Some(Value::I32(*val)),
        sandbox_wasmi::Val::I64(val) => Some(Value::I64(*val)),
        sandbox_wasmi::Val::F32(val) => Some(Value::F32((*val).into())),
        sandbox_wasmi::Val::F64(val) => Some(Value::F64((*val).into())),
        _ => None,
    }
}

/// Wasmi specific context
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
        let register_len = size_of::<UntypedVal>();

        const DEFAULT_MIN_VALUE_STACK_HEIGHT: usize = 1024;
        const DEFAULT_MAX_VALUE_STACK_HEIGHT: usize = 1024 * DEFAULT_MIN_VALUE_STACK_HEIGHT;
        const DEFAULT_MAX_RECURSION_DEPTH: usize = 16384;

        let mut config = Config::default();
        config.set_stack_limits(
            StackLimits::new(
                DEFAULT_MIN_VALUE_STACK_HEIGHT / register_len,
                DEFAULT_MAX_VALUE_STACK_HEIGHT / register_len,
                DEFAULT_MAX_RECURSION_DEPTH,
            )
            .expect("infallible"),
        );

        let engine = Engine::new(&config);
        let store = Store::new(&engine, None);
        Backend {
            store: Rc::new(StoreRefCell::new(store)),
        }
    }

    pub fn store(&self) -> &Rc<StoreRefCell> {
        &self.store
    }
}

/// Allocate new memory region
pub fn new_memory(
    store: Rc<StoreRefCell>,
    initial: u32,
    maximum: Option<u32>,
) -> crate::error::Result<(Memory, Allocation)> {
    let ty =
        MemoryType::new(initial, maximum).map_err(|error| Error::Sandbox(error.to_string()))?;
    let mut alloc = region::alloc(u32::MAX as usize, Protection::READ_WRITE)
        .unwrap_or_else(|err| unreachable!("Failed to allocate memory: {err}"));
    // # Safety:
    //
    // `wasmi::Memory::new_static()` requires static lifetime so we convert our buffer to it
    // but actual lifetime of the buffer is lifetime of `Store<T>` itself,
    // so memory will be deallocated when `Store<T>` is dropped.
    let raw = unsafe { slice::from_raw_parts_mut::<'static, u8>(alloc.as_mut_ptr(), alloc.len()) };
    let memory = sandbox_wasmi::Memory::new_static(&mut *store.borrow_mut(), ty, raw)
        .map_err(|error| Error::Sandbox(error.to_string()))?;

    Ok((Memory::Wasmi(MemoryWrapper::new(memory, store)), alloc))
}

/// Wasmi provides direct access to its memory using slices.
///
/// This wrapper limits the scope where the slice can be taken to
#[derive(Clone)]
pub struct MemoryWrapper {
    memory: sandbox_wasmi::Memory,
    store: Rc<StoreRefCell>,
}

impl std::fmt::Debug for MemoryWrapper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MemoryWrapper")
            .field("memory", &self.memory)
            .finish()
    }
}

impl MemoryWrapper {
    /// Take ownership of the memory region and return a wrapper object
    fn new(memory: sandbox_wasmi::Memory, store: Rc<StoreRefCell>) -> Self {
        Self { memory, store }
    }
}

impl MemoryTransfer for MemoryWrapper {
    fn read(&self, source_addr: Pointer<u8>, size: usize) -> error::Result<Vec<u8>> {
        let mut buffer = Vec::with_capacity(size);
        let spare_cap = buffer.spare_capacity_mut().len();
        // # Safety:
        // `Vec::set_len` is safe to call because we have just allocated enough space and never read from it.
        unsafe { buffer.set_len(spare_cap) };

        let ctx = self.store.borrow();
        self.memory
            .read(&*ctx, source_addr.into(), &mut buffer)
            .map_err(|_| error::Error::Other("memory read is out of bounds".into()))?;

        Ok(buffer)
    }

    fn read_into(&self, source_addr: Pointer<u8>, destination: &mut [u8]) -> error::Result<()> {
        let ctx = self.store.borrow();
        self.memory
            .read(&*ctx, source_addr.into(), destination)
            .map_err(|_| error::Error::Other("memory read is out of bounds".into()))?;

        Ok(())
    }

    fn write_from(&self, dest_addr: Pointer<u8>, source: &[u8]) -> error::Result<()> {
        let mut ctx = self.store.borrow_mut();
        self.memory
            .write(&mut *ctx, dest_addr.into(), source)
            .map_err(|_| error::Error::Other("memory write is out of bounds".into()))?;

        Ok(())
    }

    fn memory_grow(&mut self, pages: u32) -> error::Result<u32> {
        let mut ctx = self.store.borrow_mut();
        self.memory.grow(&mut *ctx, pages).map_err(|e| {
            Error::Sandbox(format!("Cannot grow memory in wasmi sandbox executor: {e}",))
        })
    }

    fn memory_size(&mut self) -> u32 {
        let ctx = self.store.borrow();
        self.memory.size(&*ctx)
    }

    fn get_buff(&mut self) -> *mut u8 {
        let ctx = self.store.borrow_mut();
        self.memory.data_ptr(&*ctx)
    }
}

/// Get global value by name
pub fn get_global(instance: &sandbox_wasmi::Instance, store: &Store, name: &str) -> Option<Value> {
    into_value(&instance.get_global(store, name)?.get(store))
}

/// Set global value by name
pub fn set_global(
    instance: &sandbox_wasmi::Instance,
    store: &mut Store,
    name: &str,
    value: Value,
) -> std::result::Result<Option<()>, error::Error> {
    let global = match instance.get_global(&*store, name) {
        Some(e) => e,
        None => return Ok(None),
    };

    global
        .set(store, into_wasmi_val(value))
        .map(Some)
        .map_err(|e| Error::Sandbox(e.to_string()))
}

/// Instantiate a module within a sandbox context
pub fn instantiate(
    version: Instantiate,
    context: &Backend,
    wasm: &[u8],
    guest_env: GuestEnvironment,
    _supervisor_context: &mut dyn SupervisorContext,
) -> std::result::Result<SandboxInstance, InstantiationError> {
    let mut store = context.store().borrow_mut();

    let module =
        Module::new(store.engine(), wasm).map_err(|_| InstantiationError::ModuleDecoding)?;
    let mut linker = Linker::new(store.engine());

    for import in module.imports() {
        let module = import.module().to_string();
        let name = import.name().to_string();

        match import.ty() {
            ExternType::Global(_) | ExternType::Table(_) => {}
            ExternType::Memory(_mem_ty) => {
                let memory = guest_env
                    .imports
                    .memory_by_name(&module, &name)
                    .ok_or(InstantiationError::ModuleDecoding)?;

                let wasmi_memory = memory.as_wasmi().expect(
                    "memory is created by wasmi; \
                    exported by the same module and backend; \
                    thus the operation can't fail; \
                    qed",
                );

                linker
                    .define(&module, &name, wasmi_memory.memory)
                    .map_err(|_| InstantiationError::EnvironmentDefinitionCorrupted)?;
            }
            ExternType::Func(func_ty) => {
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
                        dispatch_function(supervisor_func_index, &mut store, func_ty)
                    }
                    Instantiate::Version2 => {
                        dispatch_function_v2(supervisor_func_index, &mut store, func_ty)
                    }
                };

                linker
                    .define(&module, &name, function)
                    .map_err(|_| InstantiationError::ModuleDecoding)?;
            }
        }
    }

    let instance_pre = linker.instantiate(&mut *store, &module).map_err(|error| {
        log::trace!("Failed to call wasmi instantiate: {error:?}");
        InstantiationError::Instantiation
    })?;
    let instance = instance_pre.start(&mut *store).map_err(|error| {
        log::trace!("Failed to call wasmi start: {error:?}");
        InstantiationError::StartTrapped
    })?;

    Ok(SandboxInstance {
        backend_instance: BackendInstanceBundle::Wasmi {
            instance,
            store: context.store().clone(),
        },
    })
}

fn dispatch_function(
    supervisor_func_index: SupervisorFuncIndex,
    store: &mut Store,
    func_ty: &sandbox_wasmi::FuncType,
) -> sandbox_wasmi::Func {
    sandbox_wasmi::Func::new(
        store,
        func_ty.clone(),
        move |_caller, params, results| -> Result<(), sandbox_wasmi::Error> {
            SupervisorContextStore::with(|supervisor_context| {
                let invoke_args_data = params
                    .iter()
                    .map(|value| {
                        into_value(value).ok_or_else(|| {
                            host_trap(format!("Unsupported function argument: {:?}", value))
                        })
                    })
                    .collect::<std::result::Result<Vec<_>, _>>()?
                    .encode();

                let serialized_result_val =
                    dispatch_common(supervisor_func_index, supervisor_context, invoke_args_data)?;

                let deserialized_result = std::result::Result::<ReturnValue, HostError>::decode(
                    &mut serialized_result_val.as_slice(),
                )
                .map_err(|_| host_trap("Decoding Result<ReturnValue, HostError> failed!"))?
                .map_err(|_| host_trap("Supervisor function returned sandbox::HostError"))?;

                for (idx, result_val) in into_wasmi_result(deserialized_result)
                    .into_iter()
                    .enumerate()
                {
                    results[idx] = result_val;
                }

                Ok(())
            })
            .expect("SupervisorContextStore is set when invoking sandboxed functions; qed")
        },
    )
}

fn dispatch_function_v2(
    supervisor_func_index: SupervisorFuncIndex,
    store: &mut Store,
    func_ty: &sandbox_wasmi::FuncType,
) -> sandbox_wasmi::Func {
    sandbox_wasmi::Func::new(
        store,
        func_ty.clone(),
        move |mut caller, params, results| -> Result<(), sandbox_wasmi::Error> {
            SupervisorContextStore::with(|supervisor_context| {
                let func_env = caller.data().as_ref().expect("func env should be set");
                let store_ref_cell = func_env.store.upgrade().expect("store should be alive");
                let gas_global = func_env.gas_global;

                let gas = gas_global.get(caller.as_context());
                let store_ctx_mut = caller.as_context_mut();

                let deserialized_result = store_ref_cell
                    .borrow_scope(store_ctx_mut, move || {
                        let invoke_args_data = [gas]
                            .iter()
                            .chain(params.iter())
                            .map(|value| {
                                into_value(value).ok_or_else(|| {
                                    host_trap(format!("Unsupported function argument: {:?}", value))
                                })
                            })
                            .collect::<std::result::Result<Vec<_>, _>>()?
                            .encode();

                        let serialized_result_val = dispatch_common(
                            supervisor_func_index,
                            supervisor_context,
                            invoke_args_data,
                        )?;

                        std::result::Result::<WasmReturnValue, HostError>::decode(
                            &mut serialized_result_val.as_slice(),
                        )
                        .map_err(|_| host_trap("Decoding Result<ReturnValue, HostError> failed!"))?
                        .map_err(|_| host_trap("Supervisor function returned sandbox::HostError"))
                    })
                    .map_err(|_| host_trap("StoreRefCell borrow scope error"))??;

                for (idx, result_val) in into_wasmi_result(deserialized_result.inner)
                    .into_iter()
                    .enumerate()
                {
                    results[idx] = result_val;
                }

                gas_global
                    .set(caller, Val::I64(deserialized_result.gas))
                    .map_err(|e| host_trap(format!("Failed to set gas global: {:?}", e)))?;

                Ok(())
            })
            .expect("SandboxContextStore is set when invoking sandboxed functions; qed")
        },
    )
}

fn dispatch_common(
    supervisor_func_index: SupervisorFuncIndex,
    supervisor_context: &mut dyn SupervisorContext,
    invoke_args_data: Vec<u8>,
) -> std::result::Result<Vec<u8>, sandbox_wasmi::Error> {
    // Move serialized arguments inside the memory, invoke dispatch thunk and
    // then free allocated memory.
    let invoke_args_len = invoke_args_data.len() as WordSize;
    let invoke_args_ptr = supervisor_context
        .allocate_memory(invoke_args_len)
        .map_err(|_| host_trap("Can't allocate memory in supervisor for the arguments"))?;

    let deallocate = |fe: &mut dyn SupervisorContext, ptr, fail_msg| {
        fe.deallocate_memory(ptr).map_err(|_| host_trap(fail_msg))
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

        return Err(host_trap("Can't write invoke args into memory"));
    }

    // Perform the actual call
    let serialized_result = supervisor_context
        .invoke(invoke_args_ptr, invoke_args_len, supervisor_func_index)
        .map_err(|e| host_trap(e.to_string()));

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
        .map_err(|_| host_trap("Can't read the serialized result from dispatch thunk"));

    deallocate(
        supervisor_context,
        serialized_result_val_ptr,
        "Can't deallocate memory for dispatch thunk's result",
    )?;

    serialized_result_val
}

/// Invoke a function within a sandboxed module
pub fn invoke(
    instance: &sandbox_wasmi::Instance,
    store: &Rc<StoreRefCell>,
    export_name: &str,
    args: &[Value],
    supervisor_context: &mut dyn SupervisorContext,
) -> std::result::Result<Option<Value>, Error> {
    let function = instance
        .get_func(&*store.borrow(), export_name)
        .ok_or_else(|| Error::Sandbox(format!("function {export_name} export error")))?;

    let args: Vec<sandbox_wasmi::Val> = args.iter().copied().map(into_wasmi_val).collect();
    let func_ty = function.ty(&*store.borrow());

    let mut outputs = vec![
        sandbox_wasmi::Val::ExternRef(sandbox_wasmi::ExternRef::null());
        func_ty.results().len()
    ];

    // Init func env
    {
        let gas_global = instance
            .get_global(&*store.borrow(), GLOBAL_NAME_GAS)
            .ok_or_else(|| Error::Sandbox("Failed to get gas global".into()))?;

        store
            .borrow_mut()
            .data_mut()
            .replace(FuncEnv::new(Rc::downgrade(store), gas_global));
    }

    SupervisorContextStore::using(supervisor_context, || {
        function
            .call(&mut *store.borrow_mut(), &args, &mut outputs)
            .map_err(|error| Error::Sandbox(error.to_string()))
    })?;

    match outputs.as_slice() {
        [] => Ok(None),
        [val] => match into_value(val) {
            None => Err(Error::Sandbox(format!("Unsupported return value: {val:?}"))),
            Some(v) => Ok(Some(v)),
        },
        _outputs => Err(Error::Sandbox(
            "multiple return types are not supported yet".into(),
        )),
    }
}
