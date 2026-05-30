// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// TODO (breathx): remove cloning of slices from wasm memory.

use crate::host::{StoreData, context};
use ethexe_runtime_common::pack_u32_to_i64;
use gear_sandbox_host::context::{
    self as sandbox_context, HostResult, Instantiate, Pointer, SupervisorFuncIndex, Value, WordSize,
};
use parity_scale_codec::Encode;
use wasmtime::{AsContextMut, Caller, Linker, StoreContextMut, Val};

pub fn link(linker: &mut Linker<StoreData>) -> Result<(), wasmtime::Error> {
    linker.func_wrap("env", "ext_sandbox_get_buff_version_1", get_buff)?;
    linker.func_wrap(
        "env",
        "ext_sandbox_get_global_val_version_1",
        get_global_val,
    )?;
    linker.func_wrap(
        "env",
        "ext_sandbox_get_instance_ptr_version_1",
        get_instance_ptr,
    )?;
    linker.func_wrap(
        "env",
        "ext_sandbox_instance_teardown_version_1",
        instance_teardown,
    )?;
    linker.func_wrap("env", "ext_sandbox_instantiate_version_2", instantiate)?;
    linker.func_wrap("env", "ext_sandbox_invoke_version_1", invoke)?;
    linker.func_wrap("env", "ext_sandbox_memory_get_version_1", memory_get)?;
    linker.func_wrap("env", "ext_sandbox_memory_grow_version_1", memory_grow)?;
    linker.func_wrap("env", "ext_sandbox_memory_new_version_1", memory_new)?;
    linker.func_wrap("env", "ext_sandbox_memory_set_version_1", memory_set)?;
    linker.func_wrap("env", "ext_sandbox_memory_size_version_1", memory_size)?;
    linker.func_wrap(
        "env",
        "ext_sandbox_memory_teardown_version_1",
        memory_teardown,
    )?;
    linker.func_wrap(
        "env",
        "ext_sandbox_set_global_val_version_1",
        set_global_val,
    )?;

    Ok(())
}

struct ProcessorContext<'a> {
    caller: StoreContextMut<'a, StoreData>,
}

impl<'a> ProcessorContext<'a> {
    fn new(caller: impl Into<StoreContextMut<'a, StoreData>>) -> Self {
        Self {
            caller: caller.into(),
        }
    }

    fn dispatcher(
        self,
        dispatch_thunk_id: u32,
        state_ptr: Pointer<u8>,
    ) -> ProcessorContextDispatcher<'a> {
        ProcessorContextDispatcher {
            context: self,
            dispatch_thunk_id,
            state_ptr,
        }
    }
}

impl sandbox_context::SupervisorContext for ProcessorContext<'_> {
    fn data_ptr(&self) -> *const () {
        self.caller.data() as *const _ as *const ()
    }

    fn read_memory_into(&self, address: Pointer<u8>, dest: &mut [u8]) -> Result<(), String> {
        let memory = context::memory(&self.caller);
        let slice = memory
            .slice(u32::from(address), dest.len() as u32)
            .ok_or_else(|| "memory read out of bounds".to_string())?;
        dest.copy_from_slice(slice);
        Ok(())
    }

    fn write_memory(&mut self, address: Pointer<u8>, data: &[u8]) -> Result<(), String> {
        context::memory(&mut self.caller)
            .slice_mut(u32::from(address), data.len() as u32)
            .ok_or_else(|| "memory write out of bounds".to_string())?
            .copy_from_slice(data);
        Ok(())
    }

    fn allocate_memory(&mut self, size: WordSize) -> Result<Pointer<u8>, String> {
        context::allocator(&mut self.caller)
            .allocate(size)
            .map(Pointer::new)
            .map_err(|err| err.to_string())
    }

    fn deallocate_memory(&mut self, ptr: Pointer<u8>) -> Result<(), String> {
        context::allocator(&mut self.caller)
            .deallocate(ptr.into())
            .map_err(|err| err.to_string())
    }
}

struct ProcessorContextDispatcher<'a> {
    context: ProcessorContext<'a>,
    dispatch_thunk_id: u32,
    state_ptr: Pointer<u8>,
}

impl gear_sandbox_host::context::SupervisorContext for ProcessorContextDispatcher<'_> {
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

impl sandbox_context::SupervisorContextDispatcher for ProcessorContextDispatcher<'_> {
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
        let table_item = table
            .get(
                self.context.caller.as_context_mut(),
                self.dispatch_thunk_id as u64,
            )
            .expect("dispatch_thunk_id is out of bounds");
        let dispatch_thunk = *table_item
            .unwrap_func()
            .expect("dispatch_thunk_idx should be a funcref");

        let mut ret_vals = [Val::I64(0)];
        let result = dispatch_thunk.call(
            &mut self.context.caller,
            &[
                Val::I32(u32::from(invoke_args_ptr) as i32),
                Val::I32(invoke_args_len as i32),
                Val::I32(u32::from(self.state_ptr) as i32),
                Val::I32(usize::from(func_idx) as i32),
            ],
            &mut ret_vals,
        );

        match result {
            Ok(()) => ret_vals[0]
                .i64()
                .ok_or_else(|| "Supervisor function returned unexpected result!".into()),
            Err(err) => Err(err.to_string().into()),
        }
    }
}

fn get_buff(caller: Caller<'_, StoreData>, memory_idx: u32) -> i64 {
    log::trace!(target: "host_call", "get_buff(memory_idx={memory_idx:?})");
    let res = sandbox_context::get_buff(ProcessorContext::new(caller), memory_idx) as i64;
    log::trace!(target: "host_call", "get_buff(..) -> {res:?}");
    res
}

fn get_global_val(mut caller: Caller<'_, StoreData>, instance_idx: u32, name: i64) -> i64 {
    log::trace!(
        target: "host_call",
        "get_global_val(instance_idx={instance_idx:?}, name={name:?})"
    );

    let name = context::memory(&mut caller).slice_by_val(name).to_vec();
    let name = core::str::from_utf8(&name).unwrap_or_default();

    let res =
        sandbox_context::get_global_val(ProcessorContext::new(&mut caller), instance_idx, name);
    let res = res.encode();
    let res_len = res.len() as u32;

    let ptr = context::allocator(&mut caller).allocate(res_len).unwrap();
    caller
        .data()
        .memory()
        .write(&mut caller, ptr as usize, &res)
        .unwrap();

    let res = pack_u32_to_i64(ptr, res_len);
    log::trace!(target: "host_call", "get_global_val(..) -> {res:?}");
    res
}

fn get_instance_ptr(caller: Caller<'_, StoreData>, instance_idx: u32) -> i64 {
    log::trace!(target: "host_call", "get_instance_ptr(instance_idx={instance_idx:?})");
    let res = sandbox_context::get_instance_ptr(ProcessorContext::new(caller), instance_idx) as i64;
    log::trace!(target: "host_call", "get_instance_ptr(..) -> {res:?}");
    res
}

fn instance_teardown(caller: Caller<'_, StoreData>, instance_idx: u32) {
    log::trace!(target: "host_call", "instance_teardown(instance_idx={instance_idx:?})");
    sandbox_context::instance_teardown(ProcessorContext::new(caller), instance_idx);
}

fn instantiate(
    mut caller: Caller<'_, StoreData>,
    dispatch_thunk_id: u32,
    wasm_code: i64,
    raw_env_def: i64,
    state_ptr: u32,
) -> i32 {
    log::trace!(
        target: "host_call",
        "instantiate(dispatch_thunk_id={dispatch_thunk_id:?}, wasm_code={wasm_code:?}, raw_env_def={raw_env_def:?}, state_ptr={state_ptr:?})"
    );

    let memory = context::memory(&mut caller);
    let wasm_code = memory.slice_by_val(wasm_code).to_vec();
    let raw_env_def = memory.slice_by_val(raw_env_def).to_vec();

    let res = sandbox_context::instantiate(
        ProcessorContext::new(caller).dispatcher(dispatch_thunk_id, Pointer::new(state_ptr)),
        &wasm_code,
        &raw_env_def,
        Instantiate::Version2,
    ) as i32;

    log::trace!(target: "host_call", "instantiate(..) -> {res:?}");
    res
}

fn invoke(
    mut caller: Caller<'_, StoreData>,
    instance_idx: u32,
    function: i64,
    args: i64,
    return_val_ptr: u32,
    return_val_len: u32,
    state_ptr: u32,
) -> i32 {
    log::trace!(
        target: "host_call",
        "invoke(instance_idx={instance_idx:?}, function={function:?}, args={args:?}, return_val_ptr={return_val_ptr:?}, return_val_len={return_val_len:?}, state_ptr={state_ptr:?})"
    );

    let memory = context::memory(&mut caller);
    let function = memory.slice_by_val(function).to_vec();
    let function = core::str::from_utf8(&function).unwrap_or_default();
    let args = memory.slice_by_val(args).to_vec();

    let res = sandbox_context::invoke(
        ProcessorContext::new(caller),
        ProcessorContext::dispatcher,
        instance_idx,
        function,
        &args,
        Pointer::new(return_val_ptr),
        return_val_len,
        Pointer::new(state_ptr),
    ) as i32;

    log::trace!(target: "host_call", "invoke(..) -> {res:?}");
    res
}

fn memory_get(
    caller: Caller<'_, StoreData>,
    memory_idx: u32,
    offset: u32,
    buff_ptr: u32,
    buff_len: u32,
) -> i32 {
    log::trace!(
        target: "host_call",
        "memory_get(memory_idx={memory_idx:?}, offset={offset:?}, buff_ptr={buff_ptr:?}, buff_len={buff_len:?})"
    );

    let res = sandbox_context::memory_get(
        ProcessorContext::new(caller),
        memory_idx,
        offset,
        Pointer::new(buff_ptr),
        buff_len,
    ) as i32;

    log::trace!(target: "host_call", "memory_get(..) -> {res:?}");
    res
}

fn memory_grow(caller: Caller<'_, StoreData>, memory_idx: u32, size: u32) -> i32 {
    log::trace!(target: "host_call", "memory_grow(memory_idx={memory_idx:?}, size={size:?})");
    let res = sandbox_context::memory_grow(ProcessorContext::new(caller), memory_idx, size) as i32;
    log::trace!(target: "host_call", "memory_grow(..) -> {res:?}");
    res
}

fn memory_new(caller: Caller<'_, StoreData>, initial: u32, maximum: u32) -> i32 {
    log::trace!(target: "host_call", "memory_new(initial={initial:?}, maximum={maximum:?})");
    let res = sandbox_context::memory_new(ProcessorContext::new(caller), initial, maximum) as i32;
    log::trace!(target: "host_call", "memory_new(..) -> {res:?}");
    res
}

fn memory_set(
    caller: Caller<'_, StoreData>,
    memory_idx: u32,
    offset: u32,
    val_ptr: u32,
    val_len: u32,
) -> i32 {
    log::trace!(
        target: "host_call",
        "memory_set(memory_idx={memory_idx:?}, offset={offset:?}, val_ptr={val_ptr:?}, val_len={val_len:?})"
    );

    let res = sandbox_context::memory_set(
        ProcessorContext::new(caller),
        memory_idx,
        offset,
        Pointer::new(val_ptr),
        val_len,
    ) as i32;

    log::trace!(target: "host_call", "memory_set(..) -> {res:?}");
    res
}

fn memory_size(caller: Caller<'_, StoreData>, memory_idx: u32) -> i32 {
    log::trace!(target: "host_call", "memory_size(memory_idx={memory_idx:?})");
    let res = sandbox_context::memory_size(ProcessorContext::new(caller), memory_idx) as i32;
    log::trace!(target: "host_call", "memory_size(..) -> {res:?}");
    res
}

fn memory_teardown(caller: Caller<'_, StoreData>, memory_idx: u32) {
    log::trace!(target: "host_call", "memory_teardown(memory_idx={memory_idx:?})");
    sandbox_context::memory_teardown(ProcessorContext::new(caller), memory_idx);
}

fn set_global_val(
    mut caller: Caller<'_, StoreData>,
    instance_idx: u32,
    name: i64,
    value: i64,
) -> i32 {
    log::trace!(
        target: "host_call",
        "set_global_val(instance_idx={instance_idx:?}, name={name:?}, value={value:?})"
    );

    let memory = context::memory(&mut caller);
    let name = memory.slice_by_val(name).to_vec();
    let name = core::str::from_utf8(&name).unwrap_or_default();
    let value: Value = memory.decode_by_val(value);

    let res =
        sandbox_context::set_global_val(ProcessorContext::new(caller), instance_idx, name, value)
            as i32;
    log::trace!(target: "host_call", "set_global_val(..) -> {res:?}");
    res
}
