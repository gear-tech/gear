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

use codec::{Decode, Encode};
use gear_backend_common::{
    lazy_pages::{ChargeForPages, GlobalsConfig, LazyPagesWeights, Status},
    memory::OutOfMemoryAccessError,
};
use gear_core::memory::{GearPage, HostPointer, PageU32Size, WasmPage};
use sp_runtime_interface::{
    pass_by::{Codec, Inner, PassBy, PassByInner},
    runtime_interface,
};

static_assertions::const_assert!(
    core::mem::size_of::<HostPointer>() >= core::mem::size_of::<usize>()
);

#[cfg(feature = "std")]
use gear_lazy_pages as lazy_pages;

pub use sp_std::{convert::TryFrom, result::Result, vec::Vec};

/// Use it to safely transfer wasm page from wasm runtime to native.
pub struct WasmPageFfiWrapper(u32);

impl From<WasmPage> for WasmPageFfiWrapper {
    fn from(value: WasmPage) -> Self {
        Self(value.raw())
    }
}

impl From<WasmPageFfiWrapper> for WasmPage {
    fn from(val: WasmPageFfiWrapper) -> Self {
        // Safe because we can make wrapper only from `WasmPage`.
        unsafe { WasmPage::new_unchecked(val.0) }
    }
}

impl PassBy for WasmPageFfiWrapper {
    type PassBy = Inner<Self, u32>;
}

impl PassByInner for WasmPageFfiWrapper {
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

#[derive(Debug, Clone, Encode, Decode)]
pub struct LazyPagesProgramContext {
    /// Wasm program memory addr.
    pub wasm_mem_addr: Option<HostPointer>,
    /// Wasm program memory size.
    pub wasm_mem_size: WasmPage,
    /// Wasm program stack end page.
    pub stack_end: Option<WasmPage>,
    /// Wasm program id.
    pub program_id: Vec<u8>,
    /// Globals config to access globals inside lazy-pages.
    pub globals_config: GlobalsConfig,
    /// Lazy-pages access weights.
    pub lazy_pages_weights: LazyPagesWeights,
}

impl PassBy for LazyPagesProgramContext {
    type PassBy = Codec<LazyPagesProgramContext>;
}

/// Runtime interface for gear node and runtime.
/// Note: name is expanded as gear_ri
#[runtime_interface]
pub trait GearRI {
    fn pre_process_memory_accesses(
        reads: &[(u32, u32)],
        writes: &[(u32, u32)],
    ) -> Result<ChargeForPages, OutOfMemoryAccessError> {
        let reads = reads.iter().copied().map(Into::into).collect::<Vec<_>>();
        let writes = writes.iter().copied().map(Into::into).collect::<Vec<_>>();
        lazy_pages::pre_process_memory_accesses(&reads, &writes)
    }

    fn get_lazy_pages_status() -> Option<Status> {
        lazy_pages::get_status()
    }

    /// Init lazy-pages.
    /// Returns whether initialization was successful.
    #[version(2)]
    fn init_lazy_pages(pages_final_prefix: [u8; 32]) -> bool {
        use lazy_pages::LazyPagesVersion;

        lazy_pages::init(LazyPagesVersion::Version1, pages_final_prefix.into())
    }

    /// Init lazy pages context for current program.
    /// Panic if some goes wrong during initialization.
    #[version(2)]
    fn init_lazy_pages_for_program(ctx: LazyPagesProgramContext) {
        let wasm_mem_addr = ctx.wasm_mem_addr.map(|addr| {
            usize::try_from(addr)
                .unwrap_or_else(|err| unreachable!("Cannot cast wasm mem addr to `usize`: {}", err))
        });

        lazy_pages::initialize_for_program(
            wasm_mem_addr,
            ctx.wasm_mem_size,
            ctx.stack_end,
            ctx.program_id,
            Some(ctx.globals_config),
            ctx.lazy_pages_weights,
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
    fn set_wasm_mem_size(wasm_mem_size: WasmPageFfiWrapper) {
        lazy_pages::set_wasm_mem_size(wasm_mem_size.into())
            .map_err(|e| e.to_string())
            .expect("Cannot set new wasm memory size");
    }

    #[version(2)]
    fn get_released_pages() -> Vec<GearPage> {
        lazy_pages::get_released_pages()
    }

    // Deprecated runtime interface functions.

    #[deprecated]
    fn init_lazy_pages_for_program(
        wasm_mem_addr: Option<HostPointer>,
        wasm_mem_size_in_pages: u32,
        stack_end: Option<u32>,
        program_id: Vec<u8>,
    ) {
        let wasm_mem_size =
            WasmPage::new(wasm_mem_size_in_pages).expect("Unexpected wasm mem size number");
        let stack_end =
            stack_end.map(|page| WasmPage::new(page).expect("Unexpected wasm stack end addr"));
        let wasm_mem_addr = wasm_mem_addr
            .map(|addr| usize::try_from(addr).expect("Cannot cast wasm mem addr to `usize`"));

        lazy_pages::initialize_for_program(
            wasm_mem_addr,
            wasm_mem_size,
            stack_end,
            program_id,
            None,
            Default::default(),
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
        let size = WasmPage::new(size_in_wasm_pages).expect("Unexpected wasm memory size");
        lazy_pages::set_wasm_mem_size(size)
            .map_err(|e| e.to_string())
            .expect("Cannot set new wasm memory size");
    }

    #[deprecated]
    fn init_lazy_pages() -> bool {
        use lazy_pages::LazyPagesVersion;

        lazy_pages::init(LazyPagesVersion::Version1, vec![])
    }
}
