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

use super::context::HostContext;
use ethexe_runtime_common::{pack_u32_to_i64, unpack_i64_to_u32};
use parity_scale_codec::{Decode, Encode};
use sp_wasm_interface::{FunctionContext as _, IntoValue as _, StoreData};
use wasmtime::{Caller, Memory, StoreContext, StoreContextMut};

pub mod allocator;
pub mod database;
pub mod lazy_pages;
pub mod logging;
pub mod sandbox;

pub struct MemoryWrap(Memory);

// TODO: return results for mem accesses.
impl MemoryWrap {
    pub fn decode_by_val<'a, T: 'a, D: Decode>(
        &self,
        store: impl Into<StoreContext<'a, T>>,
        ptr_len: i64,
    ) -> D {
        let mut slice = self.slice_by_val(store, ptr_len);

        D::decode(&mut slice).unwrap()
    }

    #[allow(unused)]
    pub fn decode<'a, T: 'a, D: Decode>(
        &self,
        store: impl Into<StoreContext<'a, T>>,
        ptr: usize,
        len: usize,
    ) -> D {
        let mut slice = self.slice(store, ptr, len);

        D::decode(&mut slice).unwrap()
    }

    pub fn slice_by_val<'a, T: 'a>(
        &self,
        store: impl Into<StoreContext<'a, T>>,
        ptr_len: i64,
    ) -> &'a [u8] {
        let (ptr, len) = unpack_i64_to_u32(ptr_len);

        self.slice(store, ptr as usize, len as usize)
    }

    pub fn slice<'a, T: 'a>(
        &self,
        store: impl Into<StoreContext<'a, T>>,
        ptr: usize,
        len: usize,
    ) -> &'a [u8] {
        self.0
            .data(store)
            .get(ptr..)
            .and_then(|s| s.get(..len))
            .unwrap()
    }

    pub fn slice_mut<'a, T: 'a>(
        &self,
        store: impl Into<StoreContextMut<'a, T>>,
        ptr: usize,
        len: usize,
    ) -> &'a mut [u8] {
        self.0
            .data_mut(store)
            .get_mut(ptr..)
            .and_then(|s| s.get_mut(..len))
            .unwrap()
    }
}

pub fn allocate_and_write(
    caller: Caller<'_, StoreData>,
    data: impl Encode,
) -> (Caller<'_, StoreData>, i64) {
    allocate_and_write_raw(caller, data.encode())
}

pub fn allocate_and_write_raw(
    caller: Caller<'_, StoreData>,
    data: impl AsRef<[u8]>,
) -> (Caller<'_, StoreData>, i64) {
    let data = data.as_ref();
    let len = data.len();

    let mut host_context = HostContext { caller };

    let ptr = host_context
        .allocate_memory(len as u32)
        .unwrap()
        .into_value()
        .as_i32()
        .expect("always i32");

    let mut caller = host_context.caller;

    let memory = caller.data().memory();

    memory.write(&mut caller, ptr as usize, data).unwrap();

    let res = pack_u32_to_i64(ptr as u32, len as u32);

    (caller, res)
}
