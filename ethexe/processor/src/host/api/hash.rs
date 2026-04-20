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

    Ok(())
}

fn blake2b_256(mut caller: Caller<'_, StoreData>, data_packed: i64, out_ptr: i32) {
    log::trace!(target: "host_call", "blake2b_256(data_packed={data_packed:?}, out_ptr={out_ptr:?})");

    let memory = MemoryWrap(caller.data().memory());

    let (ptr, len) = unpack_i64_to_u32(data_packed);
    // Copy into an owned buffer to release the immutable borrow of `caller`
    // before taking the mutable borrow for `slice_mut` below.
    let data = memory.slice(&caller, ptr as usize, len as usize).to_vec();

    let hash = sp_core::hashing::blake2_256(&data);

    memory
        .slice_mut(&mut caller, out_ptr as usize, 32)
        .copy_from_slice(&hash);
}
