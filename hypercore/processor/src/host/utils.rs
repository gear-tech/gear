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

use wasmtime::{Caller, Memory};

pub fn mem_of<T>(caller: &mut Caller<'_, T>) -> Memory {
    caller.get_export("memory").unwrap().into_memory().unwrap()
}

pub fn read_ri_slice<T>(memory: &Memory, store: &mut Caller<'_, T>, data: i64) -> Vec<u8> {
    let data_bytes = data.to_le_bytes();

    let mut ptr_bytes = [0; 4];
    ptr_bytes.copy_from_slice(&data_bytes[..4]);
    let ptr = i32::from_le_bytes(ptr_bytes);

    let mut len_bytes = [0; 4];
    len_bytes.copy_from_slice(&data_bytes[4..]);
    let len = i32::from_le_bytes(len_bytes);

    let mut buffer = vec![0; len as usize];

    memory.read(store, ptr as usize, &mut buffer).unwrap();

    buffer
}
