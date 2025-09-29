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

use crate::wasm::interface;
use core::alloc::{GlobalAlloc, Layout};

interface::declare! {
    pub(super) fn ext_allocator_free_version_1(ptr: *mut u8);
    pub(super) fn ext_allocator_malloc_version_1(size: i32) -> *mut u8;
}

pub fn free(ptr: *mut u8) {
    unsafe { sys::ext_allocator_free_version_1(ptr) }
}

pub fn malloc(size: usize) -> *mut u8 {
    unsafe { sys::ext_allocator_malloc_version_1(size as _) }
}

pub struct RuntimeAllocator;

unsafe impl GlobalAlloc for RuntimeAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        malloc(layout.size())
    }

    unsafe fn dealloc(&self, ptr: *mut u8, _: Layout) {
        free(ptr)
    }
}
