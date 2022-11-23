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
    lazy_pages::{GlobalsCtx, Status},
    memory::HostPointer,
};
use sp_runtime_interface::runtime_interface;

static_assertions::const_assert!(
    core::mem::size_of::<HostPointer>() >= core::mem::size_of::<usize>()
);

#[cfg(feature = "std")]
use gear_lazy_pages as lazy_pages;

pub use sp_std::{convert::TryFrom, result::Result, vec::Vec};

/// Runtime interface for gear node and runtime.
/// Note: name is expanded as gear_ri
#[runtime_interface]
pub trait GearRI {
    fn get_lazy_pages_status() -> Option<Status> {
        lazy_pages::get_status()
    }

    /// Init lazy-pages.
    /// Returns whether initialization was successful.
    fn init_lazy_pages() -> bool {
        use lazy_pages::{DefaultUserSignalHandler, LazyPagesVersion};

        lazy_pages::init::<DefaultUserSignalHandler>(LazyPagesVersion::Version1)
    }

    #[deprecated]
    fn init_lazy_pages_for_program(
        wasm_mem_addr: Option<HostPointer>,
        wasm_mem_size_in_pages: u32,
        stack_end_page: Option<u32>,
        program_prefix: Vec<u8>,
    ) {
        let wasm_mem_size = wasm_mem_size_in_pages.into();
        let stack_end_page = stack_end_page.map(Into::into);

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

    /// Init lazy pages context for current program.
    /// Panic if some goes wrong during initialization.
    #[version(2)]
    fn init_lazy_pages_for_program(
        wasm_mem_addr: Option<HostPointer>,
        wasm_mem_size_in_pages: u32,
        stack_end_page: Option<u32>,
        program_prefix: Vec<u8>,
        globals_ctx: Option<GlobalsCtx>,
    ) {
        let wasm_mem_size = wasm_mem_size_in_pages.into();
        let stack_end_page = stack_end_page.map(Into::into);

        let wasm_mem_addr = wasm_mem_addr
            .map(|addr| usize::try_from(addr).expect("Cannot cast wasm mem addr to `usize`"));
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
        gear_lazy_pages::set_wasm_mem_begin_addr(addr as usize)
            .map_err(|e| e.to_string())
            .expect("Cannot set new wasm addr");
    }

    fn set_wasm_mem_size(size_in_wasm_pages: u32) {
        lazy_pages::set_wasm_mem_size(size_in_wasm_pages.into())
            .map_err(|e| e.to_string())
            .expect("Cannot set new wasm memory size");
    }

    fn get_released_pages() -> Vec<u32> {
        lazy_pages::get_released_pages()
            .into_iter()
            .map(|p| p.0)
            .collect()
    }
}
