// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::host::context::HostContext;
use sp_wasm_interface::{FunctionContext, IntoValue, Pointer, StoreData};
use wasmtime::{Caller, Linker};

pub fn link(linker: &mut Linker<StoreData>) -> Result<(), wasmtime::Error> {
    linker.func_wrap("env", "ext_allocator_free_version_1", free)?;
    linker.func_wrap("env", "ext_allocator_malloc_version_1", malloc)?;

    Ok(())
}

fn free(caller: Caller<'_, StoreData>, ptr: i32) {
    let mut host_context = HostContext { caller };

    host_context
        .deallocate_memory(Pointer::<u8>::new(ptr as _))
        .unwrap();
}

fn malloc(caller: Caller<'_, StoreData>, size: i32) -> i32 {
    let mut host_context = HostContext { caller };

    host_context
        .allocate_memory(size as _)
        .unwrap()
        .into_value()
        .as_i32()
        .unwrap()
}
