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

use std::{cell::RefCell, collections::BTreeMap, fmt, rc::Rc};

use codec::{Decode, Encode};
use gear_sandbox_env::HostError;
use sp_wasm_interface::{util, Pointer, ReturnValue, Value, WordSize};
use wasmi::{
    core::{Pages, Trap},
    AsContext, AsContextMut, Engine, Extern, ExternType, Func, Globals, Linker, Module, Store,
};

use crate::{
    error::{self, Error},
    sandbox::{
        BackendInstance, GuestEnvironment, InstantiationError, Memory, SandboxContext,
        SandboxInstance, SupervisorFuncIndex,
    },
    util::MemoryTransfer,
};

environmental::environmental!(SandboxContextStore: trait SandboxContext);

thread_local! {
    static WASMI_CALLER: RefCell<Option<wasmi::Caller<'static, ()>>> = RefCell::new(None);
}

#[must_use]
struct WasmiCallerSetter(());

impl WasmiCallerSetter {
    fn new(caller: wasmi::Caller<'_, ()>) -> Self {
        unsafe {
            WASMI_CALLER.with(|ref_| {
                let static_caller = std::mem::transmute::<
                    wasmi::Caller<'_, ()>,
                    wasmi::Caller<'static, ()>,
                >(caller);

                let old_caller = ref_.borrow_mut().replace(static_caller);
                assert!(old_caller.is_none());
            });
        }

        Self(())
    }
}

impl Drop for WasmiCallerSetter {
    fn drop(&mut self) {
        unsafe {
            WASMI_CALLER.with(|caller| {
                let caller = caller
                    .borrow_mut()
                    .take()
                    .expect("caller set in `WasmiCallerSetter::new`");

                let _caller = std::mem::transmute::<
                    wasmi::Caller<'static, ()>,
                    wasmi::Caller<'_, ()>,
                >(caller);
            });
        }
    }
}

#[derive(Clone)]
struct WasmiContext {
    store: Rc<RefCell<Store<()>>>,
}

impl WasmiContext {
    fn with<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&dyn AsContext<UserState = ()>) -> R,
    {
        WASMI_CALLER.with(|caller| match caller.borrow().as_ref() {
            Some(store) => f(store),
            None => {
                let store = self.store.borrow();
                f(&*store)
            }
        })
    }

    fn with_mut<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut dyn AsContextMut<UserState = ()>) -> R,
    {
        WASMI_CALLER.with(|caller| match caller.borrow_mut().as_mut() {
            Some(store) => f(store),
            None => {
                let mut store = self.store.borrow_mut();
                f(&mut *store)
            }
        })
    }
}

/// Wasmi specific context
pub struct Backend {
    engine: Engine,
    store: Rc<RefCell<Store<()>>>,
}

impl Backend {
    pub fn new() -> Self {
        let engine = Engine::default();
        let store = Store::new(&engine, ());
        let store = Rc::new(RefCell::new(store));
        Backend { engine, store }
    }

    fn create_context(&self) -> WasmiContext {
        WasmiContext {
            store: self.store.clone(),
        }
    }
}

/// Allocate new memory region
pub fn new_memory(
    context: &mut Backend,
    initial: u32,
    maximum: Option<u32>,
) -> crate::error::Result<Memory> {
    let memory_type =
        wasmi::MemoryType::new(initial, maximum).map_err(|err| Error::Sandbox(err.to_string()))?;
    let memory = wasmi::Memory::new(&mut *context.store.borrow_mut(), memory_type)
        .map_err(|err| Error::Sandbox(err.to_string()))?;
    let memory = Memory::Wasmi(MemoryWrapper::new(context, memory));

    Ok(memory)
}

/// Wasmi provides direct access to its memory using slices.
///
/// This wrapper limits the scope where the slice can be taken to
#[derive(Clone)]
pub struct MemoryWrapper {
    memory: wasmi::Memory,
    context: WasmiContext,
}

impl fmt::Debug for MemoryWrapper {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("MemoryWrapper")
            .field("memory", &self.memory)
            .finish()
    }
}

impl MemoryWrapper {
    /// Take ownership of the memory region and return a wrapper object
    fn new(context: &Backend, memory: wasmi::Memory) -> Self {
        Self {
            memory,
            context: context.create_context(),
        }
    }
}

impl MemoryTransfer for MemoryWrapper {
    fn read(&self, source_addr: Pointer<u8>, size: usize) -> error::Result<Vec<u8>> {
        self.context.with(|context| {
            let source = self.memory.data(context.as_context());

            let range = util::checked_range(source_addr.into(), size, source.len())
                .ok_or_else(|| error::Error::Other("memory read is out of bounds".into()))?;

            Ok(Vec::from(&source[range]))
        })
    }

    fn read_into(&self, source_addr: Pointer<u8>, destination: &mut [u8]) -> error::Result<()> {
        self.context.with(|context| {
            let source = self.memory.data(context.as_context());
            let range = util::checked_range(source_addr.into(), destination.len(), source.len())
                .ok_or_else(|| error::Error::Other("memory read is out of bounds".into()))?;

            destination.copy_from_slice(&source[range]);
            Ok(())
        })
    }

    fn write_from(&self, dest_addr: Pointer<u8>, source: &[u8]) -> error::Result<()> {
        self.context.with_mut(|context| {
            let destination = self.memory.data_mut(context.as_context_mut());
            let range = util::checked_range(dest_addr.into(), source.len(), destination.len())
                .ok_or_else(|| error::Error::Other("memory write is out of bounds".into()))?;

            destination[range].copy_from_slice(source);
            Ok(())
        })
    }

    fn memory_grow(&mut self, pages: u32) -> error::Result<u32> {
        self.context.with_mut(|context| {
            self.memory
                .grow(context.as_context_mut(), Pages::from(pages as u16))
                .map_err(|e| {
                    Error::Sandbox(format!(
                        "Cannot grow memory in masmi sandbox executor: {}",
                        e
                    ))
                })
                .map(Into::into)
        })
    }

    fn memory_size(&mut self) -> u32 {
        self.context
            .with(|context| self.memory.current_pages(context.as_context()).into())
    }

    fn get_buff(&mut self) -> *mut u8 {
        self.context
            .with_mut(|context| self.memory.data_mut(context.as_context_mut()).as_mut_ptr())
    }
}

/// Instantiate a module within a sandbox context
pub fn instantiate(
    backend: &mut Backend,
    wasm: &[u8],
    guest_env: GuestEnvironment,
    sandbox_context: &mut dyn SandboxContext,
) -> std::result::Result<SandboxInstance, InstantiationError> {
    let store = &mut backend.store;
    let module =
        Module::new(&backend.engine, wasm).map_err(|_| InstantiationError::ModuleDecoding)?;

    let mut linker = Linker::new(&backend.engine);
    for import in module.imports() {
        match import.ty() {
            ExternType::Func(func_type) => {
                let Some(guest_func_index) = guest_env
                    .imports
                    .func_by_name(import.module(), import.name()) else {
                    continue
                };

                let supervisor_func_index = guest_env
                    .guest_to_supervisor_mapping
                    .func_by_guest_index(guest_func_index)
                    .ok_or(InstantiationError::ModuleDecoding)?;

                let func = dispatch_function(
                    supervisor_func_index,
                    &mut store.borrow_mut(),
                    func_type.clone(),
                );
                linker
                    .define(import.module(), import.name(), func)
                    .map_err(|_| InstantiationError::Instantiation)?;
            }
            ExternType::Memory(_) => {
                let memory = guest_env
                    .imports
                    .memory_by_name(import.module(), import.name())
                    .ok_or(InstantiationError::ModuleDecoding)?;

                let memory = memory.as_wasmi().expect(
                    "memory is created by wasmi; \
					exported by the same module and backend; \
					thus the operation can't fail; \
					qed",
                );

                linker
                    .define(import.module(), import.name(), memory.memory)
                    .map_err(|_| InstantiationError::Instantiation)?;
            }
            _ => continue,
        }
    }

    let instance = SandboxContextStore::using(sandbox_context, || {
        let instance_pre = linker
            .instantiate(&mut *store.borrow_mut(), &module)
            .map_err(|_| InstantiationError::Instantiation)?;
        instance_pre
            .start(&mut *store.borrow_mut())
            .map_err(|_| InstantiationError::StartTrapped)
    })?;

    let exports = instance
        .exports(&*store.borrow())
        .map(|export| (export.name().to_string(), export.into_extern()))
        .collect();
    let globals = store.borrow_mut().globals();
    let store = store.clone();

    Ok(SandboxInstance {
        backend_instance: BackendInstance::Wasmi {
            instance,
            store,
            exports,
            globals,
        },
    })
}

fn dispatch_function(
    supervisor_func_index: SupervisorFuncIndex,
    store: &mut Store<()>,
    func_ty: wasmi::FuncType,
) -> Func {
    Func::new(store, func_ty, move |caller, params, results| {
        let _wasmi_caller_guard = WasmiCallerSetter::new(caller);
        SandboxContextStore::with(|sandbox_context| {
            // Serialize arguments into a byte vector.
            let invoke_args_data = params
                .iter()
                .cloned()
                .map(|val| wasmi_to_ri(val).map_err(Trap::new))
                .collect::<std::result::Result<Vec<_>, _>>()?
                .encode();

            // Move serialized arguments inside the memory, invoke dispatch thunk and
            // then free allocated memory.
            let invoke_args_len = invoke_args_data.len() as WordSize;
            let invoke_args_ptr = sandbox_context
                .allocate_memory(invoke_args_len)
                .map_err(|_| Trap::new("Can't allocate memory in supervisor for the arguments"))?;

            let deallocate = |fe: &mut dyn SandboxContext, ptr, fail_msg| {
                fe.deallocate_memory(ptr).map_err(|_| Trap::new(fail_msg))
            };

            if sandbox_context
                .write_memory(invoke_args_ptr, &invoke_args_data)
                .is_err()
            {
                deallocate(
                    sandbox_context,
                    invoke_args_ptr,
                    "Failed dealloction after failed write of invoke arguments",
                )?;

                return Err(Trap::new("Can't write invoke args into memory"));
            }

            // Perform the actuall call
            let serialized_result = sandbox_context
                .invoke(invoke_args_ptr, invoke_args_len, supervisor_func_index)
                .map_err(|e| Trap::new(e.to_string()));

            deallocate(
                sandbox_context,
                invoke_args_ptr,
                "Failed dealloction after invoke",
            )?;

            let serialized_result = serialized_result?;

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
                .map_err(|_| Trap::new("Can't read the serialized result from dispatch thunk"));

            deallocate(
                sandbox_context,
                serialized_result_val_ptr,
                "Can't deallocate memory for dispatch thunk's result",
            )?;

            let serialized_result_val = serialized_result_val?;

            let deserialized_result = std::result::Result::<ReturnValue, HostError>::decode(
                &mut serialized_result_val.as_slice(),
            )
            .map_err(|_| Trap::new("Decoding Result<ReturnValue, HostError> failed!"))?
            .map_err(|_| Trap::new("Supervisor function returned sandbox::HostError"))?;

            match deserialized_result {
                ReturnValue::Value(value) => {
                    results[0] = ri_to_wasmi(value);
                }
                ReturnValue::Unit => {}
            };

            Ok(())
        })
        .expect("SandboxContextStore is set when invoking sandboxed functions; qed")
    })
}

/// Invoke a function within a sandboxed module
pub fn invoke(
    instance: &wasmi::Instance,
    store: Rc<RefCell<Store<()>>>,
    export_name: &str,
    args: &[Value],
    sandbox_context: &mut dyn SandboxContext,
) -> std::result::Result<Option<Value>, error::Error> {
    let function = instance
        .get_func(&*store.borrow(), export_name)
        .ok_or_else(|| Error::MethodNotFound(export_name.to_string()))?;

    let args: Vec<wasmi::Value> = args.iter().cloned().map(ri_to_wasmi).collect();

    let results = function.ty(&*store.borrow()).results().len();
    let mut results = vec![wasmi::Value::I32(0); results];
    SandboxContextStore::using(sandbox_context, || {
        function
            .call(&mut *store.borrow_mut(), &args, &mut results)
            .map_err(|error| Error::Sandbox(error.to_string()))
    })?;

    let results: &[wasmi::Value] = results.as_ref();
    match results {
        [] => Ok(None),

        [wasm_value] => wasmi_to_ri(wasm_value.clone())
            .map(Some)
            .map_err(Error::Sandbox),

        _ => Err(Error::Sandbox(
            "multiple return types are not supported yet".into(),
        )),
    }
}

/// Get global value by name
pub fn get_global(
    exports: &BTreeMap<String, Extern>,
    globals: &Globals,
    name: &str,
) -> Option<Value> {
    let global = exports.get(name).copied()?.into_global()?;
    let value = globals.resolve(&global).get();
    wasmi_to_ri(value).ok()
}

/// Set global value by name
pub fn set_global(
    exports: &BTreeMap<String, Extern>,
    globals: &Globals,
    name: &str,
    value: Value,
) -> std::result::Result<Option<()>, error::Error> {
    let Some(Extern::Global(global)) = exports.get(name) else {
        return Ok(None);
    };

    let value = ri_to_wasmi(value);
    globals
        .resolve_mut_with(&global, |entity| entity.set(value))
        .map_err(|err| Error::Sandbox(err.to_string()))?;

    Ok(Some(()))
}

fn wasmi_to_ri(val: wasmi::Value) -> Result<Value, String> {
    match val {
        wasmi::Value::I32(val) => Ok(Value::I32(val)),
        wasmi::Value::I64(val) => Ok(Value::I64(val)),
        wasmi::Value::F32(val) => Ok(Value::F32(val.to_bits())),
        wasmi::Value::F64(val) => Ok(Value::F64(val.to_bits())),
        _ => Err(format!("Unsupported function argument: {:?}", val)),
    }
}

fn ri_to_wasmi(val: Value) -> wasmi::Value {
    match val {
        Value::I32(val) => wasmi::Value::I32(val),
        Value::I64(val) => wasmi::Value::I64(val),
        Value::F32(val) => wasmi::Value::F32(wasmi::core::F32::from_bits(val)),
        Value::F64(val) => wasmi::Value::F64(wasmi::core::F64::from_bits(val)),
    }
}
