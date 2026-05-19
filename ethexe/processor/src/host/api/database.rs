// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::host::{StoreData, context, threads};
use wasmtime::{Caller, Linker};

pub fn link(linker: &mut Linker<StoreData>) -> Result<(), wasmtime::Error> {
    linker.func_wrap("env", "ext_database_read_by_hash_version_1", read_by_hash)?;
    linker.func_wrap("env", "ext_database_write_version_1", write)?;
    linker.func_wrap("env", "ext_update_state_hash_version_1", update_state_hash)?;

    Ok(())
}

fn update_state_hash(caller: Caller<'_, StoreData>, program_state_hash_ptr: u32) {
    log::trace!(target: "host_call", "update_state_hash(program_state_hash={program_state_hash_ptr:?})");

    let program_state_hash = context::memory(caller).decode_by_max_len(program_state_hash_ptr);

    threads::update_state_hash(program_state_hash);
}

fn read_by_hash(mut caller: Caller<'_, StoreData>, hash_ptr: u32) -> i64 {
    log::trace!(target: "host_call", "read_by_hash(hash_ptr={hash_ptr:?})");

    let hash = context::memory(&mut caller).decode_by_max_len(hash_ptr);

    let maybe_data = caller.data().db.read(hash);

    let res = maybe_data
        .map(|data| context::memory(caller).allocate_and_write_val_raw(data))
        .unwrap_or(0);

    log::trace!(target: "host_call", "read_by_hash(..) -> {res:?}");

    res
}

fn write(mut caller: Caller<'_, StoreData>, ptr: u32, len: u32) -> i32 {
    log::trace!(target: "host_call", "write(ptr={ptr:?}, len={len:?})");

    let db = caller.data().db.clone_boxed();
    let memory = context::memory(&mut caller);
    let data = memory.slice(ptr, len).unwrap();
    let hash = db.write(data);

    let res = context::memory(caller).allocate_and_write_val(hash);

    // This extracts first bytes (ptr).
    let res = res as i32;

    log::trace!(target: "host_call", "write(..) -> {res:?}");

    res
}
