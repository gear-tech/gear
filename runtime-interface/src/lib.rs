// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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

//! Runtime interface for gear node

#![allow(useless_deprecated, deprecated)]
#![cfg_attr(not(feature = "std"), no_std)]

use gear_core::{
    lazy_pages::{AccessError, GlobalsCtx, Status},
    memory::{HostPointer, PageNumber, PageU32Size, WasmPageNumber},
};
use sp_runtime_interface::{
    pass_by::{Inner, PassBy, PassByInner},
    runtime_interface,
};

static_assertions::const_assert!(
    core::mem::size_of::<HostPointer>() >= core::mem::size_of::<usize>()
);

#[cfg(feature = "std")]
use gear_lazy_pages as lazy_pages;

pub use sp_std::{convert::TryFrom, result::Result, vec::Vec};

/// Use it to safely transfer wasm page from wasm runtime to native.
pub struct WasmPageFFIWrapper(u32);

impl From<WasmPageNumber> for WasmPageFFIWrapper {
    fn from(value: WasmPageNumber) -> Self {
        Self(value.raw())
    }
}

impl From<WasmPageFFIWrapper> for WasmPageNumber {
    fn from(val: WasmPageFFIWrapper) -> Self {
        // Safe because we can make wrapper only from `WasmPageNumber`.
        unsafe { WasmPageNumber::new_unchecked(val.0) }
    }
}

impl PassBy for WasmPageFFIWrapper {
    type PassBy = Inner<Self, u32>;
}

impl PassByInner for WasmPageFFIWrapper {
    type Inner = u32;

    fn into_inner(self) -> Self::Inner {
        self.0
    }

    fn inner(&self) -> &Self::Inner {
        &self.0
    }

    fn from_inner(inner: Self::Inner) -> Self {
        Self(inner)
    }
}

/// Runtime interface for gear node and runtime.
/// Note: name is expanded as gear_ri
#[runtime_interface]
pub trait GearRI {
    fn pre_process_memory_accesses(
        reads: &[(u32, u32)],
        writes: &[(u32, u32)],
    ) -> Result<(), AccessError> {
        lazy_pages::pre_process_memory_accesses(reads, writes, None, None)
    }

    fn get_lazy_pages_status() -> Option<Status> {
        lazy_pages::get_status()
    }

    /// Init lazy-pages.
    /// Returns whether initialization was successful.
    fn init_lazy_pages() -> bool {
        use lazy_pages::LazyPagesVersion;

        lazy_pages::init(LazyPagesVersion::Version1)
    }

    /// Init lazy pages context for current program.
    /// Panic if some goes wrong during initialization.
    #[version(2)]
    fn init_lazy_pages_for_program(
        wasm_mem_addr: Option<HostPointer>,
        wasm_mem_size: WasmPageFFIWrapper,
        stack_end_page: Option<WasmPageNumber>,
        program_prefix: Vec<u8>,
        globals_ctx: Option<GlobalsCtx>,
    ) {
        let wasm_mem_size = wasm_mem_size.into();

        // `as usize` is safe, because of const assert above.
        let wasm_mem_addr = wasm_mem_addr.map(|addr| addr as usize);

        lazy_pages::initialize_for_program(
            wasm_mem_addr,
            wasm_mem_size,
            stack_end_page,
            program_prefix,
            globals_ctx,
        )
        .map_err(|e| e.to_string())
        .expect("Cannot initialize lazy pages for current program");
    }

    /// Mprotect all wasm mem buffer except released pages.
    /// If `protect` argument is true then restrict all accesses to pages,
    /// else allows read and write accesses.
    fn mprotect_lazy_pages(protect: bool) {
        if protect {
            lazy_pages::set_lazy_pages_protection()
        } else {
            lazy_pages::unset_lazy_pages_protection()
        }
        .map_err(|err| err.to_string())
        .expect("Cannot set/unset mprotection for lazy pages");
    }

    fn set_wasm_mem_begin_addr(addr: HostPointer) {
        // `as usize` is safe, because of const assert above.
        gear_lazy_pages::set_wasm_mem_begin_addr(addr as usize)
            .map_err(|e| e.to_string())
            .expect("Cannot set new wasm addr");
    }

    #[version(2)]
    fn set_wasm_mem_size(wasm_mem_size: WasmPageFFIWrapper) {
        lazy_pages::set_wasm_mem_size(wasm_mem_size.into())
            .map_err(|e| e.to_string())
            .expect("Cannot set new wasm memory size");
    }

    #[version(2)]
    fn get_released_pages() -> Vec<PageNumber> {
        lazy_pages::get_released_pages()
    }

    // Deprecated runtime interface functions.

    #[deprecated]
    fn init_lazy_pages_for_program(
        wasm_mem_addr: Option<HostPointer>,
        wasm_mem_size_in_pages: u32,
        stack_end_page: Option<u32>,
        program_prefix: Vec<u8>,
    ) {
        let wasm_mem_size =
            WasmPageNumber::new(wasm_mem_size_in_pages).expect("Unexpected wasm mem size number");
        let stack_end_page = stack_end_page
            .map(|page| WasmPageNumber::new(page).expect("Unexpected wasm stack end addr"));

        let wasm_mem_addr = wasm_mem_addr
            .map(|addr| usize::try_from(addr).expect("Cannot cast wasm mem addr to `usize`"));
        lazy_pages::initialize_for_program(
            wasm_mem_addr,
            wasm_mem_size,
            stack_end_page,
            program_prefix,
            None,
        )
        .map_err(|e| e.to_string())
        .expect("Cannot initialize lazy pages for current program");
    }

    #[deprecated]
    fn get_released_pages() -> Vec<u32> {
        // TODO: (issue #1731) pass result thru safe wrapper
        lazy_pages::get_released_pages()
            .into_iter()
            .map(|p| p.raw())
            .collect()
    }

    #[deprecated]
    fn set_wasm_mem_size(size_in_wasm_pages: u32) {
        let size = WasmPageNumber::new(size_in_wasm_pages).expect("Unexpected wasm memory size");
        lazy_pages::set_wasm_mem_size(size)
            .map_err(|e| e.to_string())
            .expect("Cannot set new wasm memory size");
    }
}
