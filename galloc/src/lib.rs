// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

#![no_std]
#![doc(html_logo_url = "https://docs.gear.rs/logo.svg")]
#![doc(html_favicon_url = "https://gear-tech.io/favicons/favicon.ico")]

#[cfg(not(target_arch = "wasm32"))]
#[global_allocator]
static mut ALLOC: dlmalloc::GlobalDlmalloc = dlmalloc::GlobalDlmalloc;

#[cfg(target_arch = "wasm32")]
#[global_allocator]
static mut ALLOC: wasm::GlobalGearAlloc = wasm::GlobalGearAlloc;

pub mod prelude;

#[cfg(target_arch = "wasm32")]
mod wasm {
    use core::{
        alloc::{GlobalAlloc, Layout},
        cell::Cell,
        fmt,
        fmt::Write,
        mem::MaybeUninit,
        ptr,
    };
    use dlmalloc::{Allocator as _, Dlmalloc};

    const PAGE_SIZE: usize = 64 * 1024;
    const HEAP_BASE: *mut u8 = {
        unsafe extern "C" {
            static __heap_base: i32;
        }

        unsafe { &__heap_base as *const i32 as *mut u8 }
    };

    pub fn stack_debug(args: fmt::Arguments<'_>) {
        const MAX_BUFFER_SIZE: usize = 128;

        struct StackFmtWriter<'a> {
            buf: &'a mut [MaybeUninit<u8>],
            pos: usize,
        }

        impl fmt::Write for StackFmtWriter<'_> {
            fn write_str(&mut self, s: &str) -> fmt::Result {
                let upper_bound = (self.pos + s.len()).min(MAX_BUFFER_SIZE);
                if let Some(buf) = self.buf.get_mut(self.pos..upper_bound) {
                    let buf = buf as *mut [MaybeUninit<u8>] as *mut [u8];
                    let s = &s.as_bytes()[..buf.len()];

                    // SAFETY: we only write to uninitialized memory
                    unsafe {
                        (*buf).copy_from_slice(s);
                    }

                    self.pos += buf.len();
                }

                Ok(())
            }
        }

        gear_stack_buffer::with_byte_buffer(MAX_BUFFER_SIZE, |buf| {
            let mut writer = StackFmtWriter { buf, pos: 0 };
            writer.write_fmt(args).expect("fmt failed");

            // SAFETY: buffer was initialized via `write_fmt` and limited by `pos`
            unsafe { gsys::gr_debug(writer.buf.as_ptr().cast(), writer.pos as u32) }
        });
    }

    #[inline]
    fn page_to_ptr(page: u16) -> *mut u8 {
        (page as usize * PAGE_SIZE) as *mut u8
    }

    #[inline]
    fn ptr_to_page(ptr: *mut u8) -> u16 {
        (ptr as usize / PAGE_SIZE) as u16
    }

    #[inline]
    fn align_down(ptr: *mut u8) -> *mut u8 {
        (ptr as usize / PAGE_SIZE * PAGE_SIZE) as *mut u8
    }

    fn gr_alloc(size: usize) -> (*mut u8, usize) {
        let pages = size.div_ceil(PAGE_SIZE);
        let size = pages * PAGE_SIZE;

        let page_no = unsafe { gsys::alloc(pages as u32) };
        if page_no == u32::MAX {
            return (ptr::null_mut(), 0);
        }

        let ptr = page_to_ptr(page_no as u16);
        (ptr, size)
    }

    fn gr_free(ptr: *mut u8, size: usize) -> bool {
        let start = ptr_to_page(ptr);
        let end = unsafe { ptr_to_page(ptr.add(size)) - 1 };
        unsafe { gsys::free_range(start as u32, end as u32) == 0 }
    }

    static mut ALLOC: Dlmalloc<GearAlloc> = Dlmalloc::new_with_allocator(GearAlloc::new());

    pub struct GlobalGearAlloc;

    unsafe impl GlobalAlloc for GlobalGearAlloc {
        #[inline]
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            debug("GlobalGearAlloc::alloc");
            let alloc = ptr::addr_of_mut!(ALLOC);
            unsafe {
                let ptr = (*alloc).malloc(layout.size(), layout.align());
                (*alloc).trim(0);
                ptr
            }
        }

        #[inline]
        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            debug("GlobalGearAlloc::dealloc");
            let alloc = ptr::addr_of_mut!(ALLOC);
            unsafe { (*alloc).free(ptr, layout.size(), layout.align()) }
        }

        #[inline]
        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            debug("GlobalGearAlloc::alloc_zeroed");
            let alloc = ptr::addr_of_mut!(ALLOC);
            unsafe { (*alloc).calloc(layout.size(), layout.align()) }
        }

        #[inline]
        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            debug("GlobalGearAlloc::realloc");
            let alloc = ptr::addr_of_mut!(ALLOC);
            unsafe { (*alloc).realloc(ptr, layout.size(), layout.align(), new_size) }
        }
    }

    struct GearAlloc {
        preinstalled_memory: Cell<bool>,
    }

    impl GearAlloc {
        const fn new() -> Self {
            Self {
                preinstalled_memory: Cell::new(false),
            }
        }

        fn init_preinstalled_memory(&self, size: usize) -> Option<(*mut u8, usize)> {
            if self.preinstalled_memory.get() {
                return None;
            }

            self.preinstalled_memory.set(true);

            let remaining_space = align_down(HEAP_BASE) as usize + PAGE_SIZE - HEAP_BASE as usize;
            if remaining_space == 0 {
                // no preinstalled memory is available
                None
            } else if size <= remaining_space {
                // no additional allocation is needed
                Some((HEAP_BASE, remaining_space))
            } else {
                // proceed to additional allocation
                let (ptr, size) = gr_alloc(size - remaining_space);

                unsafe {
                    debug_assert_eq!(ptr.sub(remaining_space), HEAP_BASE);
                }

                let size = size + remaining_space;
                Some((HEAP_BASE, size))
            }
        }
    }

    fn debug(data: &str) {
        unsafe { gsys::gr_debug(data.as_ptr(), data.len() as u32) }
    }

    unsafe impl dlmalloc::Allocator for GearAlloc {
        fn alloc(&self, size: usize) -> (*mut u8, usize, u32) {
            stack_debug(format_args!("GearAlloc::alloc({size})"));

            if let Some((ptr, size)) = self.init_preinstalled_memory(size) {
                debug("GearAlloc::init_preinstalled_memory()");
                return (ptr, size, 0);
            }

            let (ptr, size) = gr_alloc(size);
            (ptr, size, 0)
        }

        fn remap(
            &self,
            _ptr: *mut u8,
            _oldsize: usize,
            _newsize: usize,
            _can_move: bool,
        ) -> *mut u8 {
            debug("GearAlloc::remap");
            ptr::null_mut()
        }

        fn free_part(&self, ptr: *mut u8, oldsize: usize, newsize: usize) -> bool {
            stack_debug(format_args!(
                "GearAlloc::free_part({ptr:?}, {oldsize}, {newsize})"
            ));

            if oldsize == newsize {
                return true;
            }

            unsafe { gr_free(ptr.add(newsize), oldsize - newsize) }
        }

        fn free(&self, ptr: *mut u8, size: usize) -> bool {
            debug("GearAlloc::free");
            gr_free(ptr, size)
        }

        fn can_release_part(&self, _flags: u32) -> bool {
            true
        }

        fn allocates_zeros(&self) -> bool {
            true
        }

        fn page_size(&self) -> usize {
            PAGE_SIZE
        }
    }
}
