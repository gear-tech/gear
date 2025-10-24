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

use crate::{
    Result,
    host::{api::MemoryWrap, threads},
};
use ethexe_common::db::HashStorageRO;
use gprimitives::H256;
use sp_wasm_interface::StoreData;
use wasmtime::{Caller, Linker};

pub fn link(linker: &mut Linker<StoreData>) -> Result<()> {
    linker.func_wrap("env", "ext_database_read_by_hash_version_1", read_by_hash)?;
    linker.func_wrap("env", "ext_database_write_version_1", write)?;
    linker.func_wrap("env", "ext_get_block_height_version_1", get_block_height)?;
    linker.func_wrap(
        "env",
        "ext_get_block_timestamp_version_1",
        get_block_timestamp,
    )?;
    linker.func_wrap("env", "ext_update_state_hash_version_1", update_state_hash)?;

    Ok(())
}

fn update_state_hash(caller: Caller<'_, StoreData>, program_state_hash_ptr: i32) {
    log::trace!(target: "host_call", "update_state_hash(program_state_hash={program_state_hash_ptr:?})");

    let memory = MemoryWrap(caller.data().memory());

    let hash_slice = memory.slice(&caller, program_state_hash_ptr as usize, size_of::<H256>());
    let program_state_hash = H256::from_slice(hash_slice);

    threads::update_state_hash(program_state_hash);
}

fn read_by_hash(caller: Caller<'_, StoreData>, hash_ptr: i32) -> i64 {
    log::trace!(target: "host_call", "read_by_hash(hash_ptr={hash_ptr:?})");

    let memory = MemoryWrap(caller.data().memory());

    let hash_slice = memory.slice(&caller, hash_ptr as usize, size_of::<H256>());
    let hash = H256::from_slice(hash_slice);

    let maybe_data = threads::with_db(|db| db.read_by_hash(hash));

    let res = maybe_data
        .map(|data| super::allocate_and_write_raw(caller, data).1)
        .unwrap_or(0);

    log::trace!(target: "host_call", "read_by_hash(..) -> {res:?}");

    res
}

fn write(caller: Caller<'_, StoreData>, ptr: i32, len: i32) -> i32 {
    log::trace!(target: "host_call", "write(ptr={ptr:?}, len={len:?})");

    let memory = MemoryWrap(caller.data().memory());

    let data = memory.slice(&caller, ptr as usize, len as usize);

    let hash = threads::with_db(|db| db.write_hash(data));

    let (_caller, res) = super::allocate_and_write(caller, hash);

    // This extracts first bytes (ptr).
    let res = res as i32;

    log::trace!(target: "host_call", "write(..) -> {res:?}");

    res
}

fn get_block_height(_caller: Caller<'_, StoreData>) -> i32 {
    log::trace!(target: "host_call", "get_block_height()");

    let height = threads::chain_head_info().height;

    log::trace!(target: "host_call", "get_block_height() -> {height:?}");

    height as i32
}

fn get_block_timestamp(_caller: Caller<'_, StoreData>) -> i64 {
    log::trace!(target: "host_call", "get_block_timestamp()");

    let timestamp = threads::chain_head_info().timestamp;

    log::trace!(target: "host_call", "get_block_timestamp() -> {timestamp:?}");

    timestamp as i64
}
