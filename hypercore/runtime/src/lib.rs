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

// TODO: Replace `feature = "cargo-clippy"` with `clippy`, once moved repo,
// or at least `hypercore-*` crates, on stable or latest nightly in toml.
#![no_std]

#[cfg(all(
    feature = "wasm",
    not(target_arch = "wasm32"),
    not(feature = "cargo-clippy")
))]
compile_error!("Building runtime with \"-F wasm\", but not for \"wasm32\" target, is forbidden!");

#[cfg(any(not(feature = "wasm"), feature = "cargo-clippy"))]
include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));

#[cfg(all(
    feature = "wasm",
    any(target_arch = "wasm32", feature = "cargo-clippy")
))]
extern crate alloc;

#[cfg(all(
    feature = "wasm",
    any(target_arch = "wasm32", feature = "cargo-clippy")
))]
mod wasm {
    mod api;
    mod interface;

    #[global_allocator]
    pub static ALLOC: dlmalloc_rs::GlobalDlmalloc = dlmalloc_rs::GlobalDlmalloc;

    #[cfg(not(feature = "cargo-clippy"))]
    #[panic_handler]
    fn panic_handler(info: &core::panic::PanicInfo) -> ! {
        log::error!("{info}");
        core::arch::wasm32::unreachable()
    }

    #[no_mangle]
    extern "C" fn _start() {
        interface::logging_ri::RuntimeLogger::init();
    }
}
