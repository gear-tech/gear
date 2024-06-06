// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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
use anyhow::Result;
use gear_runtime_interface::{detail, Instantiate};
use sp_wasm_interface::{Pointer, StoreData};
use wasmtime::{Caller, Linker};

pub fn link(linker: &mut Linker<StoreData>) -> Result<()> {
    linker.func_wrap(
        "env",
        "ext_sandbox_instance_teardown_version_1",
        instance_teardown,
    )?;
    linker.func_wrap("env", "ext_sandbox_instantiate_version_2", instantiate)?;
    linker.func_wrap("env", "ext_sandbox_invoke_version_1", invoke)?;
    linker.func_wrap(
        "env",
        "ext_sandbox_memory_teardown_version_1",
        memory_teardown,
    )?;
    linker.func_wrap("env", "ext_sandbox_memory_new_version_1", memory_new)?;

    Ok(())
}

fn instance_teardown(caller: Caller<'_, StoreData>, instance_idx: i32) {
    log::trace!(target: "host_call", "instance_teardown(instance_idx={instance_idx:?})");

    let mut host_context = HostContext { caller };
    detail::instance_teardown(&mut host_context, instance_idx as u32)
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
    let res = detail::instantiate(
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
    log::trace!(target: "host_call", "invoke(instance_idx={instance_idx:?}, function={function:?}, args={args:?}, return_val_ptr={return_val_ptr:?}, return_val_len={return_val_ptr:?}, state_ptr={state_ptr:?})");
    let instance_idx = instance_idx as u32;

    let memory = MemoryWrap(caller.data().memory());

    let function = memory.slice_by_val(&caller, function).to_vec();
    let function = core::str::from_utf8(&function).unwrap_or_default();

    let args = memory.slice_by_val(&caller, args).to_vec();

    let return_val_ptr = Pointer::<u8>::new(return_val_ptr as u32);

    let return_val_len = return_val_len as u32;

    let state_ptr = Pointer::<u8>::new(state_ptr as u32);

    let mut host_context = HostContext { caller };
    let res = detail::invoke(
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

fn memory_teardown(caller: Caller<'_, StoreData>, memory_idx: i32) {
    log::trace!(target: "host_call", "memory_teardown(memory_idx={memory_idx:?})");

    let mut host_context = HostContext { caller };
    detail::memory_teardown(&mut host_context, memory_idx as u32)
}

fn memory_new(caller: Caller<'_, StoreData>, initial: i32, maximum: i32) -> i32 {
    log::trace!(target: "host_call", "memory_new(initial={initial:?}, maximum={maximum:?})");

    let mut host_context = HostContext { caller };
    let res = detail::memory_new(&mut host_context, initial as u32, maximum as u32) as i32;

    log::trace!(target: "host_call", "memory_new(..) -> {res:?}");

    res
}
