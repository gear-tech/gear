// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

#![allow(missing_docs)]

use core::{
    cell::RefCell,
    sync::atomic::{AtomicU32, Ordering},
};
use parity_scale_codec::{Decode, Encode};
use std::{
    panic::{self, AssertUnwindSafe},
    ptr,
};

use crate::sandbox as sandbox_env;

pub use crate::{
    error::Result as HostResult,
    sandbox::{SandboxBackend, SupervisorFuncIndex, env::Instantiate},
};
pub use sp_wasm_interface_common::{HostPointer, Pointer, ReturnValue, Value, WordSize};

static SANDBOX_BACKEND_TYPE: sandbox_env::AtomicSandboxBackend =
    sandbox_env::AtomicSandboxBackend::new(sandbox_env::SandboxBackend::Wasmtime);

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
    store_data_ptr: *const (),
    store: sandbox_env::SandboxComponents,
}

impl Sandboxes {
    pub fn new() -> Self {
        let sandbox_backend = SANDBOX_BACKEND_TYPE.load(Ordering::SeqCst);

        Self {
            store_data_ptr: ptr::null(),
            store: sandbox_env::SandboxComponents::new(sandbox_backend),
        }
    }

    pub fn get(&mut self, store_data_ptr: *const ()) -> &mut sandbox_env::SandboxComponents {
        if self.store_data_ptr != store_data_ptr {
            self.store_data_ptr = store_data_ptr;
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

pub trait SupervisorContext {
    fn data_ptr(&self) -> *const ();

    fn read_memory_into(&self, address: Pointer<u8>, dest: &mut [u8]) -> Result<(), String>;

    fn read_memory(&self, address: Pointer<u8>, size: WordSize) -> HostResult<Vec<u8>> {
        let mut vec = vec![0; size as usize];
        self.read_memory_into(address, &mut vec)?;
        Ok(vec)
    }

    fn write_memory(&mut self, address: Pointer<u8>, data: &[u8]) -> Result<(), String>;

    fn allocate_memory(&mut self, size: WordSize) -> Result<Pointer<u8>, String>;

    fn deallocate_memory(&mut self, ptr: Pointer<u8>) -> Result<(), String>;
}

pub trait SupervisorContextDispatcher: SupervisorContext {
    fn dispatch_thunk_id(&self) -> u32;

    fn invoke(
        &mut self,
        invoke_args_ptr: Pointer<u8>,
        invoke_args_len: WordSize,
        func_idx: SupervisorFuncIndex,
    ) -> HostResult<i64>;
}

fn with_thread_state<R>(f: impl FnOnce(&mut ThreadState) -> R) -> R {
    THREAD_STATE.with(|state| {
        let mut state = state.borrow_mut();
        f(&mut state)
    })
}

fn read_memory(
    supervisor_context: &impl SupervisorContext,
    address: Pointer<u8>,
    size: WordSize,
) -> Result<Vec<u8>, String> {
    let mut vec = vec![0; size as usize];
    supervisor_context.read_memory_into(address, &mut vec)?;
    Ok(vec)
}

pub fn get_buff(supervisor_context: impl SupervisorContext, memory_idx: u32) -> HostPointer {
    use crate::util::MemoryTransfer;

    with_thread_state(|state| {
        state
            .sandboxes
            .get(supervisor_context.data_ptr())
            .memory(memory_idx)
            .expect("Failed to get memory buffer pointer: cannot get backend memory")
            .get_buff() as HostPointer
    })
}

pub fn get_global_val(
    supervisor_context: impl SupervisorContext,
    instance_idx: u32,
    name: &str,
) -> Option<Value> {
    with_thread_state(|state| {
        state
            .sandboxes
            .get(supervisor_context.data_ptr())
            .instance(instance_idx)
            .map(|instance| instance.get_global_val(name))
            .map_err(|err| err.to_string())
            .expect("Failed to get global from sandbox")
    })
}

pub fn get_instance_ptr(
    supervisor_context: impl SupervisorContext,
    instance_idx: u32,
) -> HostPointer {
    let instance = with_thread_state(|state| {
        state
            .sandboxes
            .get(supervisor_context.data_ptr())
            .instance(instance_idx)
            .expect("Failed to get sandboxed instance")
    });

    instance.as_ref().get_ref() as *const sandbox_env::SandboxInstance as HostPointer
}

pub fn instance_teardown(supervisor_context: impl SupervisorContext, instance_idx: u32) {
    with_thread_state(|state| {
        state
            .sandboxes
            .get(supervisor_context.data_ptr())
            .instance_teardown(instance_idx)
            .expect("Failed to teardown sandbox instance");
    });
}

pub fn instantiate(
    mut supervisor_context: impl SupervisorContextDispatcher,
    wasm_code: &[u8],
    raw_env_def: &[u8],
    version: Instantiate,
) -> u32 {
    let store_data_key = supervisor_context.data_ptr();

    let guest_env = with_thread_state(|state| {
        let store = state.sandboxes.get(store_data_key);
        sandbox_env::GuestEnvironment::decode(store, raw_env_def)
    });

    let Ok(guest_env) = guest_env else {
        return sandbox_env::env::ERR_MODULE;
    };

    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        with_thread_state(|thread_state| {
            thread_state.sandboxes.get(store_data_key).instantiate(
                version,
                wasm_code,
                guest_env,
                &mut supervisor_context,
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
            instance.register(store, supervisor_context.dispatch_thunk_id())
        }),
        Err(sandbox_env::InstantiationError::StartTrapped) => sandbox_env::env::ERR_EXECUTION,
        Err(_) => sandbox_env::env::ERR_MODULE,
    }
}

#[allow(clippy::too_many_arguments)]
pub fn invoke<C, D, F>(
    supervisor_context: C,
    supervisor_dispatcher: F,
    instance_idx: u32,
    function: &str,
    mut args: &[u8],
    return_val_ptr: Pointer<u8>,
    return_val_len: u32,
    state_ptr: Pointer<u8>,
) -> u32
where
    C: SupervisorContext,
    D: SupervisorContextDispatcher,
    F: FnOnce(C, u32, Pointer<u8>) -> D,
{
    log::trace!("invoke, instance_idx={instance_idx}");

    let args = Vec::<Value>::decode(&mut args)
        .expect("Can't decode serialized arguments for the invocation")
        .into_iter()
        .collect::<Vec<_>>();

    let (instance, dispatch_thunk_id) = with_thread_state(|state| {
        let store = state.sandboxes.get(supervisor_context.data_ptr());

        let instance = store
            .instance(instance_idx)
            .expect("backend instance not found");

        let dispatch_thunk_id = store
            .dispatch_thunk_id(instance_idx)
            .expect("dispatch_thunk not found");

        (instance, dispatch_thunk_id)
    });

    let mut dispatcher = supervisor_dispatcher(supervisor_context, dispatch_thunk_id, state_ptr);

    match instance.invoke(function, &args, &mut dispatcher) {
        Ok(None) => sandbox_env::env::ERR_OK,
        Ok(Some(val)) => {
            let encoded = ReturnValue::Value(val).encode();
            if encoded.len() > return_val_len as usize {
                panic!("Return value buffer is too small");
            }

            dispatcher
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

pub fn memory_get(
    mut supervisor_context: impl SupervisorContext,
    memory_idx: u32,
    offset: u32,
    buf_ptr: Pointer<u8>,
    buf_len: u32,
) -> u32 {
    use crate::util::MemoryTransfer;

    let sandboxed_memory = with_thread_state(|state| {
        state
            .sandboxes
            .get(supervisor_context.data_ptr())
            .memory(memory_idx)
            .expect("sandboxed memory not found")
    });

    let buffer = match sandboxed_memory.read(Pointer::new(offset), buf_len as usize) {
        Err(_) => return sandbox_env::env::ERR_OUT_OF_BOUNDS,
        Ok(buffer) => buffer,
    };

    match supervisor_context.write_memory(buf_ptr, &buffer) {
        Ok(()) => sandbox_env::env::ERR_OK,
        Err(_) => sandbox_env::env::ERR_OUT_OF_BOUNDS,
    }
}

pub fn memory_grow(supervisor_context: impl SupervisorContext, memory_idx: u32, size: u32) -> u32 {
    use crate::util::MemoryTransfer;

    with_thread_state(|state| {
        state
            .sandboxes
            .get(supervisor_context.data_ptr())
            .memory(memory_idx)
            .expect("Failed to grow memory: cannot get backend memory")
            .memory_grow(size)
            .expect("Failed to grow memory")
    })
}

pub fn memory_new(supervisor_context: impl SupervisorContext, initial: u32, maximum: u32) -> u32 {
    with_thread_state(|state| {
        state.sandboxes.clear(&mut state.clear_counter);
        state
            .sandboxes
            .get(supervisor_context.data_ptr())
            .new_memory(initial, maximum)
            .map_err(|err| err.to_string())
            .expect("Failed to create new memory with sandbox")
    })
}

pub fn memory_set(
    supervisor_context: impl SupervisorContext,
    memory_idx: u32,
    offset: u32,
    val_ptr: Pointer<u8>,
    val_len: u32,
) -> u32 {
    use crate::util::MemoryTransfer;

    let Ok(buffer) = read_memory(&supervisor_context, val_ptr, val_len) else {
        return sandbox_env::env::ERR_OUT_OF_BOUNDS;
    };

    let sandboxed_memory = with_thread_state(|state| {
        state
            .sandboxes
            .get(supervisor_context.data_ptr())
            .memory(memory_idx)
            .expect("memory_set: not found")
    });

    match sandboxed_memory.write_from(Pointer::new(offset), &buffer) {
        Ok(()) => sandbox_env::env::ERR_OK,
        Err(_) => sandbox_env::env::ERR_OUT_OF_BOUNDS,
    }
}

pub fn memory_size(supervisor_context: impl SupervisorContext, memory_idx: u32) -> u32 {
    use crate::util::MemoryTransfer;

    with_thread_state(|state| {
        state
            .sandboxes
            .get(supervisor_context.data_ptr())
            .memory(memory_idx)
            .expect("Failed to get memory size: cannot get backend memory")
            .memory_size()
    })
}

pub fn memory_teardown(supervisor_context: impl SupervisorContext, memory_idx: u32) {
    with_thread_state(|state| {
        state
            .sandboxes
            .get(supervisor_context.data_ptr())
            .memory_teardown(memory_idx)
            .expect("Failed to teardown sandbox memory");
    });
}

pub fn set_global_val(
    supervisor_context: impl SupervisorContext,
    instance_idx: u32,
    name: &str,
    value: Value,
) -> u32 {
    log::trace!("set_global_val, instance_idx={instance_idx}");

    let result = with_thread_state(|state| {
        let instance = state
            .sandboxes
            .get(supervisor_context.data_ptr())
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
