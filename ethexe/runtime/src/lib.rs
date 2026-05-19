// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "std")]
include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));

extern crate alloc;

mod wasm {
    use interface::{allocator_ri::RuntimeAllocator, logging_ri::RuntimeLogger};

    mod api;
    mod interface;
    mod storage;

    #[cfg_attr(target_arch = "wasm32", global_allocator)]
    #[cfg_attr(not(target_arch = "wasm32"), allow(unused))]
    static ALLOCATOR: RuntimeAllocator = RuntimeAllocator;

    #[cfg(target_arch = "wasm32")]
    #[unsafe(no_mangle)]
    extern "C" fn _start() {
        __start()
    }

    #[cfg_attr(not(target_arch = "wasm32"), allow(unused))]
    fn __start() {
        RuntimeLogger::init();
    }

    #[cfg(target_arch = "wasm32")]
    #[panic_handler]
    fn panic_handler(info: &core::panic::PanicInfo) -> ! {
        log::error!("{info}");
        core::arch::wasm32::unreachable()
    }
}
