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

use crate::host::{StoreData, store};
use ethexe_runtime_common::pack_u32_to_i64;
use parity_scale_codec::Encode;
use wasmtime::StoreContextMut;

pub mod allocator;
pub mod database;
pub mod lazy_pages;
pub mod logging;
pub mod promise;
pub mod sandbox;

pub fn allocate_and_write<'a>(
    caller: impl Into<StoreContextMut<'a, StoreData>>,
    data: impl Encode,
) -> i64 {
    allocate_and_write_raw(caller, data.encode())
}

pub fn allocate_and_write_raw<'a>(
    caller: impl Into<StoreContextMut<'a, StoreData>>,
    data: impl AsRef<[u8]>,
) -> i64 {
    let mut caller = caller.into();
    let data = data.as_ref();
    let len = data.len();

    let ptr: u32 = store::allocator(&mut caller).allocate(len as u32).unwrap();
    let memory = caller.data().memory();
    memory.write(&mut caller, ptr as usize, data).unwrap();

    pack_u32_to_i64(ptr, len as u32)
}
