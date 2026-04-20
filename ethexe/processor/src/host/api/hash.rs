// This file is part of Gear.
//
// Copyright (C) 2024-2026 Gear Technologies Inc.
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

use crate::host::api::MemoryWrap;
use ethexe_runtime_common::unpack_i64_to_u32;
use sp_wasm_interface::StoreData;
use wasmtime::{Caller, Linker};

pub fn link(linker: &mut Linker<StoreData>) -> Result<(), wasmtime::Error> {
    linker.func_wrap("env", "ext_blake2b_256_v1", blake2b_256)?;
    linker.func_wrap("env", "ext_sha256_v1", sha256)?;
    linker.func_wrap("env", "ext_keccak256_v1", keccak256)?;

    Ok(())
}

/// Read guest memory into an owned Vec so the immutable borrow of
/// `caller` is released before we take a mutable borrow to write the
/// hash output back.
fn copy_in(caller: &Caller<'_, StoreData>, memory: &MemoryWrap, data_packed: i64) -> Vec<u8> {
    let (ptr, len) = unpack_i64_to_u32(data_packed);
    memory.slice(caller, ptr as usize, len as usize).to_vec()
}

fn write_hash(caller: &mut Caller<'_, StoreData>, memory: &MemoryWrap, out_ptr: i32, hash: &[u8]) {
    memory
        .slice_mut(caller, out_ptr as usize, hash.len())
        .copy_from_slice(hash);
}

fn blake2b_256(mut caller: Caller<'_, StoreData>, data_packed: i64, out_ptr: i32) {
    log::trace!(target: "host_call", "blake2b_256(data_packed={data_packed:?}, out_ptr={out_ptr:?})");

    let memory = MemoryWrap(caller.data().memory());
    let data = copy_in(&caller, &memory, data_packed);
    let hash = sp_core::hashing::blake2_256(&data);
    write_hash(&mut caller, &memory, out_ptr, &hash);
}

fn sha256(mut caller: Caller<'_, StoreData>, data_packed: i64, out_ptr: i32) {
    log::trace!(target: "host_call", "sha256(data_packed={data_packed:?}, out_ptr={out_ptr:?})");

    let memory = MemoryWrap(caller.data().memory());
    let data = copy_in(&caller, &memory, data_packed);
    let hash = sp_core::hashing::sha2_256(&data);
    write_hash(&mut caller, &memory, out_ptr, &hash);
}

fn keccak256(mut caller: Caller<'_, StoreData>, data_packed: i64, out_ptr: i32) {
    log::trace!(target: "host_call", "keccak256(data_packed={data_packed:?}, out_ptr={out_ptr:?})");

    let memory = MemoryWrap(caller.data().memory());
    let data = copy_in(&caller, &memory, data_packed);
    let hash = sp_core::hashing::keccak_256(&data);
    write_hash(&mut caller, &memory, out_ptr, &hash);
}
