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

//! A WASM executor utilizing the sandbox runtime interface of the host.

use codec::{Decode, Encode};

use crate::{
    env, AsContext, Error, GlobalsSetError, HostFuncType, ReturnValue, SandboxCaller, SandboxStore,
    Value,
};
use alloc::string::String;
use gear_runtime_interface::sandbox;
use gear_sandbox_env::WasmReturnValue;
use sp_std::{marker, mem, prelude::*, rc::Rc, slice, vec};
use sp_wasm_interface::HostPointer;

mod ffi {
    use super::HostFuncType;
    use sp_std::mem;

    /// Index into the default table that points to a `HostFuncType`.
    pub type HostFuncIndex = usize;

    /// Coerce `HostFuncIndex` to a callable host function pointer.
    ///
    /// # Safety
    ///
    /// This function should be only called with a `HostFuncIndex` that was previously registered
    /// in the environment definition. Typically this should only
    /// be called with an argument received in `dispatch_thunk`.
    pub unsafe fn coerce_host_index_to_func<T>(idx: HostFuncIndex) -> HostFuncType<T> {
        // We need to ensure that sizes of a callable function pointer and host function index is
        // indeed equal.
        // We can't use `static_assertions` create because it makes compiler panic, fallback to
        // runtime assert. const_assert!(mem::size_of::<HostFuncIndex>() ==
        // mem::size_of::<HostFuncType<T>>());
        assert!(mem::size_of::<HostFuncIndex>() == mem::size_of::<HostFuncType<T>>());
        mem::transmute::<HostFuncIndex, HostFuncType<T>>(idx)
    }
}

fn set_global_val(instance_idx: u32, name: &str, value: Value) -> Result<(), GlobalsSetError> {
    match sandbox::set_global_val(instance_idx, name, value) {
        env::ERROR_GLOBALS_OK => Ok(()),
        env::ERROR_GLOBALS_NOT_FOUND => Err(GlobalsSetError::NotFound),
        _ => Err(GlobalsSetError::Other),
    }
}

fn get_global_val(instance_idx: u32, name: &str) -> Option<Value> {
    sandbox::get_global_val(instance_idx, name)
}

pub trait AsContextExt {}

pub struct Store<T>(T);

impl<T> SandboxStore<T> for Store<T> {
    fn new(state: T) -> Self {
        Self(state)
    }
}

impl<T> AsContext<T> for Store<T> {
    fn data_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<T> AsContextExt for Store<T> {}

pub struct Caller<'a, T> {
    data: &'a mut T,
    mem: Memory,
    instance_idx: u32,
}

impl<'a, T> SandboxCaller<T> for Caller<'a, T> {
    fn set_global_val(&mut self, name: &str, value: Value) -> Option<()> {
        set_global_val(self.instance_idx, name, value).ok()
    }

    fn get_global_val(&self, name: &str) -> Option<Value> {
        get_global_val(self.instance_idx, name)
    }

    fn memory(&self) -> Memory {
        self.mem.clone()
    }
}

impl<T> AsContext<T> for Caller<'_, T> {
    fn data_mut(&mut self) -> &mut T {
        self.data
    }
}

impl<T> AsContextExt for Caller<'_, T> {}

struct MemoryHandle {
    memory_idx: u32,
}

impl Drop for MemoryHandle {
    fn drop(&mut self) {
        sandbox::memory_teardown(self.memory_idx);
    }
}

/// The linear memory used by the sandbox.
#[derive(Clone)]
pub struct Memory {
    // Handle to memory instance is wrapped to add reference-counting semantics
    // to `Memory`.
    handle: Rc<MemoryHandle>,
}

impl<T> super::SandboxMemory<T> for Memory {
    fn new(_store: &mut Store<T>, initial: u32, maximum: Option<u32>) -> Result<Memory, Error> {
        let maximum = if let Some(maximum) = maximum {
            maximum
        } else {
            env::MEM_UNLIMITED
        };

        match sandbox::memory_new(initial, maximum) {
            env::ERR_MODULE => Err(Error::Module),
            memory_idx => Ok(Memory {
                handle: Rc::new(MemoryHandle { memory_idx }),
            }),
        }
    }

    fn get<C>(&self, _ctx: &C, offset: u32, buf: &mut [u8]) -> Result<(), Error>
    where
        C: AsContext<T>,
    {
        let result = sandbox::memory_get(
            self.handle.memory_idx,
            offset,
            buf.as_mut_ptr(),
            buf.len() as u32,
        );
        match result {
            env::ERR_OK => Ok(()),
            env::ERR_OUT_OF_BOUNDS => Err(Error::OutOfBounds),
            _ => unreachable!(),
        }
    }

    fn set<C>(&self, _ctx: &mut C, offset: u32, val: &[u8]) -> Result<(), Error>
    where
        C: AsContext<T>,
    {
        let result = sandbox::memory_set(
            self.handle.memory_idx,
            offset,
            val.as_ptr() as _,
            val.len() as u32,
        );
        match result {
            env::ERR_OK => Ok(()),
            env::ERR_OUT_OF_BOUNDS => Err(Error::OutOfBounds),
            _ => unreachable!(),
        }
    }

    fn grow<C>(&self, ctx: &mut C, pages: u32) -> Result<u32, Error>
    where
        C: AsContext<T>,
    {
        let size = self.size(ctx);
        sandbox::memory_grow(self.handle.memory_idx, pages);
        Ok(size)
    }

    fn size<C>(&self, _ctx: &C) -> u32
    where
        C: AsContext<T>,
    {
        sandbox::memory_size(self.handle.memory_idx)
    }

    unsafe fn get_buff<C>(&self, _ctx: &mut C) -> HostPointer
    where
        C: AsContext<T>,
    {
        sandbox::get_buff(self.handle.memory_idx)
    }
}

/// A builder for the environment of the sandboxed WASM module.
pub struct EnvironmentDefinitionBuilder<T> {
    env_def: env::EnvironmentDefinition,
    memory: Option<Memory>,
    _marker: marker::PhantomData<T>,
}

impl<T> EnvironmentDefinitionBuilder<T> {
    fn add_entry<N1, N2>(&mut self, module: N1, field: N2, extern_entity: env::ExternEntity)
    where
        N1: Into<String>,
        N2: Into<String>,
    {
        let entry = env::Entry {
            module_name: module.into().into(),
            field_name: field.into().into(),
            entity: extern_entity,
        };
        self.env_def.entries.push(entry);
    }
}

impl<T> super::SandboxEnvironmentBuilder<T, Memory> for EnvironmentDefinitionBuilder<T> {
    fn new() -> EnvironmentDefinitionBuilder<T> {
        EnvironmentDefinitionBuilder {
            env_def: env::EnvironmentDefinition {
                entries: Vec::new(),
            },
            memory: None,
            _marker: marker::PhantomData::<T>,
        }
    }

    fn add_host_func<N1, N2>(&mut self, module: N1, field: N2, f: HostFuncType<T>)
    where
        N1: Into<String>,
        N2: Into<String>,
    {
        let f = env::ExternEntity::Function(f as usize as u32);
        self.add_entry(module, field, f);
    }

    fn add_memory<N1, N2>(&mut self, module: N1, field: N2, mem: Memory)
    where
        N1: Into<String>,
        N2: Into<String>,
    {
        let memory_idx = mem.handle.memory_idx;

        // We need to retain memory to keep it alive while the EnvironmentDefinitionBuilder alive.
        let old_mem = self.memory.replace(mem);
        assert!(old_mem.is_none());

        let mem = env::ExternEntity::Memory(memory_idx);
        self.add_entry(module, field, mem);
    }
}

/// Sandboxed instance of a WASM module.
pub struct Instance<T> {
    instance_idx: u32,
    memory: Memory,
    _marker: marker::PhantomData<T>,
}

#[repr(C)]
struct DispatchThunkState {
    instance_idx: Option<u32>,
    memory: Memory,
    data: usize,
}

/// The primary responsibility of this thunk is to deserialize arguments and
/// call the original function, specified by the index.
extern "C" fn dispatch_thunk<T>(
    serialized_args_ptr: *const u8,
    serialized_args_len: usize,
    state: *mut DispatchThunkState,
    f: ffi::HostFuncIndex,
) -> u64 {
    let serialized_args = unsafe {
        if serialized_args_len == 0 {
            &[]
        } else {
            slice::from_raw_parts(serialized_args_ptr, serialized_args_len)
        }
    };
    let args = Vec::<Value>::decode(&mut &serialized_args[..]).expect(
        "serialized args should be provided by the runtime;
			correctly serialized data should be deserializable;
			qed",
    );

    unsafe {
        // This should be safe since `coerce_host_index_to_func` is called with an argument
        // received in an `dispatch_thunk` implementation, so `f` should point
        // on a valid host function.
        let f = ffi::coerce_host_index_to_func(f);

        // This should be safe since mutable reference to T is passed upon the invocation.
        let state = &*state;
        let data = &mut *(state.data as *mut T);
        let caller = Caller {
            data,
            mem: state.memory.clone(),
            instance_idx: state
                .instance_idx
                .unwrap_or_else(|| unreachable!("Instance index should be present")),
        };

        let mut result = Vec::with_capacity(WasmReturnValue::ENCODED_MAX_SIZE);
        // Pass control flow to the designated function.
        f(caller, &args).encode_to(&mut result);

        // Leak the result vector and return the pointer to return data.
        let result_ptr = result.as_ptr() as u64;
        let result_len = result.len() as u64;
        mem::forget(result);

        (result_ptr << 32) | result_len
    }
}

impl<T> super::SandboxInstance<T> for Instance<T> {
    type Memory = Memory;
    type EnvironmentBuilder = EnvironmentDefinitionBuilder<T>;

    fn new(
        store: &mut Store<T>,
        code: &[u8],
        env_def_builder: &EnvironmentDefinitionBuilder<T>,
    ) -> Result<Instance<T>, Error> {
        let serialized_env_def: Vec<u8> = env_def_builder.env_def.encode();

        let memory = env_def_builder
            .memory
            .clone()
            .unwrap_or_else(|| unreachable!("Memory expected to be present"));

        let mut state = DispatchThunkState {
            instance_idx: None,
            memory: memory.clone(),
            data: store.data_mut() as *const T as _,
        };

        // It's very important to instantiate thunk with the right type.
        let dispatch_thunk = dispatch_thunk::<T>;
        let result = sandbox::instantiate(
            dispatch_thunk as usize as u32,
            code,
            &serialized_env_def,
            &mut state as *mut DispatchThunkState as _,
        );

        let instance_idx = match result {
            env::ERR_MODULE => return Err(Error::Module),
            env::ERR_EXECUTION => return Err(Error::Execution),
            instance_idx => instance_idx,
        };

        Ok(Instance {
            instance_idx,
            memory,
            _marker: marker::PhantomData::<T>,
        })
    }

    fn invoke(
        &mut self,
        store: &mut Store<T>,
        name: &str,
        args: &[Value],
    ) -> Result<ReturnValue, Error> {
        let serialized_args = args.to_vec().encode();
        let mut return_val = vec![0u8; ReturnValue::ENCODED_MAX_SIZE];

        let mut state = DispatchThunkState {
            instance_idx: Some(self.instance_idx),
            memory: self.memory.clone(),
            data: store.data_mut() as *const T as _,
        };

        let result = sandbox::invoke(
            self.instance_idx,
            name,
            &serialized_args,
            return_val.as_mut_ptr() as _,
            return_val.len() as u32,
            &mut state as *mut DispatchThunkState as _,
        );

        match result {
            env::ERR_OK => {
                let return_val =
                    ReturnValue::decode(&mut &return_val[..]).map_err(|_| Error::Execution)?;
                Ok(return_val)
            }
            env::ERR_EXECUTION => Err(Error::Execution),
            _ => unreachable!(),
        }
    }

    fn get_global_val(&self, _store: &Store<T>, name: &str) -> Option<Value> {
        get_global_val(self.instance_idx, name)
    }

    fn set_global_val(&self, name: &str, value: Value) -> Result<(), super::GlobalsSetError> {
        set_global_val(self.instance_idx, name, value)
    }

    fn get_instance_ptr(&self) -> HostPointer {
        sandbox::get_instance_ptr(self.instance_idx)
    }
}

impl<T> Drop for Instance<T> {
    fn drop(&mut self) {
        sandbox::instance_teardown(self.instance_idx);
    }
}
