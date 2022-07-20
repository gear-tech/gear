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
use core::fmt;
use gear_core::{
    ids::ProgramId,
    memory::{HostPointer, Memory, PageBuf, PageNumber, WasmPageNumber},
};
use gear_runtime_interface::{gear_ri, RIError};
use sp_std::{collections::btree_map::BTreeMap, vec::Vec};

#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display)]
pub enum Error {
    #[display(fmt = "RUNTIME INTERFACE ERROR: {}", _0)]
    RIError(RIError),
    #[display(fmt = "RUNTIME INTERFACE ERROR: {:?} has no released data", _0)]
    ReleasedPageHasNoData(PageNumber),
    #[display(fmt = "RUNTIME ERROR: released page {:?} has initial data", _0)]
    ReleasedPageHasInitialData(PageNumber),
    #[display(fmt = "RUNTIME ERROR: wasm memory buffer is undefined")]
    WasmMemBufferIsUndefined,
    #[display(fmt = "wasm memory buffer size is bigger then u32::MAX")]
    WasmMemorySizeOverflow,
}

impl From<RIError> for Error {
    fn from(err: RIError) -> Self {
        Self::RIError(err)
    }
}

fn mprotect_lazy_pages(host_addr: bool, protect: bool) -> Result<(), Error> {
    if !host_addr {
        return Ok(());
    }

    gear_ri::mprotect_lazy_pages(protect)
        .map_err(Into::into)
        .map_err(|e| {
            log::error!("{} (it's better to stop node now)", e);
            e
        })
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
pub fn protect_pages_and_init_info(mem: &impl Memory, prog_id: ProgramId) -> Result<(), Error> {
    gear_ri::reset_lazy_pages_info();

    let prog_prefix = crate::pages_prefix(prog_id.into_origin());
    gear_ri::set_program_prefix(prog_prefix);

    let host_addr_is_some: bool;
    if let Some(addr) = mem.get_buffer_host_addr() {
        gear_ri::set_wasm_mem_begin_addr(addr).map_err(|e| {
            log::error!("{} (it's better to stop node now)", e);
            e
        })?;
        host_addr_is_some = true;
    } else {
        return Ok(());
    }

    let size = mem
        .size()
        .0
        .checked_add(1)
        .ok_or(Error::WasmMemorySizeOverflow)?
        .checked_mul(WasmPageNumber::size() as u32)
        .ok_or(Error::WasmMemorySizeOverflow)?;
    gear_ri::set_wasm_mem_size(size)?;

    mprotect_lazy_pages(host_addr_is_some, true)
}

/// Lazy pages contract post execution actions
pub fn post_execution_actions(
    host_addr: bool,
    pages_data: &mut BTreeMap<PageNumber, PageBuf>,
) -> Result<(), Error> {
    // Loads data for released lazy pages. Data which was before execution.
    let released_pages = gear_ri::get_released_pages();
    for page in released_pages {
        let data = gear_ri::get_released_page_old_data(page)
            .ok_or_else(|| Error::ReleasedPageHasNoData(page.into()))?;
        if pages_data.insert(page.into(), data).is_some() {
            return Err(Error::ReleasedPageHasInitialData(page.into()));
        }
    }

    // Removes protections from lazy pages
    mprotect_lazy_pages(host_addr, false)
}

/// Remove lazy-pages protection, returns wasm memory begin addr
pub fn remove_lazy_pages_prot(host_addr: bool) -> Result<(), Error> {
    mprotect_lazy_pages(host_addr, false)
}

/// Protect lazy-pages and set new wasm mem addr and size, if they have been changed
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
        gear_ri::set_wasm_mem_begin_addr(new_mem_addr.ok_or(Error::WasmMemBufferIsUndefined)?)
            .map_err(|e| {
                log::error!("{} (it's better to stop node now)", e);
                e
            })?;
    }

    let new_mem_size = mem.size();
    if new_mem_size > old_mem_size {
        let size = new_mem_size
            .0
            .checked_add(1)
            .ok_or(Error::WasmMemorySizeOverflow)?
            .checked_mul(WasmPageNumber::size() as u32)
            .ok_or(Error::WasmMemorySizeOverflow)?;
        gear_ri::set_wasm_mem_size(size)?;
    }

    mprotect_lazy_pages(new_mem_addr.is_some(), true)
}

/// Returns list of realeased pages numbers
pub fn get_released_pages() -> Vec<PageNumber> {
    gear_ri::get_released_pages()
        .into_iter()
        .map(PageNumber)
        .collect()
}
