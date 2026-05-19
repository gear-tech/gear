// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

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
