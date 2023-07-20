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

use core::cell::RefCell;

use codec::{Decode, Encode};
use gear_sandbox_host::sandbox as sandbox_env;
use sp_wasm_interface::{
    util,
    wasmtime::{AsContext, AsContextMut, Func, Val},
    Caller, FunctionContext, HostPointer, Pointer, StoreData, Value, WordSize,
};

struct Store {
    store_data_key: usize,
    store: sandbox_env::Store<Func>,
}

impl Store {
    pub fn new() -> Self {
        Self {
            store_data_key: 0,
            store: sandbox_env::Store::new(sandbox_env::SandboxBackend::Wasmer),
        }
    }

    pub fn get(&mut self, store_data_key: usize) -> &mut sandbox_env::Store<Func> {
        if self.store_data_key != store_data_key {
            self.store_data_key = store_data_key;
            self.store.clear();
        }

        &mut self.store
    }
}

thread_local! {
    static SANDBOXES: RefCell<Store> = RefCell::new(Store::new());
}

pub fn init() {
    SANDBOXES.with(|sandboxes| {
        let _store = sandboxes.borrow_mut().get(0);
    })
}

struct SandboxContext<'a, 'b> {
    caller: &'a mut Caller<'b, StoreData>,
    dispatch_thunk: Func,
    /// Custom data to propagate it in supervisor export functions
    state: u32,
}

impl<'a, 'b> sandbox_env::SandboxContext for SandboxContext<'a, 'b> {
    fn invoke(
        &mut self,
        invoke_args_ptr: Pointer<u8>,
        invoke_args_len: WordSize,
        func_idx: sandbox_env::SupervisorFuncIndex,
    ) -> gear_sandbox_host::error::Result<i64> {
        let mut ret_vals = [Val::null()];
        let result = self.dispatch_thunk.call(
            &mut self.caller,
            &[
                Val::I32(u32::from(invoke_args_ptr) as i32),
                Val::I32(invoke_args_len as i32),
                Val::I32(self.state as i32),
                Val::I32(usize::from(func_idx) as i32),
            ],
            &mut ret_vals,
        );

        match result {
            Ok(()) => {
                if let Some(ret_val) = ret_vals[0].i64() {
                    Ok(ret_val)
                } else {
                    Err("Supervisor function returned unexpected result!".into())
                }
            }
            Err(err) => Err(err.to_string().into()),
        }
    }

    fn read_memory_into(
        &self,
        address: Pointer<u8>,
        dest: &mut [u8],
    ) -> sp_wasm_interface::Result<()> {
        util::read_memory_into(self.caller.as_context(), address, dest)
    }

    fn write_memory(&mut self, address: Pointer<u8>, data: &[u8]) -> sp_wasm_interface::Result<()> {
        util::write_memory_from(self.caller.as_context_mut(), address, data)
    }

    fn allocate_memory(&mut self, size: WordSize) -> sp_wasm_interface::Result<Pointer<u8>> {
        util::allocate_memory(self.caller, size)
    }

    fn deallocate_memory(&mut self, ptr: Pointer<u8>) -> sp_wasm_interface::Result<()> {
        util::deallocate_memory(self.caller, ptr)
    }
}

fn read_memory(
    ctx: impl AsContext<Data = StoreData>,
    address: Pointer<u8>,
    size: WordSize,
) -> sp_wasm_interface::Result<Vec<u8>> {
    let mut vec = vec![0; size as usize];
    util::read_memory_into(ctx, address, &mut vec)?;
    Ok(vec)
}

fn trace(func: &str, caller: &Caller<'_, StoreData>) {
    let data_ptr: *const _ = caller.data();
    let caller_ptr: *const _ = caller;
    let thread_id = std::thread::current().id();

    log::trace!(
        "{func}; data_ptr = {:#x?}, caller_ptr = {:#x?}, thread_id = {:?}",
        data_ptr as usize,
        caller_ptr as usize,
        thread_id,
    );
}

pub fn get_buff(context: &mut dyn FunctionContext, memory_idx: u32) -> HostPointer {
    use gear_sandbox_host::util::MemoryTransfer;

    let mut method_result: HostPointer = u32::MAX.into();

    sp_wasm_interface::with_caller_mut(context, |caller| {
        trace("get_buff", caller);

        let data_ptr: *const _ = caller.data();
        method_result = SANDBOXES.with(|sandboxes| {
            let mut memory = sandboxes
                .borrow_mut()
                .get(data_ptr as usize)
                .memory(memory_idx)
                .expect("Failed to get memory buffer pointer: cannot get backend memory");

            memory.get_buff() as HostPointer
        });
    });

    method_result
}

pub fn get_global_val(
    context: &mut dyn FunctionContext,
    instance_idx: u32,
    name: &str,
) -> Option<Value> {
    let mut method_result = None::<Value>;

    sp_wasm_interface::with_caller_mut(context, |caller| {
        trace("get_global_val", caller);

        let data_ptr: *const _ = caller.data();
        method_result = SANDBOXES.with(|sandboxes| {
            sandboxes
                .borrow_mut()
                .get(data_ptr as usize)
                .instance(instance_idx)
                .map(|i| i.get_global_val(name))
                .map_err(|e| e.to_string())
                .expect("Failed to get global from sandbox")
        });
    });

    method_result
}

pub fn get_instance_ptr(context: &mut dyn FunctionContext, instance_id: u32) -> HostPointer {
    let mut method_result: HostPointer = u32::MAX.into();

    sp_wasm_interface::with_caller_mut(context, |caller| {
        trace("get_instance_ptr", caller);

        let data_ptr: *const _ = caller.data();
        method_result = SANDBOXES.with(|sandboxes| {
            let instance = sandboxes
                .borrow_mut()
                .get(data_ptr as usize)
                .instance(instance_id)
                .expect("Failed to get sandboxed instance");

            instance.as_ref().get_ref() as *const gear_sandbox_host::sandbox::SandboxInstance
                as HostPointer
        });
    });

    method_result
}

pub fn instance_teardown(context: &mut dyn FunctionContext, instance_idx: u32) {
    sp_wasm_interface::with_caller_mut(context, |caller| {
        trace("instance_teardown", caller);

        let data_ptr: *const _ = caller.data();
        SANDBOXES.with(|sandboxes| {
            sandboxes
                .borrow_mut()
                .get(data_ptr as usize)
                .instance_teardown(instance_idx)
                .expect("Failed to teardown sandbox instance")
        })
    });
}

pub fn instantiate(
    context: &mut dyn FunctionContext,
    dispatch_thunk_id: u32,
    wasm_code: &[u8],
    raw_env_def: &[u8],
    state_ptr: Pointer<u8>,
) -> u32 {
    let mut method_result = u32::MAX;

    sp_wasm_interface::with_caller_mut(context, |caller| {
        trace("instantiate", caller);

        // Extract a dispatch thunk from the instance's table by the specified index.
        let dispatch_thunk = {
            let table = caller
                .data()
                .table
                .expect("Runtime doesn't have a table; sandbox is unavailable");
            let table_item = table.get(caller.as_context_mut(), dispatch_thunk_id);

            *table_item
                .expect("dispatch_thunk_id is out of bounds")
                .funcref()
                .expect("dispatch_thunk_idx should be a funcref")
                .expect("dispatch_thunk_idx should point to actual func")
        };

        let data_ptr: *const _ = caller.data();
        let store_data_key = data_ptr as usize;
        let guest_env = SANDBOXES.with(|sandboxes| {
            let mut store_ref = sandboxes.borrow_mut();
            let store = store_ref.get(store_data_key);

            sandbox_env::GuestEnvironment::decode(store, raw_env_def)
        });
        let Ok(guest_env) = guest_env else {
            method_result = sandbox_env::env::ERR_MODULE;
            return;
        };

        // Catch any potential panics so that we can properly restore the sandbox store
        // which we've destructively borrowed.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            SANDBOXES.with(|sandboxes| {
                sandboxes.borrow_mut().get(store_data_key).instantiate(
                    wasm_code,
                    guest_env,
                    &mut SandboxContext {
                        caller,
                        dispatch_thunk,
                        state: state_ptr.into(),
                    },
                )
            })
        }));

        let result = match result {
            Ok(result) => result,
            Err(error) => std::panic::resume_unwind(error),
        };

        let instance_idx_or_err_code = match result {
            Ok(instance) => SANDBOXES.with(|sandboxes| {
                let mut store_ref = sandboxes.borrow_mut();
                let store = store_ref.get(store_data_key);

                instance.register(store, dispatch_thunk)
            }),
            Err(sandbox_env::InstantiationError::StartTrapped) => sandbox_env::env::ERR_EXECUTION,
            Err(_) => sandbox_env::env::ERR_MODULE,
        };

        method_result = instance_idx_or_err_code;
    });

    method_result
}

pub fn invoke(
    context: &mut dyn FunctionContext,
    instance_idx: u32,
    function: &str,
    mut args: &[u8],
    return_val_ptr: Pointer<u8>,
    return_val_len: u32,
    state_ptr: Pointer<u8>,
) -> u32 {
    use sandbox_env::SandboxContext as _;

    let mut method_result = u32::MAX;

    sp_wasm_interface::with_caller_mut(context, |caller| {
        trace("invoke", caller);
        log::trace!("invoke, instance_idx={instance_idx}");

        // Deserialize arguments and convert them into wasmi types.
        let args = Vec::<sp_wasm_interface::Value>::decode(&mut args)
            .expect("Can't decode serialized arguments for the invocation")
            .into_iter()
            .collect::<Vec<_>>();

        let data_ptr: *const _ = caller.data();
        let (instance, dispatch_thunk) = SANDBOXES.with(|sandboxes| {
            let mut store_ref = sandboxes.borrow_mut();
            let store = store_ref.get(data_ptr as usize);

            let instance = store
                .instance(instance_idx)
                .expect("backend instance not found");

            let dispatch_thunk = store
                .dispatch_thunk(instance_idx)
                .expect("dispatch_thunk not found");

            (instance, dispatch_thunk)
        });

        let mut sandbox_context = SandboxContext {
            caller,
            dispatch_thunk,
            state: state_ptr.into(),
        };
        let result = instance.invoke(function, &args, &mut sandbox_context);

        method_result = match result {
            Ok(None) => sandbox_env::env::ERR_OK,
            Ok(Some(val)) => {
                // Serialize return value and write it back into the memory.
                sp_wasm_interface::ReturnValue::Value(val).using_encoded(|val| {
                    if val.len() > return_val_len as usize {
                        panic!("Return value buffer is too small");
                    }

                    sandbox_context
                        .write_memory(return_val_ptr, val)
                        .expect("can't write return value");

                    sandbox_env::env::ERR_OK
                })
            }
            Err(e) => {
                log::trace!("e = {e:?}");

                sandbox_env::env::ERR_EXECUTION
            }
        };
    });

    method_result
}

pub fn memory_get(
    context: &mut dyn FunctionContext,
    memory_idx: u32,
    offset: u32,
    buf_ptr: Pointer<u8>,
    buf_len: u32,
) -> u32 {
    use gear_sandbox_host::util::MemoryTransfer;

    let mut method_result = u32::MAX;

    sp_wasm_interface::with_caller_mut(context, |caller| {
        trace("memory_get", caller);

        let data_ptr: *const _ = caller.data();
        let sandboxed_memory = SANDBOXES.with(|sandboxes| {
            sandboxes
                .borrow_mut()
                .get(data_ptr as usize)
                .memory(memory_idx)
                .expect("sandboxed memory not found")
        });

        let len = buf_len as usize;

        let buffer = match sandboxed_memory.read(Pointer::new(offset), len) {
            Err(_) => {
                method_result = sandbox_env::env::ERR_OUT_OF_BOUNDS;
                return;
            }
            Ok(buffer) => buffer,
        };

        method_result = match util::write_memory_from(caller, buf_ptr, &buffer) {
            Ok(_) => sandbox_env::env::ERR_OK,
            Err(_) => sandbox_env::env::ERR_OUT_OF_BOUNDS,
        };
    });

    method_result
}

pub fn memory_grow(context: &mut dyn FunctionContext, memory_idx: u32, size: u32) -> u32 {
    use gear_sandbox_host::util::MemoryTransfer;

    let mut method_result = u32::MAX;

    sp_wasm_interface::with_caller_mut(context, |caller| {
        trace("memory_grow", caller);

        let data_ptr: *const _ = caller.data();
        method_result = SANDBOXES.with(|sandboxes| {
            let mut memory = sandboxes
                .borrow_mut()
                .get(data_ptr as usize)
                .memory(memory_idx)
                .expect("Failed to grow memory: cannot get backend memory");

            memory.memory_grow(size).expect("Failed to grow memory")
        });
    });

    method_result
}

pub fn memory_new(context: &mut dyn FunctionContext, initial: u32, maximum: u32) -> u32 {
    let mut method_result = u32::MAX;

    sp_wasm_interface::with_caller_mut(context, |caller| {
        trace("memory_new", caller);

        let data_ptr: *const _ = caller.data();
        method_result = SANDBOXES.with(|sandboxes| {
            sandboxes
                .borrow_mut()
                .get(data_ptr as usize)
                .new_memory(initial, maximum)
                .map_err(|e| e.to_string())
                .expect("Failed to create new memory with sandbox")
        });
    });

    method_result
}

pub fn memory_set(
    context: &mut dyn FunctionContext,
    memory_idx: u32,
    offset: u32,
    val_ptr: Pointer<u8>,
    val_len: u32,
) -> u32 {
    use gear_sandbox_host::util::MemoryTransfer;

    let mut method_result = u32::MAX;

    sp_wasm_interface::with_caller_mut(context, |caller| {
        trace("memory_set", caller);

        let Ok(buffer) = read_memory(&caller, val_ptr, val_len) else {
            method_result = sandbox_env::env::ERR_OUT_OF_BOUNDS;
            return;
        };

        let data_ptr: *const _ = caller.data();
        method_result = SANDBOXES.with(|sandboxes| {
            let sandboxed_memory = sandboxes
                .borrow_mut()
                .get(data_ptr as usize)
                .memory(memory_idx)
                .expect("memory_set: not found");

            match sandboxed_memory.write_from(Pointer::new(offset), &buffer) {
                Ok(_) => sandbox_env::env::ERR_OK,
                Err(_) => sandbox_env::env::ERR_OUT_OF_BOUNDS,
            }
        });
    });

    method_result
}

pub fn memory_size(context: &mut dyn FunctionContext, memory_idx: u32) -> u32 {
    use gear_sandbox_host::util::MemoryTransfer;

    let mut method_result = 0;

    sp_wasm_interface::with_caller_mut(context, |caller| {
        trace("memory_size", caller);

        let data_ptr: *const _ = caller.data();
        method_result = SANDBOXES.with(|sandboxes| {
            let mut memory = sandboxes
                .borrow_mut()
                .get(data_ptr as usize)
                .memory(memory_idx)
                .expect("Failed to get memory size: cannot get backend memory");

            memory.memory_size()
        });
    });

    method_result
}

pub fn memory_teardown(context: &mut dyn FunctionContext, memory_idx: u32) {
    sp_wasm_interface::with_caller_mut(context, |caller| {
        trace("memory_teardown", caller);

        let data_ptr: *const _ = caller.data();
        SANDBOXES.with(|sandboxes| {
            sandboxes
                .borrow_mut()
                .get(data_ptr as usize)
                .memory_teardown(memory_idx)
                .expect("Failed to teardown sandbox memory")
        });
    });
}

pub fn set_global_val(
    context: &mut dyn FunctionContext,
    instance_idx: u32,
    name: &str,
    value: Value,
) -> u32 {
    let mut method_result = u32::MAX;

    sp_wasm_interface::with_caller_mut(context, |caller| {
        trace("set_global_val", caller);

        log::trace!("set_global_val, instance_idx={instance_idx}");

        let data_ptr: *const _ = caller.data();
        let result = SANDBOXES.with(|sandboxes| {
            let instance = sandboxes
                .borrow_mut()
                .get(data_ptr as usize)
                .instance(instance_idx)
                .map_err(|e| e.to_string())
                .expect("Failed to set global in sandbox");

            instance.set_global_val(name, value)
        });

        log::trace!("set_global_val, name={name}, value={value:?}, result={result:?}",);

        method_result = match result {
            Ok(None) => sandbox_env::env::ERROR_GLOBALS_NOT_FOUND,
            Ok(Some(_)) => sandbox_env::env::ERROR_GLOBALS_OK,
            Err(_) => sandbox_env::env::ERROR_GLOBALS_OTHER,
        };
    });

    method_result
}
