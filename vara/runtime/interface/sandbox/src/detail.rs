// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use gear_sandbox_host::context::{
    self, HostPointer, HostResult, Instantiate, Pointer, SupervisorFuncIndex, Value, WordSize,
};
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
}

impl<'a, 'b> RuntimeInterfaceContext<'a, 'b> {
    fn new(caller: &'a mut Caller<'b, StoreData>) -> Self {
        Self { caller }
    }

    fn dispatcher(
        self,
        dispatch_thunk_id: u32,
        state_ptr: Pointer<u8>,
    ) -> RuntimeInterfaceDispatchContext<'a, 'b> {
        RuntimeInterfaceDispatchContext {
            context: self,
            dispatch_thunk_id,
            state_ptr,
        }
    }
}

impl context::SupervisorContext for RuntimeInterfaceContext<'_, '_> {
    fn data_ptr(&self) -> *const () {
        self.caller.data() as *const _ as *const ()
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

struct RuntimeInterfaceDispatchContext<'a, 'b> {
    context: RuntimeInterfaceContext<'a, 'b>,
    dispatch_thunk_id: u32,
    state_ptr: Pointer<u8>,
}

impl context::SupervisorContext for RuntimeInterfaceDispatchContext<'_, '_> {
    fn data_ptr(&self) -> *const () {
        self.context.data_ptr()
    }

    fn read_memory_into(&self, address: Pointer<u8>, dest: &mut [u8]) -> Result<(), String> {
        self.context.read_memory_into(address, dest)
    }

    fn read_memory(&self, address: Pointer<u8>, size: WordSize) -> HostResult<Vec<u8>> {
        self.context.read_memory(address, size)
    }

    fn write_memory(&mut self, address: Pointer<u8>, data: &[u8]) -> Result<(), String> {
        self.context.write_memory(address, data)
    }

    fn allocate_memory(&mut self, size: WordSize) -> Result<Pointer<u8>, String> {
        self.context.allocate_memory(size)
    }

    fn deallocate_memory(&mut self, ptr: Pointer<u8>) -> Result<(), String> {
        self.context.deallocate_memory(ptr)
    }
}

impl context::SupervisorContextDispatcher for RuntimeInterfaceDispatchContext<'_, '_> {
    fn dispatch_thunk_id(&self) -> u32 {
        self.dispatch_thunk_id
    }

    fn invoke(
        &mut self,
        invoke_args_ptr: Pointer<u8>,
        invoke_args_len: WordSize,
        func_idx: SupervisorFuncIndex,
    ) -> HostResult<i64> {
        let table = self
            .context
            .caller
            .data()
            .table
            .expect("Runtime doesn't have a table; sandbox is unavailable");
        let table_item: Val = table
            .get(
                self.context.caller.as_context_mut(),
                self.dispatch_thunk_id.into(),
            )
            .map(Into::into)
            .expect("dispatch_thunk_id is out of bounds");
        let dispatch_thunk = *table_item
            .funcref()
            .expect("dispatch_thunk_idx should be a funcref")
            .expect("dispatch_thunk_idx should point to actual func");

        let mut ret_vals = [Val::FuncRef(None)];
        let result = dispatch_thunk.call(
            &mut *self.context.caller,
            &[
                Val::I32(u32::from(invoke_args_ptr) as i32),
                Val::I32(invoke_args_len as i32),
                Val::I32(u32::from(self.state_ptr) as i32),
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
    let mut method_result: HostPointer = u32::MAX.into();

    sp_wasm_interface::with_caller_mut(context, |caller| {
        trace("get_buff", caller);

        method_result = context::get_buff(RuntimeInterfaceContext::new(caller), memory_idx);
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

        method_result =
            context::get_global_val(RuntimeInterfaceContext::new(caller), instance_idx, name);
    });

    method_result
}

pub fn get_instance_ptr(context: &mut dyn FunctionContext, instance_id: u32) -> HostPointer {
    let mut method_result: HostPointer = u32::MAX.into();

    sp_wasm_interface::with_caller_mut(context, |caller| {
        trace("get_instance_ptr", caller);

        method_result =
            context::get_instance_ptr(RuntimeInterfaceContext::new(caller), instance_id);
    });

    method_result
}

pub fn instance_teardown(context: &mut dyn FunctionContext, instance_idx: u32) {
    sp_wasm_interface::with_caller_mut(context, |caller| {
        trace("instance_teardown", caller);

        context::instance_teardown(RuntimeInterfaceContext::new(caller), instance_idx);
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
        trace("instantiate", caller);

        method_result = context::instantiate(
            RuntimeInterfaceContext::new(caller).dispatcher(dispatch_thunk_id, state_ptr),
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
        trace("invoke", caller);

        method_result = context::invoke(
            RuntimeInterfaceContext::new(caller),
            RuntimeInterfaceContext::dispatcher,
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
        trace("memory_get", caller);

        method_result = context::memory_get(
            RuntimeInterfaceContext::new(caller),
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
        trace("memory_grow", caller);

        method_result =
            context::memory_grow(RuntimeInterfaceContext::new(caller), memory_idx, size);
    });

    method_result
}

pub fn memory_new(context: &mut dyn FunctionContext, initial: u32, maximum: u32) -> u32 {
    let mut method_result = u32::MAX;

    sp_wasm_interface::with_caller_mut(context, |caller| {
        trace("memory_new", caller);

        method_result = context::memory_new(RuntimeInterfaceContext::new(caller), initial, maximum);
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
        trace("memory_set", caller);

        method_result = context::memory_set(
            RuntimeInterfaceContext::new(caller),
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
        trace("memory_size", caller);

        method_result = context::memory_size(RuntimeInterfaceContext::new(caller), memory_idx);
    });

    method_result
}

pub fn memory_teardown(context: &mut dyn FunctionContext, memory_idx: u32) {
    sp_wasm_interface::with_caller_mut(context, |caller| {
        trace("memory_teardown", caller);

        context::memory_teardown(RuntimeInterfaceContext::new(caller), memory_idx);
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

        method_result = context::set_global_val(
            RuntimeInterfaceContext::new(caller),
            instance_idx,
            name,
            value,
        );
    });

    method_result
}
