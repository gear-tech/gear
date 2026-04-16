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

use crate::host::{self, HostPointer, Instantiate, Pointer, Value, WordSize};
use sp_wasm_interface::{
    Caller, FunctionContext, StoreData, util,
    wasmtime::{AsContext, AsContextMut, Val},
};

pub fn init(
    sandbox_backend: gear_sandbox_host::sandbox::SandboxBackend,
    store_clear_counter_limit: Option<u32>,
) {
    host::init(sandbox_backend, store_clear_counter_limit);
}

struct RuntimeInterfaceOps;

impl host::ContextOps for RuntimeInterfaceOps {
    type Caller<'a> = Caller<'a, StoreData>;

    fn trace(func: &str, caller: &Self::Caller<'_>) {
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

    fn store_data_key(caller: &Self::Caller<'_>) -> usize {
        caller.data() as *const _ as usize
    }

    fn invoke_dispatch_thunk(
        caller: &mut Self::Caller<'_>,
        dispatch_thunk_id: u32,
        invoke_args_ptr: Pointer<u8>,
        invoke_args_len: WordSize,
        state: u32,
        func_idx: gear_sandbox_host::sandbox::SupervisorFuncIndex,
    ) -> gear_sandbox_host::error::Result<i64> {
        let table = caller
            .data()
            .table
            .expect("Runtime doesn't have a table; sandbox is unavailable");
        let table_item = table
            .get(caller.as_context_mut(), dispatch_thunk_id)
            .expect("dispatch_thunk_id is out of bounds");
        let dispatch_thunk = *table_item
            .funcref()
            .expect("dispatch_thunk_idx should be a funcref")
            .expect("dispatch_thunk_idx should point to actual func");

        let mut ret_vals = [Val::null()];
        let result = dispatch_thunk.call(
            &mut *caller,
            &[
                Val::I32(u32::from(invoke_args_ptr) as i32),
                Val::I32(invoke_args_len as i32),
                Val::I32(state as i32),
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
        caller: &Self::Caller<'_>,
        address: Pointer<u8>,
        dest: &mut [u8],
    ) -> Result<(), String> {
        util::read_memory_into(caller.as_context(), address, dest).map_err(|err| err.to_string())
    }

    fn write_memory(
        caller: &mut Self::Caller<'_>,
        address: Pointer<u8>,
        data: &[u8],
    ) -> Result<(), String> {
        util::write_memory_from(caller.as_context_mut(), address, data)
            .map_err(|err| err.to_string())
    }

    fn allocate_memory(
        caller: &mut Self::Caller<'_>,
        size: WordSize,
    ) -> Result<Pointer<u8>, String> {
        util::allocate_memory(caller, size).map_err(|err| err.to_string())
    }

    fn deallocate_memory(caller: &mut Self::Caller<'_>, ptr: Pointer<u8>) -> Result<(), String> {
        util::deallocate_memory(caller, ptr).map_err(|err| err.to_string())
    }
}

pub fn get_buff(context: &mut dyn FunctionContext, memory_idx: u32) -> HostPointer {
    let mut method_result: HostPointer = u32::MAX.into();

    sp_wasm_interface::with_caller_mut(context, |caller| {
        method_result = host::get_buff::<RuntimeInterfaceOps>(caller, memory_idx);
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
        method_result = host::get_global_val::<RuntimeInterfaceOps>(caller, instance_idx, name);
    });

    method_result
}

pub fn get_instance_ptr(context: &mut dyn FunctionContext, instance_id: u32) -> HostPointer {
    let mut method_result: HostPointer = u32::MAX.into();

    sp_wasm_interface::with_caller_mut(context, |caller| {
        method_result = host::get_instance_ptr::<RuntimeInterfaceOps>(caller, instance_id);
    });

    method_result
}

pub fn instance_teardown(context: &mut dyn FunctionContext, instance_idx: u32) {
    sp_wasm_interface::with_caller_mut(context, |caller| {
        host::instance_teardown::<RuntimeInterfaceOps>(caller, instance_idx);
    });
}

pub fn instantiate(
    context: &mut dyn FunctionContext,
    dispatch_thunk_id: u32,
    wasm_code: &[u8],
    raw_env_def: &[u8],
    state_ptr: Pointer<u8>,
    version: Instantiate,
) -> u32 {
    let mut method_result = u32::MAX;

    sp_wasm_interface::with_caller_mut(context, |caller| {
        method_result = host::instantiate::<RuntimeInterfaceOps>(
            caller,
            dispatch_thunk_id,
            wasm_code,
            raw_env_def,
            state_ptr,
            version,
        );
    });

    method_result
}

pub fn invoke(
    context: &mut dyn FunctionContext,
    instance_idx: u32,
    function: &str,
    args: &[u8],
    return_val_ptr: Pointer<u8>,
    return_val_len: u32,
    state_ptr: Pointer<u8>,
) -> u32 {
    let mut method_result = u32::MAX;

    sp_wasm_interface::with_caller_mut(context, |caller| {
        method_result = host::invoke::<RuntimeInterfaceOps>(
            caller,
            instance_idx,
            function,
            args,
            return_val_ptr,
            return_val_len,
            state_ptr,
        );
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
    let mut method_result = u32::MAX;

    sp_wasm_interface::with_caller_mut(context, |caller| {
        method_result =
            host::memory_get::<RuntimeInterfaceOps>(caller, memory_idx, offset, buf_ptr, buf_len);
    });

    method_result
}

pub fn memory_grow(context: &mut dyn FunctionContext, memory_idx: u32, size: u32) -> u32 {
    let mut method_result = u32::MAX;

    sp_wasm_interface::with_caller_mut(context, |caller| {
        method_result = host::memory_grow::<RuntimeInterfaceOps>(caller, memory_idx, size);
    });

    method_result
}

pub fn memory_new(context: &mut dyn FunctionContext, initial: u32, maximum: u32) -> u32 {
    let mut method_result = u32::MAX;

    sp_wasm_interface::with_caller_mut(context, |caller| {
        method_result = host::memory_new::<RuntimeInterfaceOps>(caller, initial, maximum);
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
    let mut method_result = u32::MAX;

    sp_wasm_interface::with_caller_mut(context, |caller| {
        method_result =
            host::memory_set::<RuntimeInterfaceOps>(caller, memory_idx, offset, val_ptr, val_len);
    });

    method_result
}

pub fn memory_size(context: &mut dyn FunctionContext, memory_idx: u32) -> u32 {
    let mut method_result = 0;

    sp_wasm_interface::with_caller_mut(context, |caller| {
        method_result = host::memory_size::<RuntimeInterfaceOps>(caller, memory_idx);
    });

    method_result
}

pub fn memory_teardown(context: &mut dyn FunctionContext, memory_idx: u32) {
    sp_wasm_interface::with_caller_mut(context, |caller| {
        host::memory_teardown::<RuntimeInterfaceOps>(caller, memory_idx);
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
        method_result =
            host::set_global_val::<RuntimeInterfaceOps>(caller, instance_idx, name, value);
    });

    method_result
}
