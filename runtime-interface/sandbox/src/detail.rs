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

use gear_sandbox_host::context::{self, HostPointer, Instantiate, Pointer, Value, WordSize};
use sp_wasm_interface::{
    Caller, FunctionContext, StoreData, util,
    wasmtime::{AsContext, AsContextMut, Val},
};

pub fn init(
    sandbox_backend: gear_sandbox_host::sandbox::SandboxBackend,
    store_clear_counter_limit: Option<u32>,
) {
    context::init(sandbox_backend, store_clear_counter_limit);
}

struct RuntimeInterfaceContext<'a, 'b> {
    caller: &'a mut Caller<'b, StoreData>,
    state: u32,
}

impl<'a, 'b> RuntimeInterfaceContext<'a, 'b> {
    fn new(caller: &'a mut Caller<'b, StoreData>, state: u32) -> Self {
        Self { caller, state }
    }
}

impl context::SupervisorContext for RuntimeInterfaceContext<'_, '_> {
    fn trace(&self, func: &str) {
        let data_ptr: *const _ = self.caller.data();
        let caller_ptr: *const _ = self.caller;
        let thread_id = std::thread::current().id();

        log::trace!(
            "{func}; data_ptr = {:#x?}, caller_ptr = {:#x?}, thread_id = {:?}",
            data_ptr as usize,
            caller_ptr as usize,
            thread_id,
        );
    }

    fn store_data_key(&self) -> usize {
        self.caller.data() as *const _ as usize
    }

    fn invoke(
        &mut self,
        dispatch_thunk_id: u32,
        invoke_args_ptr: Pointer<u8>,
        invoke_args_len: WordSize,
        func_idx: gear_sandbox_host::sandbox::SupervisorFuncIndex,
    ) -> gear_sandbox_host::error::Result<i64> {
        let table = self
            .caller
            .data()
            .table
            .expect("Runtime doesn't have a table; sandbox is unavailable");
        let table_item = table
            .get(self.caller.as_context_mut(), dispatch_thunk_id)
            .expect("dispatch_thunk_id is out of bounds");
        let dispatch_thunk = *table_item
            .funcref()
            .expect("dispatch_thunk_idx should be a funcref")
            .expect("dispatch_thunk_idx should point to actual func");

        let mut ret_vals = [Val::null()];
        let result = dispatch_thunk.call(
            &mut *self.caller,
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

    fn read_memory_into(&self, address: Pointer<u8>, dest: &mut [u8]) -> Result<(), String> {
        util::read_memory_into(self.caller.as_context(), address, dest)
            .map_err(|err| err.to_string())
    }

    fn write_memory(&mut self, address: Pointer<u8>, data: &[u8]) -> Result<(), String> {
        util::write_memory_from(self.caller.as_context_mut(), address, data)
            .map_err(|err| err.to_string())
    }

    fn allocate_memory(&mut self, size: WordSize) -> Result<Pointer<u8>, String> {
        util::allocate_memory(self.caller, size).map_err(|err| err.to_string())
    }

    fn deallocate_memory(&mut self, ptr: Pointer<u8>) -> Result<(), String> {
        util::deallocate_memory(self.caller, ptr).map_err(|err| err.to_string())
    }
}

pub fn get_buff(context: &mut dyn FunctionContext, memory_idx: u32) -> HostPointer {
    let mut method_result: HostPointer = u32::MAX.into();

    sp_wasm_interface::with_caller_mut(context, |caller| {
        method_result = context::get_buff(&mut RuntimeInterfaceContext::new(caller, 0), memory_idx);
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
        method_result = context::get_global_val(
            &mut RuntimeInterfaceContext::new(caller, 0),
            instance_idx,
            name,
        );
    });

    method_result
}

pub fn get_instance_ptr(context: &mut dyn FunctionContext, instance_id: u32) -> HostPointer {
    let mut method_result: HostPointer = u32::MAX.into();

    sp_wasm_interface::with_caller_mut(context, |caller| {
        method_result =
            context::get_instance_ptr(&mut RuntimeInterfaceContext::new(caller, 0), instance_id);
    });

    method_result
}

pub fn instance_teardown(context: &mut dyn FunctionContext, instance_idx: u32) {
    sp_wasm_interface::with_caller_mut(context, |caller| {
        context::instance_teardown(&mut RuntimeInterfaceContext::new(caller, 0), instance_idx);
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
        method_result = context::instantiate(
            &mut RuntimeInterfaceContext::new(caller, state_ptr.into()),
            dispatch_thunk_id,
            wasm_code,
            raw_env_def,
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
        method_result = context::invoke(
            &mut RuntimeInterfaceContext::new(caller, state_ptr.into()),
            instance_idx,
            function,
            args,
            return_val_ptr,
            return_val_len,
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
        method_result = context::memory_get(
            &mut RuntimeInterfaceContext::new(caller, 0),
            memory_idx,
            offset,
            buf_ptr,
            buf_len,
        );
    });

    method_result
}

pub fn memory_grow(context: &mut dyn FunctionContext, memory_idx: u32, size: u32) -> u32 {
    let mut method_result = u32::MAX;

    sp_wasm_interface::with_caller_mut(context, |caller| {
        method_result = context::memory_grow(
            &mut RuntimeInterfaceContext::new(caller, 0),
            memory_idx,
            size,
        );
    });

    method_result
}

pub fn memory_new(context: &mut dyn FunctionContext, initial: u32, maximum: u32) -> u32 {
    let mut method_result = u32::MAX;

    sp_wasm_interface::with_caller_mut(context, |caller| {
        method_result = context::memory_new(
            &mut RuntimeInterfaceContext::new(caller, 0),
            initial,
            maximum,
        );
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
        method_result = context::memory_set(
            &mut RuntimeInterfaceContext::new(caller, 0),
            memory_idx,
            offset,
            val_ptr,
            val_len,
        );
    });

    method_result
}

pub fn memory_size(context: &mut dyn FunctionContext, memory_idx: u32) -> u32 {
    let mut method_result = 0;

    sp_wasm_interface::with_caller_mut(context, |caller| {
        method_result =
            context::memory_size(&mut RuntimeInterfaceContext::new(caller, 0), memory_idx);
    });

    method_result
}

pub fn memory_teardown(context: &mut dyn FunctionContext, memory_idx: u32) {
    sp_wasm_interface::with_caller_mut(context, |caller| {
        context::memory_teardown(&mut RuntimeInterfaceContext::new(caller, 0), memory_idx);
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
        method_result = context::set_global_val(
            &mut RuntimeInterfaceContext::new(caller, 0),
            instance_idx,
            name,
            value,
        );
    });

    method_result
}
