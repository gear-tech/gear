// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::host::{StoreData, context};
use wasmtime::{Caller, Linker};

pub fn link(linker: &mut Linker<StoreData>) -> Result<(), wasmtime::Error> {
    linker.func_wrap("env", "ext_allocator_free_version_1", free)?;
    linker.func_wrap("env", "ext_allocator_malloc_version_1", malloc)?;

    Ok(())
}

fn free(caller: Caller<'_, StoreData>, ptr: u32) {
    context::allocator(caller).deallocate(ptr).unwrap()
}

fn malloc(caller: Caller<'_, StoreData>, size: u32) -> u32 {
    context::allocator(caller).allocate(size).unwrap()
}
