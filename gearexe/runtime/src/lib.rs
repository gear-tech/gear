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
