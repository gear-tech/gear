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

// TODO (breathx): remove cloning of slices from wasm memory.

use crate::host::{api::MemoryWrap, context::HostContext};
use ethexe_runtime_common::pack_u32_to_i64;
use gear_runtime_interface::{Instantiate, sandbox_detail};
use parity_scale_codec::Encode;
use sp_wasm_interface::{FunctionContext as _, IntoValue as _, Pointer, StoreData};
use wasmtime::{Caller, Linker};

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

fn get_buff(caller: Caller<'_, StoreData>, memory_idx: i32) -> i64 {
    log::trace!(target: "host_call", "get_buff(memory_idx={memory_idx:?})");

    let mut host_context = HostContext { caller };
    let res = sandbox_detail::get_buff(&mut host_context, memory_idx as u32) as i64;

    log::trace!(target: "host_call", "get_buff(..) -> {res:?}");

    res
}

fn get_global_val(caller: Caller<'_, StoreData>, instance_idx: i32, name: i64) -> i64 {
    log::trace!(target: "host_call", "get_global_val(instance_idx={instance_idx:?}, name={name:?})");

    let memory = MemoryWrap(caller.data().memory());

    let name = memory.slice_by_val(&caller, name).to_vec();
    let name = core::str::from_utf8(&name).unwrap_or_default();

    let mut host_context = HostContext { caller };

    let res = sandbox_detail::get_global_val(&mut host_context, instance_idx as u32, name);
    let res = res.encode();
    let res_len = res.len() as u32;

    let ptr = host_context
        .allocate_memory(res_len as u32)
        .unwrap()
        .into_value()
        .as_i32()
        .expect("always i32");

    let mut caller = host_context.caller;

    let memory = caller.data().memory();

    memory.write(&mut caller, ptr as usize, &res).unwrap();

    let res = pack_u32_to_i64(ptr as u32, res_len);

    log::trace!(target: "host_call", "get_global_val(..) -> {res:?}");

    res
}

fn get_instance_ptr(caller: Caller<'_, StoreData>, instance_idx: i32) -> i64 {
    log::trace!(target: "host_call", "get_instance_ptr(instance_idx={instance_idx:?})");

    let mut host_context = HostContext { caller };
    let res = sandbox_detail::get_instance_ptr(&mut host_context, instance_idx as u32) as i64;

    log::trace!(target: "host_call", "get_instance_ptr(..) -> {res:?}");

    res
}

fn instance_teardown(caller: Caller<'_, StoreData>, instance_idx: i32) {
    log::trace!(target: "host_call", "instance_teardown(instance_idx={instance_idx:?})");

    let mut host_context = HostContext { caller };
    sandbox_detail::instance_teardown(&mut host_context, instance_idx as u32)
}

fn instantiate(
    caller: Caller<'_, StoreData>,
    dispatch_thunk_id: i32,
    wasm_code: i64,
    raw_env_def: i64,
    state_ptr: i32,
) -> i32 {
    log::trace!(target: "host_call", "instantiate(dispatch_thunk_id={dispatch_thunk_id:?}, wasm_code={wasm_code:?}, raw_env_def={raw_env_def:?}, state_ptr={state_ptr:?})");

    let dispatch_thunk_id = dispatch_thunk_id as u32;

    let memory = MemoryWrap(caller.data().memory());

    let wasm_code = memory.slice_by_val(&caller, wasm_code).to_vec();

    let raw_env_def = memory.slice_by_val(&caller, raw_env_def).to_vec();

    let state_ptr = Pointer::<u8>::new(state_ptr as u32);

    let mut host_context = HostContext { caller };
    let res = sandbox_detail::instantiate(
        &mut host_context,
        dispatch_thunk_id,
        &wasm_code,
        &raw_env_def,
        state_ptr,
        Instantiate::Version2,
    ) as i32;

    log::trace!(target: "host_call", "instantiate(..) -> {res:?}");

    res
}

fn invoke(
    caller: Caller<'_, StoreData>,
    instance_idx: i32,
    function: i64,
    args: i64,
    return_val_ptr: i32,
    return_val_len: i32,
    state_ptr: i32,
) -> i32 {
    log::trace!(target: "host_call", "invoke(instance_idx={instance_idx:?}, function={function:?}, args={args:?}, return_val_ptr={return_val_ptr:?}, return_val_len={return_val_len:?}, state_ptr={state_ptr:?})");

    let instance_idx = instance_idx as u32;

    let memory = MemoryWrap(caller.data().memory());

    let function = memory.slice_by_val(&caller, function).to_vec();
    let function = core::str::from_utf8(&function).unwrap_or_default();

    let args = memory.slice_by_val(&caller, args).to_vec();

    let return_val_ptr = Pointer::<u8>::new(return_val_ptr as u32);

    let return_val_len = return_val_len as u32;

    let state_ptr = Pointer::<u8>::new(state_ptr as u32);

    let mut host_context = HostContext { caller };
    let res = sandbox_detail::invoke(
        &mut host_context,
        instance_idx,
        function,
        &args,
        return_val_ptr,
        return_val_len,
        state_ptr,
    ) as i32;

    log::trace!(target: "host_call", "invoke(..) -> {res:?}");

    res
}

fn memory_get(
    caller: Caller<'_, StoreData>,
    memory_idx: i32,
    offset: i32,
    buff_ptr: i32,
    buff_len: i32,
) -> i32 {
    log::trace!(target: "host_call", "memory_get(memory_idx={memory_idx:?}, offset={offset:?}, buff_ptr={buff_ptr:?}, buff_len={buff_len:?})");

    let buff_ptr = Pointer::new(buff_ptr as u32);

    let mut host_context = HostContext { caller };
    let res = sandbox_detail::memory_get(
        &mut host_context,
        memory_idx as u32,
        offset as u32,
        buff_ptr,
        buff_len as u32,
    ) as i32;

    log::trace!(target: "host_call", "memory_get(..) -> {res:?}");

    res
}

fn memory_grow(caller: Caller<'_, StoreData>, memory_idx: i32, size: i32) -> i32 {
    log::trace!(target: "host_call", "memory_grow(memory_idx={memory_idx:?}, size={size:?})");

    let mut host_context = HostContext { caller };
    let res = sandbox_detail::memory_grow(&mut host_context, memory_idx as u32, size as u32) as i32;

    log::trace!(target: "host_call", "memory_grow(..) -> {res:?}");

    res
}

fn memory_new(caller: Caller<'_, StoreData>, initial: i32, maximum: i32) -> i32 {
    log::trace!(target: "host_call", "memory_new(initial={initial:?}, maximum={maximum:?})");

    let mut host_context = HostContext { caller };
    let res = sandbox_detail::memory_new(&mut host_context, initial as u32, maximum as u32) as i32;

    log::trace!(target: "host_call", "memory_new(..) -> {res:?}");

    res
}

fn memory_set(
    caller: Caller<'_, StoreData>,
    memory_idx: i32,
    offset: i32,
    val_ptr: i32,
    val_len: i32,
) -> i32 {
    log::trace!(target: "host_call", "memory_set(memory_idx={memory_idx:?}, offset={offset:?}, val_ptr={val_ptr:?}, val_len={val_len:?})");

    let val_ptr = Pointer::new(val_ptr as u32);

    let mut host_context = HostContext { caller };
    let res = sandbox_detail::memory_set(
        &mut host_context,
        memory_idx as u32,
        offset as u32,
        val_ptr,
        val_len as u32,
    ) as i32;

    log::trace!(target: "host_call", "memory_set(..) -> {res:?}");

    res
}

fn memory_size(caller: Caller<'_, StoreData>, memory_idx: i32) -> i32 {
    log::trace!(target: "host_call", "memory_size(memory_idx={memory_idx:?})");

    let mut host_context = HostContext { caller };
    let res = sandbox_detail::memory_size(&mut host_context, memory_idx as u32) as i32;

    log::trace!(target: "host_call", "memory_size(..) -> {res:?}");

    res
}

fn memory_teardown(caller: Caller<'_, StoreData>, memory_idx: i32) {
    log::trace!(target: "host_call", "memory_teardown(memory_idx={memory_idx:?})");

    let mut host_context = HostContext { caller };
    sandbox_detail::memory_teardown(&mut host_context, memory_idx as u32)
}

fn set_global_val(caller: Caller<'_, StoreData>, instance_idx: i32, name: i64, value: i64) -> i32 {
    log::trace!(target: "host_call", "set_global_val(instance_idx={instance_idx:?}, name={name:?}, value={value:?})");

    let memory = MemoryWrap(caller.data().memory());

    let name = memory.slice_by_val(&caller, name).to_vec();
    let name = core::str::from_utf8(&name).unwrap_or_default();

    let value = memory.decode_by_val(&caller, value);

    let mut host_context = HostContext { caller };
    let res =
        sandbox_detail::set_global_val(&mut host_context, instance_idx as u32, name, value) as i32;

    log::trace!(target: "host_call", "set_global_val(..) -> {res:?}");

    res
}
