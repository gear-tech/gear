// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
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

use crate::host::{StoreData, api::MemoryWrap, context::HostContext, store};
use ethexe_runtime_common::pack_u32_to_i64;
use gear_sandbox_interface::host::{
    self as sandbox_detail, HostResult, Instantiate, Pointer, SupervisorFuncIndex, Value, WordSize,
};
use parity_scale_codec::{Decode, Encode};
use std::ops::Range;
use wasmtime::{AsContext, AsContextMut, Caller, Linker, Val};

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

struct ProcessorOps;

impl sandbox_detail::ContextOps for ProcessorOps {
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
        func_idx: SupervisorFuncIndex,
    ) -> HostResult<i64> {
        let table = caller
            .data()
            .table
            .expect("Runtime doesn't have a table; sandbox is unavailable");
        let table_item = table
            .get(caller.as_context_mut(), dispatch_thunk_id as u64)
            .expect("dispatch_thunk_id is out of bounds");
        let dispatch_thunk = *table_item
            .unwrap_func()
            .expect("dispatch_thunk_idx should be a funcref");

        let mut ret_vals = [Val::I64(0)];
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
            Ok(()) => ret_vals[0]
                .i64()
                .ok_or_else(|| "Supervisor function returned unexpected result!".into()),
            Err(err) => Err(err.to_string().into()),
        }
    }

    fn read_memory_into(
        caller: &Self::Caller<'_>,
        address: Pointer<u8>,
        dest: &mut [u8],
    ) -> Result<(), String> {
        let memory = caller.as_context().data().memory().data(caller);
        let range = checked_range(u32::from(address) as usize, dest.len(), memory.len())
            .ok_or_else(|| String::from("memory read is out of bounds"))?;
        dest.copy_from_slice(&memory[range]);
        Ok(())
    }

    fn write_memory(
        caller: &mut Self::Caller<'_>,
        address: Pointer<u8>,
        data: &[u8],
    ) -> Result<(), String> {
        store::write_memory_from(caller, u32::from(address), data)
    }

    fn allocate_memory(
        caller: &mut Self::Caller<'_>,
        size: WordSize,
    ) -> Result<Pointer<u8>, String> {
        store::allocate_memory(caller, size).map(Pointer::new)
    }

    fn deallocate_memory(caller: &mut Self::Caller<'_>, ptr: Pointer<u8>) -> Result<(), String> {
        store::deallocate_memory(caller, u32::from(ptr))
    }
}

fn checked_range(offset: usize, len: usize, max: usize) -> Option<Range<usize>> {
    let end = offset.checked_add(len)?;
    (end <= max).then(|| offset..end)
}

fn get_buff(mut caller: Caller<'_, StoreData>, memory_idx: i32) -> i64 {
    log::trace!(target: "host_call", "get_buff(memory_idx={memory_idx:?})");
    let res = sandbox_detail::get_buff::<ProcessorOps>(&mut caller, memory_idx as u32) as i64;
    log::trace!(target: "host_call", "get_buff(..) -> {res:?}");
    res
}

fn get_global_val(mut caller: Caller<'_, StoreData>, instance_idx: i32, name: i64) -> i64 {
    log::trace!(
        target: "host_call",
        "get_global_val(instance_idx={instance_idx:?}, name={name:?})"
    );

    let memory = MemoryWrap(caller.data().memory());
    let name = memory.slice_by_val(&caller, name).to_vec();
    let name = core::str::from_utf8(&name).unwrap_or_default();

    let res =
        sandbox_detail::get_global_val::<ProcessorOps>(&mut caller, instance_idx as u32, name);
    let res = res.encode();
    let res_len = res.len() as u32;

    let mut host_context = HostContext { caller };
    let ptr = host_context.allocate_memory(res_len).unwrap();
    let mut caller = host_context.caller;
    caller
        .data()
        .memory()
        .write(&mut caller, ptr as usize, &res)
        .unwrap();

    let res = pack_u32_to_i64(ptr, res_len);
    log::trace!(target: "host_call", "get_global_val(..) -> {res:?}");
    res
}

fn get_instance_ptr(mut caller: Caller<'_, StoreData>, instance_idx: i32) -> i64 {
    log::trace!(target: "host_call", "get_instance_ptr(instance_idx={instance_idx:?})");
    let res =
        sandbox_detail::get_instance_ptr::<ProcessorOps>(&mut caller, instance_idx as u32) as i64;
    log::trace!(target: "host_call", "get_instance_ptr(..) -> {res:?}");
    res
}

fn instance_teardown(mut caller: Caller<'_, StoreData>, instance_idx: i32) {
    log::trace!(target: "host_call", "instance_teardown(instance_idx={instance_idx:?})");
    sandbox_detail::instance_teardown::<ProcessorOps>(&mut caller, instance_idx as u32);
}

fn instantiate(
    mut caller: Caller<'_, StoreData>,
    dispatch_thunk_id: i32,
    wasm_code: i64,
    raw_env_def: i64,
    state_ptr: i32,
) -> i32 {
    log::trace!(
        target: "host_call",
        "instantiate(dispatch_thunk_id={dispatch_thunk_id:?}, wasm_code={wasm_code:?}, raw_env_def={raw_env_def:?}, state_ptr={state_ptr:?})"
    );

    let memory = MemoryWrap(caller.data().memory());
    let wasm_code = memory.slice_by_val(&caller, wasm_code).to_vec();
    let raw_env_def = memory.slice_by_val(&caller, raw_env_def).to_vec();

    let res = sandbox_detail::instantiate::<ProcessorOps>(
        &mut caller,
        dispatch_thunk_id as u32,
        &wasm_code,
        &raw_env_def,
        Pointer::new(state_ptr as u32),
        Instantiate::Version2,
    ) as i32;

    log::trace!(target: "host_call", "instantiate(..) -> {res:?}");
    res
}

fn invoke(
    mut caller: Caller<'_, StoreData>,
    instance_idx: i32,
    function: i64,
    args: i64,
    return_val_ptr: i32,
    return_val_len: i32,
    state_ptr: i32,
) -> i32 {
    log::trace!(
        target: "host_call",
        "invoke(instance_idx={instance_idx:?}, function={function:?}, args={args:?}, return_val_ptr={return_val_ptr:?}, return_val_len={return_val_len:?}, state_ptr={state_ptr:?})"
    );

    let memory = MemoryWrap(caller.data().memory());
    let function = memory.slice_by_val(&caller, function).to_vec();
    let function = core::str::from_utf8(&function).unwrap_or_default();
    let args = memory.slice_by_val(&caller, args).to_vec();

    let res = sandbox_detail::invoke::<ProcessorOps>(
        &mut caller,
        instance_idx as u32,
        function,
        &args,
        Pointer::new(return_val_ptr as u32),
        return_val_len as u32,
        Pointer::new(state_ptr as u32),
    ) as i32;

    log::trace!(target: "host_call", "invoke(..) -> {res:?}");
    res
}

fn memory_get(
    mut caller: Caller<'_, StoreData>,
    memory_idx: i32,
    offset: i32,
    buff_ptr: i32,
    buff_len: i32,
) -> i32 {
    log::trace!(
        target: "host_call",
        "memory_get(memory_idx={memory_idx:?}, offset={offset:?}, buff_ptr={buff_ptr:?}, buff_len={buff_len:?})"
    );

    let res = sandbox_detail::memory_get::<ProcessorOps>(
        &mut caller,
        memory_idx as u32,
        offset as u32,
        Pointer::new(buff_ptr as u32),
        buff_len as u32,
    ) as i32;

    log::trace!(target: "host_call", "memory_get(..) -> {res:?}");
    res
}

fn memory_grow(mut caller: Caller<'_, StoreData>, memory_idx: i32, size: i32) -> i32 {
    log::trace!(target: "host_call", "memory_grow(memory_idx={memory_idx:?}, size={size:?})");
    let res =
        sandbox_detail::memory_grow::<ProcessorOps>(&mut caller, memory_idx as u32, size as u32)
            as i32;
    log::trace!(target: "host_call", "memory_grow(..) -> {res:?}");
    res
}

fn memory_new(mut caller: Caller<'_, StoreData>, initial: i32, maximum: i32) -> i32 {
    log::trace!(target: "host_call", "memory_new(initial={initial:?}, maximum={maximum:?})");
    let res =
        sandbox_detail::memory_new::<ProcessorOps>(&mut caller, initial as u32, maximum as u32)
            as i32;
    log::trace!(target: "host_call", "memory_new(..) -> {res:?}");
    res
}

fn memory_set(
    mut caller: Caller<'_, StoreData>,
    memory_idx: i32,
    offset: i32,
    val_ptr: i32,
    val_len: i32,
) -> i32 {
    log::trace!(
        target: "host_call",
        "memory_set(memory_idx={memory_idx:?}, offset={offset:?}, val_ptr={val_ptr:?}, val_len={val_len:?})"
    );

    let res = sandbox_detail::memory_set::<ProcessorOps>(
        &mut caller,
        memory_idx as u32,
        offset as u32,
        Pointer::new(val_ptr as u32),
        val_len as u32,
    ) as i32;

    log::trace!(target: "host_call", "memory_set(..) -> {res:?}");
    res
}

fn memory_size(mut caller: Caller<'_, StoreData>, memory_idx: i32) -> i32 {
    log::trace!(target: "host_call", "memory_size(memory_idx={memory_idx:?})");
    let res = sandbox_detail::memory_size::<ProcessorOps>(&mut caller, memory_idx as u32) as i32;
    log::trace!(target: "host_call", "memory_size(..) -> {res:?}");
    res
}

fn memory_teardown(mut caller: Caller<'_, StoreData>, memory_idx: i32) {
    log::trace!(target: "host_call", "memory_teardown(memory_idx={memory_idx:?})");
    sandbox_detail::memory_teardown::<ProcessorOps>(&mut caller, memory_idx as u32);
}

fn set_global_val(
    mut caller: Caller<'_, StoreData>,
    instance_idx: i32,
    name: i64,
    value: i64,
) -> i32 {
    log::trace!(
        target: "host_call",
        "set_global_val(instance_idx={instance_idx:?}, name={name:?}, value={value:?})"
    );

    let memory = MemoryWrap(caller.data().memory());
    let name = memory.slice_by_val(&caller, name).to_vec();
    let name = core::str::from_utf8(&name).unwrap_or_default();
    let value = memory.slice_by_val(&caller, value).to_vec();
    let value = Value::decode(&mut value.as_slice()).unwrap();

    let res = sandbox_detail::set_global_val::<ProcessorOps>(
        &mut caller,
        instance_idx as u32,
        name,
        value,
    ) as i32;
    log::trace!(target: "host_call", "set_global_val(..) -> {res:?}");
    res
}
