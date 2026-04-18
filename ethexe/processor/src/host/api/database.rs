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

use crate::host::{StoreData, store, threads};
use gprimitives::H256;
use wasmtime::{Caller, Linker};

pub fn link(linker: &mut Linker<StoreData>) -> Result<(), wasmtime::Error> {
    linker.func_wrap("env", "ext_database_read_by_hash_version_1", read_by_hash)?;
    linker.func_wrap("env", "ext_database_write_version_1", write)?;
    linker.func_wrap("env", "ext_update_state_hash_version_1", update_state_hash)?;

    Ok(())
}

fn update_state_hash(caller: Caller<'_, StoreData>, program_state_hash_ptr: i32) {
    log::trace!(target: "host_call", "update_state_hash(program_state_hash={program_state_hash_ptr:?})");

    let program_state_hash =
        store::memory(caller).decode(program_state_hash_ptr as usize, size_of::<H256>());

    threads::update_state_hash(program_state_hash);
}

fn read_by_hash(mut caller: Caller<'_, StoreData>, hash_ptr: i32) -> i64 {
    log::trace!(target: "host_call", "read_by_hash(hash_ptr={hash_ptr:?})");

    let hash = store::memory(&mut caller).decode(hash_ptr as usize, size_of::<H256>());

    let maybe_data = threads::with_db(|db| db.read(hash));

    let res = maybe_data
        .map(|data| super::allocate_and_write_raw(caller, data))
        .unwrap_or(0);

    log::trace!(target: "host_call", "read_by_hash(..) -> {res:?}");

    res
}

fn write(mut caller: Caller<'_, StoreData>, ptr: i32, len: i32) -> i32 {
    log::trace!(target: "host_call", "write(ptr={ptr:?}, len={len:?})");

    let memory = store::memory(&mut caller);
    let data = memory.slice(ptr as usize, len as usize);
    let hash = threads::with_db(|db| db.write(data));

    let res = super::allocate_and_write(caller, hash);

    // This extracts first bytes (ptr).
    let res = res as i32;

    log::trace!(target: "host_call", "write(..) -> {res:?}");

    res
}
