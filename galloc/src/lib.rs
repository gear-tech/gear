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
static mut ALLOC: wasm::GlobalGearTalc = wasm::GlobalGearTalc::new();

pub mod prelude;

#[cfg(target_arch = "wasm32")]
mod wasm {
    use core::{
        alloc::{GlobalAlloc, Layout},
        cell::OnceCell,
        fmt,
        fmt::Write,
        mem::MaybeUninit,
    };
    use dlmalloc::Allocator as _;
    use talc::{OomHandler, Span, Talc, Talck, locking::AssumeUnlockable};

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

    pub struct GlobalGearTalc {
        inner: Talck<AssumeUnlockable, GearOomHandler>,
        preinstalled_memory_init: OnceCell<()>,
    }

    impl GlobalGearTalc {
        pub const fn new() -> Self {
            Self {
                inner: Talc::new(GearOomHandler::new()).lock(),
                preinstalled_memory_init: OnceCell::new(),
            }
        }

        #[inline]
        fn page_to_ptr(page: u32) -> *mut u8 {
            (page as usize * GearOomHandler::PAGE_SIZE) as *mut u8
        }

        #[inline]
        fn ptr_to_page(ptr: *mut u8) -> u32 {
            (ptr as usize / GearOomHandler::PAGE_SIZE) as u32
        }

        fn init_preinstalled_memory(&self) {
            unsafe extern "C" {
                static __heap_base: i32;
            }

            self.preinstalled_memory_init.get_or_init(|| {
                let heap_base = unsafe { &__heap_base as *const i32 as *mut u8 };
                let page_begin = Self::ptr_to_page(heap_base);
                let page_begin = Self::page_to_ptr(page_begin);
                let remaining_space =
                    page_begin as usize + GearOomHandler::PAGE_SIZE - heap_base as usize;
                let heap_acme = unsafe { heap_base.add(remaining_space) };

                let mut inner = self.inner.lock();
                if page_begin == heap_base {
                    // no additional memory is available
                } else if let Ok(heap) = unsafe { inner.claim(Span::new(heap_base, heap_acme)) } {
                    inner.oom_handler.prev_heap = heap;
                }
            });
        }
    }

    unsafe impl GlobalAlloc for GlobalGearTalc {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            self.init_preinstalled_memory();
            unsafe { self.inner.alloc(layout) }
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            stack_debug(format_args!("dealloc({:?}, {:?})", ptr, layout));
            unsafe { self.inner.dealloc(ptr, layout) }
        }

        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            self.init_preinstalled_memory();
            unsafe { self.inner.alloc_zeroed(layout) }
        }

        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            unsafe { self.inner.realloc(ptr, layout, new_size) }
        }
    }

    pub struct GearOomHandler {
        prev_heap: Span,
    }

    impl GearOomHandler {
        const PAGE_SIZE: usize = 1024 * 64;

        const fn new() -> Self {
            Self {
                prev_heap: Span::empty(),
            }
        }
    }

    impl OomHandler for GearOomHandler {
        fn handle_oom(talc: &mut Talc<Self>, layout: Layout) -> Result<(), ()> {
            stack_debug(format_args!("handle_oom({:?})", layout));

            let required = layout.size();
            let mut delta_pages = required.div_ceil(Self::PAGE_SIZE);

            let prev = 'prev: {
                // This performs a scan, trying to find a smaller possible
                // growth if the previous one was unsuccessful. Return
                // any successful allocated to memory.
                // If not quite enough, talc will invoke handle_oom again.

                // if we're about to fail because of allocation failure
                // we may as well try as hard as we can to probe what's permissable
                // which can be done with a log2(n)-ish algorithm
                // (factoring in repeated called to handle_oom)
                while delta_pages != 0 {
                    // use `core::arch::wasm` instead once it doesn't
                    // require the unstable feature wasm_simd64?
                    let result = unsafe { gsys::alloc(delta_pages as u32) };

                    if result != u32::MAX {
                        break 'prev result as usize;
                    } else {
                        delta_pages >>= 1;
                        continue;
                    }
                }

                return Err(());
            };

            let prev_heap_acme = (prev * Self::PAGE_SIZE) as *mut u8;
            let new_heap_acme = prev_heap_acme.wrapping_add(delta_pages * Self::PAGE_SIZE);

            // try to get base & acme, which will fail if prev_heap is empty
            // otherwise the allocator has been initialized previously
            if let Some((prev_base, prev_acme)) = talc.oom_handler.prev_heap.get_base_acme()
                && prev_acme == prev_heap_acme
            {
                talc.oom_handler.prev_heap = unsafe {
                    talc.extend(
                        talc.oom_handler.prev_heap,
                        Span::new(prev_base, new_heap_acme),
                    )
                };

                return Ok(());
            }

            talc.oom_handler.prev_heap = unsafe {
                // delta_pages is always greater than zero
                // thus one page is enough space for metadata
                // therefore we can unwrap the result
                talc.claim(Span::new(prev_heap_acme, new_heap_acme))
                    .unwrap()
            };

            Ok(())
        }
    }
}
