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

use parity_scale_codec::{Decode, Encode};

use crate::{
    env, AsContextExt, Error, GlobalsSetError, HostFuncType, ReturnValue, SandboxStore, Value,
};
use alloc::{format, string::String};
use gear_sandbox_env::WasmReturnValue;
use gear_sandbox_interface::sandbox;
use sp_core::RuntimeDebug;
use sp_std::{marker, mem, prelude::*, rc::Rc, slice, vec};
use sp_wasm_interface_common::HostPointer;

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
        // runtime assert. const_assert!(size_of::<HostFuncIndex>() == size_of::<HostFuncType<T>>());
        assert!(size_of::<HostFuncIndex>() == size_of::<HostFuncType<T>>());
        unsafe { mem::transmute::<HostFuncIndex, HostFuncType<T>>(idx) }
    }
}

pub trait AsContext {}

#[derive(RuntimeDebug)]
pub struct Store<T>(T);

impl<T> SandboxStore for Store<T> {
    fn new(state: T) -> Self {
        Self(state)
    }
}

impl<T> AsContextExt for Store<T> {
    type State = T;

    fn data_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<T> AsContext for Store<T> {}

pub struct Caller<'a, T>(&'a mut T);

impl<T> AsContextExt for Caller<'_, T> {
    type State = T;

    fn data_mut(&mut self) -> &mut T {
        self.0
    }
}

impl<T> AsContext for Caller<'_, T> {}

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

    fn read<Context>(&self, _ctx: &Context, offset: u32, buf: &mut [u8]) -> Result<(), Error>
    where
        Context: AsContextExt<State = T>,
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
            err => {
                let err_msg = format!("Memory::read: unexpected error. Got error - {err}");

                log::error!("{err_msg}");
                unreachable!("{err_msg}")
            }
        }
    }

    fn write<Context>(&self, _ctx: &mut Context, offset: u32, val: &[u8]) -> Result<(), Error>
    where
        Context: AsContextExt<State = T>,
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
            err => {
                let err_msg = format!("Memory::write: unexpected error. Got error - {err}");

                log::error!("{err_msg}");
                unreachable!("{err_msg}")
            }
        }
    }

    fn grow<Context>(&self, ctx: &mut Context, pages: u32) -> Result<u32, Error>
    where
        Context: AsContextExt<State = T>,
    {
        let size = self.size(ctx);
        sandbox::memory_grow(self.handle.memory_idx, pages);
        Ok(size)
    }

    fn size<Context>(&self, _ctx: &Context) -> u32
    where
        Context: AsContextExt<State = T>,
    {
        sandbox::memory_size(self.handle.memory_idx)
    }

    unsafe fn get_buff<Context>(&self, _ctx: &Context) -> HostPointer
    where
        Context: AsContextExt<State = T>,
    {
        sandbox::get_buff(self.handle.memory_idx)
    }
}

/// A builder for the environment of the sandboxed WASM module.
pub struct EnvironmentDefinitionBuilder<T> {
    env_def: env::EnvironmentDefinition,
    retained_memories: Vec<Memory>,
    _marker: marker::PhantomData<T>,
}

impl<T> EnvironmentDefinitionBuilder<T> {
    fn add_entry<N1, N2>(&mut self, module: N1, field: N2, extern_entity: env::ExternEntity)
    where
        N1: Into<String>,
        N2: Into<String>,
    {
        let entry = env::Entry {
            module_name: module.into(),
            field_name: field.into(),
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
            retained_memories: Vec::new(),
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
        // We need to retain memory to keep it alive while the EnvironmentDefinitionBuilder alive.
        self.retained_memories.push(mem.clone());

        let mem = env::ExternEntity::Memory(mem.handle.memory_idx);
        self.add_entry(module, field, mem);
    }
}

/// Sandboxed instance of a WASM module.
pub struct Instance<T> {
    instance_idx: Rc<u32>,
    _retained_memories: Vec<Memory>,
    _marker: marker::PhantomData<T>,
}

impl<T> Clone for Instance<T> {
    fn clone(&self) -> Self {
        Self {
            instance_idx: self.instance_idx.clone(),
            _retained_memories: self._retained_memories.clone(),
            _marker: marker::PhantomData,
        }
    }
}

impl<T> Drop for Instance<T> {
    fn drop(&mut self) {
        if let Some(idx) = Rc::get_mut(&mut self.instance_idx) {
            sandbox::instance_teardown(*idx);
        }
    }
}

/// The primary responsibility of this thunk is to deserialize arguments and
/// call the original function, specified by the index.
extern "C" fn dispatch_thunk<T>(
    serialized_args_ptr: *const u8,
    serialized_args_len: usize,
    state: usize,
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
        let state = &mut *(state as *mut T);
        let mut caller = Caller(state);

        let mut result = Vec::with_capacity(WasmReturnValue::ENCODED_MAX_SIZE);
        // Pass control flow to the designated function.
        f(&mut caller, &args).encode_to(&mut result);

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

        // It's very important to instantiate thunk with the right type.
        let dispatch_thunk = dispatch_thunk::<T>;
        let result = sandbox::instantiate(
            dispatch_thunk as usize as u32,
            code,
            &serialized_env_def,
            store.data_mut() as *const T as _,
        );

        let instance_idx = match result {
            env::ERR_MODULE => return Err(Error::Module),
            env::ERR_EXECUTION => return Err(Error::Execution),
            instance_idx => instance_idx,
        };

        // We need to retain memories to keep them alive while the Instance is alive.
        let retained_memories = env_def_builder.retained_memories.clone();
        Ok(Instance {
            instance_idx: Rc::new(instance_idx),
            _retained_memories: retained_memories,
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

        let result = sandbox::invoke(
            *self.instance_idx,
            name,
            &serialized_args,
            return_val.as_mut_ptr() as _,
            return_val.len() as u32,
            store.data_mut() as *const T as _,
        );

        match result {
            env::ERR_OK => {
                let return_val =
                    ReturnValue::decode(&mut &return_val[..]).map_err(|_| Error::Execution)?;
                Ok(return_val)
            }
            env::ERR_EXECUTION => Err(Error::Execution),
            err => {
                let err_msg = format!("Memory::invoke: unexpected error. Got error - {err}");

                log::error!("{err_msg}");
                unreachable!("{err_msg}")
            }
        }
    }

    fn get_global_val(&self, _store: &mut Store<T>, name: &str) -> Option<Value> {
        sandbox::get_global_val(*self.instance_idx, name)
    }

    fn set_global_val(
        &self,
        _store: &mut Store<T>,
        name: &str,
        value: Value,
    ) -> Result<(), super::GlobalsSetError> {
        match sandbox::set_global_val(*self.instance_idx, name, value) {
            env::ERROR_GLOBALS_OK => Ok(()),
            env::ERROR_GLOBALS_NOT_FOUND => Err(GlobalsSetError::NotFound),
            _ => Err(GlobalsSetError::Other),
        }
    }

    fn get_instance_ptr(&self) -> HostPointer {
        sandbox::get_instance_ptr(*self.instance_idx)
    }
}
