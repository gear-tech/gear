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

use std::mem;
use wasmtime::{Memory, StoreContext};

pub mod allocator;
pub mod logging;
pub mod sandbox;

pub struct MemoryWrap(Memory);

impl MemoryWrap {
    fn slice_by_val<'a, T: 'a>(
        &self,
        store: impl Into<StoreContext<'a, T>>,
        ptr_len: i64,
    ) -> &'a [u8] {
        let [ptr, len]: [i32; 2] = unsafe { mem::transmute(ptr_len) };

        self.slice(store, ptr as usize, len as usize)
    }

    fn slice<'a, T: 'a>(
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
}
