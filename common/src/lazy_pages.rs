// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
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

//! Lazy pages support runtime functions

use crate::Origin;
use alloc::string::ToString;
use core::fmt;
use gear_core::{
    ids::ProgramId,
    memory::{HostPointer, Memory, PageNumber, WasmPageNumber},
};
use gear_runtime_interface::{gear_ri, RIError};
use sp_std::vec::Vec;

#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display)]
pub enum Error {
    #[display(fmt = "{}", _0)]
    RIError(RIError),
    #[display(fmt = "Wasm memory buffer is undefined after wasm memory relocation")]
    WasmMemBufferIsUndefined,
}

impl From<RIError> for Error {
    fn from(err: RIError) -> Self {
        Self::RIError(err)
    }
}

fn mprotect_lazy_pages(mem: &impl Memory, protect: bool) -> Result<(), Error> {
    if mem.get_buffer_host_addr().is_none() {
        return Ok(());
    }

    // Cannot panic, unless OS has some problems with pages protection.
    gear_ri::mprotect_lazy_pages(protect).expect("Cannot set/unset protection for wasm mem");

    Ok(())
}

/// Try to enable and initialize lazy pages env
pub fn try_to_enable_lazy_pages() -> bool {
    if !gear_ri::init_lazy_pages() {
        // TODO: lazy-pages must be disabled in validators in relay-chain.
        log::debug!("lazy-pages: disabled or unsupported");
        false
    } else {
        log::debug!("lazy-pages: enabled");
        true
    }
}

/// Returns whether lazy pages environment is enabled
pub fn is_lazy_pages_enabled() -> bool {
    gear_ri::is_lazy_pages_enabled()
}

/// Protect and save storage keys for pages which has no data
pub fn init_for_program(
    mem: &impl Memory,
    prog_id: ProgramId,
    stack_end: Option<WasmPageNumber>,
) -> Result<(), Error> {
    let program_prefix = crate::pages_prefix(prog_id.into_origin());
    let wasm_mem_addr = mem.get_buffer_host_addr();
    let wasm_mem_size = mem.size();
    let stack_end_page = stack_end.map(|p| p.0);

    // Cannot panic unless OS allocates buffer in not aligned by native page addr, or
    // something goes wrong with pages protection.
    gear_ri::initialize_for_program(
        wasm_mem_addr,
        wasm_mem_size.0,
        stack_end_page,
        program_prefix,
    )
    .map_err(|err| err.to_string())
    .expect("Cannot initialize lazy pages for current program");

    Ok(())
}

/// Remove lazy-pages protection, returns wasm memory begin addr
pub fn remove_lazy_pages_prot(mem: &impl Memory) -> Result<(), Error> {
    mprotect_lazy_pages(mem, false)
}

/// Protect lazy-pages and set new wasm mem addr and size,
/// if they have been changed.
pub fn update_lazy_pages_and_protect_again(
    mem: &impl Memory,
    old_mem_addr: Option<HostPointer>,
    old_mem_size: WasmPageNumber,
) -> Result<(), Error> {
    struct PointerDisplay(HostPointer);

    impl fmt::Debug for PointerDisplay {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "{:#x}", self.0)
        }
    }

    let new_mem_addr = mem.get_buffer_host_addr();
    if new_mem_addr != old_mem_addr {
        log::debug!(
            "backend executor has changed wasm mem buff: from {:?} to {:?}",
            old_mem_addr.map(PointerDisplay),
            new_mem_addr.map(PointerDisplay)
        );
        let new_mem_addr = new_mem_addr.ok_or(Error::WasmMemBufferIsUndefined)?;

        // Cannot panic, unless OS allocates wasm mem buffer
        // in not aligned by native page addr.
        gear_ri::set_wasm_mem_begin_addr(new_mem_addr).expect("Cannot not set new wasm mem addr");
    }

    let new_mem_size = mem.size();
    if new_mem_size > old_mem_size {
        gear_ri::set_wasm_mem_size(new_mem_size.0);
    }

    mprotect_lazy_pages(mem, true)
}

/// Returns list of released pages numbers.
pub fn get_released_pages() -> Vec<PageNumber> {
    gear_ri::get_released_pages()
        .into_iter()
        .map(PageNumber)
        .collect()
}
