// This file is part of Gear.
//
// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use core::{
    cell::RefCell,
    marker::PhantomData,
    sync::atomic::{AtomicU32, Ordering},
};
use std::panic::{self, AssertUnwindSafe};

use gear_sandbox_host::sandbox::{self as sandbox_env, SupervisorContext as _};
use parity_scale_codec::{Decode, Encode};

pub use gear_sandbox_host::{
    error::Result as HostResult,
    sandbox::{SandboxBackend, SupervisorFuncIndex, env::Instantiate},
};
pub use sp_wasm_interface_common::{HostPointer, Pointer, ReturnValue, Value, WordSize};

static SANDBOX_BACKEND_TYPE: sandbox_env::AtomicSandboxBackend =
    sandbox_env::AtomicSandboxBackend::new(sandbox_env::SandboxBackend::Wasmer);

const DEFAULT_SANDBOX_STORE_CLEAR_COUNTER_LIMIT: u32 = 50;

static SANDBOX_STORE_CLEAR_COUNTER_LIMIT: AtomicU32 =
    AtomicU32::new(DEFAULT_SANDBOX_STORE_CLEAR_COUNTER_LIMIT);

pub fn init(sandbox_backend: sandbox_env::SandboxBackend, store_clear_counter_limit: Option<u32>) {
    SANDBOX_BACKEND_TYPE.store(sandbox_backend, Ordering::SeqCst);
    SANDBOX_STORE_CLEAR_COUNTER_LIMIT.store(
        store_clear_counter_limit.unwrap_or(DEFAULT_SANDBOX_STORE_CLEAR_COUNTER_LIMIT),
        Ordering::SeqCst,
    );
}

pub struct Sandboxes {
    store_data_key: usize,
    store: sandbox_env::SandboxComponents,
}

impl Sandboxes {
    pub fn new() -> Self {
        let sandbox_backend = SANDBOX_BACKEND_TYPE.load(Ordering::SeqCst);

        Self {
            store_data_key: 0,
            store: sandbox_env::SandboxComponents::new(sandbox_backend),
        }
    }

    pub fn get(&mut self, store_data_key: usize) -> &mut sandbox_env::SandboxComponents {
        if self.store_data_key != store_data_key {
            self.store_data_key = store_data_key;
            self.store.clear();
        }

        &mut self.store
    }

    pub fn clear(&mut self, counter: &mut u32) {
        if *counter >= SANDBOX_STORE_CLEAR_COUNTER_LIMIT.load(Ordering::SeqCst) {
            *counter = 0;
            self.store.clear();
        }
        *counter += 1;
    }
}

impl Default for Sandboxes {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Default)]
struct ThreadState {
    sandboxes: Sandboxes,
    clear_counter: u32,
}

thread_local! {
    static THREAD_STATE: RefCell<ThreadState> = RefCell::new(ThreadState::default());
}

pub trait ContextOps {
    type Caller<'a>;

    fn trace(func: &str, caller: &Self::Caller<'_>);
    fn store_data_key(caller: &Self::Caller<'_>) -> usize;

    fn invoke_dispatch_thunk(
        caller: &mut Self::Caller<'_>,
        dispatch_thunk_id: u32,
        invoke_args_ptr: Pointer<u8>,
        invoke_args_len: WordSize,
        state: u32,
        func_idx: SupervisorFuncIndex,
    ) -> HostResult<i64>;

    fn read_memory_into(
        caller: &Self::Caller<'_>,
        address: Pointer<u8>,
        dest: &mut [u8],
    ) -> Result<(), String>;

    fn write_memory(
        caller: &mut Self::Caller<'_>,
        address: Pointer<u8>,
        data: &[u8],
    ) -> Result<(), String>;

    fn allocate_memory(
        caller: &mut Self::Caller<'_>,
        size: WordSize,
    ) -> Result<Pointer<u8>, String>;

    fn deallocate_memory(caller: &mut Self::Caller<'_>, ptr: Pointer<u8>) -> Result<(), String>;
}

fn with_thread_state<R>(f: impl FnOnce(&mut ThreadState) -> R) -> R {
    THREAD_STATE.with(|state| {
        let mut state = state.borrow_mut();
        f(&mut state)
    })
}

pub struct SupervisorContext<'a, 'b, O: ContextOps> {
    pub caller: &'a mut O::Caller<'b>,
    pub dispatch_thunk_id: u32,
    pub state: u32,
    _marker: PhantomData<O>,
}

impl<'a, 'b, O: ContextOps> SupervisorContext<'a, 'b, O> {
    pub fn new(caller: &'a mut O::Caller<'b>, dispatch_thunk_id: u32, state: u32) -> Self {
        Self {
            caller,
            dispatch_thunk_id,
            state,
            _marker: PhantomData,
        }
    }
}

impl<O: ContextOps> sandbox_env::SupervisorContext for SupervisorContext<'_, '_, O> {
    fn invoke(
        &mut self,
        invoke_args_ptr: Pointer<u8>,
        invoke_args_len: WordSize,
        func_idx: SupervisorFuncIndex,
    ) -> HostResult<i64> {
        O::invoke_dispatch_thunk(
            self.caller,
            self.dispatch_thunk_id,
            invoke_args_ptr,
            invoke_args_len,
            self.state,
            func_idx,
        )
    }

    fn read_memory_into(&self, address: Pointer<u8>, dest: &mut [u8]) -> Result<(), String> {
        O::read_memory_into(self.caller, address, dest)
    }

    fn write_memory(&mut self, address: Pointer<u8>, data: &[u8]) -> Result<(), String> {
        O::write_memory(self.caller, address, data)
    }

    fn allocate_memory(&mut self, size: WordSize) -> Result<Pointer<u8>, String> {
        O::allocate_memory(self.caller, size)
    }

    fn deallocate_memory(&mut self, ptr: Pointer<u8>) -> Result<(), String> {
        O::deallocate_memory(self.caller, ptr)
    }
}

fn read_memory<O: ContextOps>(
    caller: &O::Caller<'_>,
    address: Pointer<u8>,
    size: WordSize,
) -> Result<Vec<u8>, String> {
    let mut vec = vec![0; size as usize];
    O::read_memory_into(caller, address, &mut vec)?;
    Ok(vec)
}

pub fn get_buff<O: ContextOps + 'static>(
    caller: &mut O::Caller<'_>,
    memory_idx: u32,
) -> HostPointer {
    use gear_sandbox_host::util::MemoryTransfer;

    O::trace("get_buff", caller);

    with_thread_state(|state| {
        state
            .sandboxes
            .get(O::store_data_key(caller))
            .memory(memory_idx)
            .expect("Failed to get memory buffer pointer: cannot get backend memory")
            .get_buff() as HostPointer
    })
}

pub fn get_global_val<O: ContextOps + 'static>(
    caller: &mut O::Caller<'_>,
    instance_idx: u32,
    name: &str,
) -> Option<Value> {
    O::trace("get_global_val", caller);

    with_thread_state(|state| {
        state
            .sandboxes
            .get(O::store_data_key(caller))
            .instance(instance_idx)
            .map(|instance| instance.get_global_val(name))
            .map_err(|err| err.to_string())
            .expect("Failed to get global from sandbox")
    })
}

pub fn get_instance_ptr<O: ContextOps + 'static>(
    caller: &mut O::Caller<'_>,
    instance_idx: u32,
) -> HostPointer {
    O::trace("get_instance_ptr", caller);

    let instance = with_thread_state(|state| {
        state
            .sandboxes
            .get(O::store_data_key(caller))
            .instance(instance_idx)
            .expect("Failed to get sandboxed instance")
    });

    instance.as_ref().get_ref() as *const sandbox_env::SandboxInstance as HostPointer
}

pub fn instance_teardown<O: ContextOps + 'static>(caller: &mut O::Caller<'_>, instance_idx: u32) {
    O::trace("instance_teardown", caller);

    with_thread_state(|state| {
        state
            .sandboxes
            .get(O::store_data_key(caller))
            .instance_teardown(instance_idx)
            .expect("Failed to teardown sandbox instance");
    });
}

pub fn instantiate<O: ContextOps + 'static>(
    caller: &mut O::Caller<'_>,
    dispatch_thunk_id: u32,
    wasm_code: &[u8],
    raw_env_def: &[u8],
    state_ptr: Pointer<u8>,
    version: Instantiate,
) -> u32 {
    O::trace("instantiate", caller);

    let store_data_key = O::store_data_key(caller);

    let guest_env = with_thread_state(|state| {
        let store = state.sandboxes.get(store_data_key);
        sandbox_env::GuestEnvironment::decode(store, raw_env_def)
    });

    let Ok(guest_env) = guest_env else {
        return sandbox_env::env::ERR_MODULE;
    };

    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        with_thread_state(|state| {
            state.sandboxes.get(store_data_key).instantiate(
                version,
                wasm_code,
                guest_env,
                &mut SupervisorContext::<O>::new(caller, dispatch_thunk_id, state_ptr.into()),
            )
        })
    }));

    let result = match result {
        Ok(result) => result,
        Err(error) => panic::resume_unwind(error),
    };

    match result {
        Ok(instance) => with_thread_state(|state| {
            let store = state.sandboxes.get(store_data_key);
            instance.register(store, dispatch_thunk_id)
        }),
        Err(sandbox_env::InstantiationError::StartTrapped) => sandbox_env::env::ERR_EXECUTION,
        Err(_) => sandbox_env::env::ERR_MODULE,
    }
}

#[allow(clippy::too_many_arguments)]
pub fn invoke<O: ContextOps + 'static>(
    caller: &mut O::Caller<'_>,
    instance_idx: u32,
    function: &str,
    mut args: &[u8],
    return_val_ptr: Pointer<u8>,
    return_val_len: u32,
    state_ptr: Pointer<u8>,
) -> u32 {
    O::trace("invoke", caller);
    log::trace!("invoke, instance_idx={instance_idx}");

    let args = Vec::<Value>::decode(&mut args)
        .expect("Can't decode serialized arguments for the invocation")
        .into_iter()
        .collect::<Vec<_>>();

    let (instance, dispatch_thunk_id) = with_thread_state(|state| {
        let store = state.sandboxes.get(O::store_data_key(caller));

        let instance = store
            .instance(instance_idx)
            .expect("backend instance not found");

        let dispatch_thunk_id = store
            .dispatch_thunk_id(instance_idx)
            .expect("dispatch_thunk not found");

        (instance, dispatch_thunk_id)
    });

    let mut sandbox_context =
        SupervisorContext::<O>::new(caller, dispatch_thunk_id, state_ptr.into());

    match instance.invoke(function, &args, &mut sandbox_context) {
        Ok(None) => sandbox_env::env::ERR_OK,
        Ok(Some(val)) => {
            let encoded = ReturnValue::Value(val).encode();
            if encoded.len() > return_val_len as usize {
                panic!("Return value buffer is too small");
            }

            sandbox_context
                .write_memory(return_val_ptr, &encoded)
                .expect("can't write return value");

            sandbox_env::env::ERR_OK
        }
        Err(err) => {
            log::trace!("invoke error = {err:?}");
            sandbox_env::env::ERR_EXECUTION
        }
    }
}

pub fn memory_get<O: ContextOps + 'static>(
    caller: &mut O::Caller<'_>,
    memory_idx: u32,
    offset: u32,
    buf_ptr: Pointer<u8>,
    buf_len: u32,
) -> u32 {
    use gear_sandbox_host::util::MemoryTransfer;

    O::trace("memory_get", caller);

    let sandboxed_memory = with_thread_state(|state| {
        state
            .sandboxes
            .get(O::store_data_key(caller))
            .memory(memory_idx)
            .expect("sandboxed memory not found")
    });

    let buffer = match sandboxed_memory.read(Pointer::new(offset), buf_len as usize) {
        Err(_) => return sandbox_env::env::ERR_OUT_OF_BOUNDS,
        Ok(buffer) => buffer,
    };

    match O::write_memory(caller, buf_ptr, &buffer) {
        Ok(()) => sandbox_env::env::ERR_OK,
        Err(_) => sandbox_env::env::ERR_OUT_OF_BOUNDS,
    }
}

pub fn memory_grow<O: ContextOps + 'static>(
    caller: &mut O::Caller<'_>,
    memory_idx: u32,
    size: u32,
) -> u32 {
    use gear_sandbox_host::util::MemoryTransfer;

    O::trace("memory_grow", caller);

    with_thread_state(|state| {
        state
            .sandboxes
            .get(O::store_data_key(caller))
            .memory(memory_idx)
            .expect("Failed to grow memory: cannot get backend memory")
            .memory_grow(size)
            .expect("Failed to grow memory")
    })
}

pub fn memory_new<O: ContextOps + 'static>(
    caller: &mut O::Caller<'_>,
    initial: u32,
    maximum: u32,
) -> u32 {
    O::trace("memory_new", caller);

    with_thread_state(|state| {
        state.sandboxes.clear(&mut state.clear_counter);
        state
            .sandboxes
            .get(O::store_data_key(caller))
            .new_memory(initial, maximum)
            .map_err(|err| err.to_string())
            .expect("Failed to create new memory with sandbox")
    })
}

pub fn memory_set<O: ContextOps + 'static>(
    caller: &mut O::Caller<'_>,
    memory_idx: u32,
    offset: u32,
    val_ptr: Pointer<u8>,
    val_len: u32,
) -> u32 {
    use gear_sandbox_host::util::MemoryTransfer;

    O::trace("memory_set", caller);

    let Ok(buffer) = read_memory::<O>(caller, val_ptr, val_len) else {
        return sandbox_env::env::ERR_OUT_OF_BOUNDS;
    };

    let sandboxed_memory = with_thread_state(|state| {
        state
            .sandboxes
            .get(O::store_data_key(caller))
            .memory(memory_idx)
            .expect("memory_set: not found")
    });

    match sandboxed_memory.write_from(Pointer::new(offset), &buffer) {
        Ok(()) => sandbox_env::env::ERR_OK,
        Err(_) => sandbox_env::env::ERR_OUT_OF_BOUNDS,
    }
}

pub fn memory_size<O: ContextOps + 'static>(caller: &mut O::Caller<'_>, memory_idx: u32) -> u32 {
    use gear_sandbox_host::util::MemoryTransfer;

    O::trace("memory_size", caller);

    with_thread_state(|state| {
        state
            .sandboxes
            .get(O::store_data_key(caller))
            .memory(memory_idx)
            .expect("Failed to get memory size: cannot get backend memory")
            .memory_size()
    })
}

pub fn memory_teardown<O: ContextOps + 'static>(caller: &mut O::Caller<'_>, memory_idx: u32) {
    O::trace("memory_teardown", caller);

    with_thread_state(|state| {
        state
            .sandboxes
            .get(O::store_data_key(caller))
            .memory_teardown(memory_idx)
            .expect("Failed to teardown sandbox memory");
    });
}

pub fn set_global_val<O: ContextOps + 'static>(
    caller: &mut O::Caller<'_>,
    instance_idx: u32,
    name: &str,
    value: Value,
) -> u32 {
    O::trace("set_global_val", caller);
    log::trace!("set_global_val, instance_idx={instance_idx}");

    let result = with_thread_state(|state| {
        let instance = state
            .sandboxes
            .get(O::store_data_key(caller))
            .instance(instance_idx)
            .map_err(|err| err.to_string())
            .expect("Failed to set global in sandbox");

        instance.set_global_val(name, value)
    });

    log::trace!("set_global_val, name={name}, value={value:?}, result={result:?}");

    match result {
        Ok(None) => sandbox_env::env::ERROR_GLOBALS_NOT_FOUND,
        Ok(Some(_)) => sandbox_env::env::ERROR_GLOBALS_OK,
        Err(_) => sandbox_env::env::ERROR_GLOBALS_OTHER,
    }
}
