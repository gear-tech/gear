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
    memory::{HostPointer, Memory, PageBuf, PageNumber},
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
}

impl From<RIError> for Error {
    fn from(err: RIError) -> Self {
        Self::RIError(err)
    }
}

fn mprotect_lazy_pages(mem: &dyn Memory, protect: bool) -> Result<(), Error> {
    let wasm_mem_addr = match mem.get_buffer_host_addr() {
        None => return Ok(()),
        Some(addr) => addr,
    };
    gear_ri::mprotect_lazy_pages(wasm_mem_addr, protect)
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
pub fn protect_pages_and_init_info<I>(
    mem: &dyn Memory,
    lazy_pages: I,
    prog_id: ProgramId,
) -> Result<(), Error>
where
    I: Iterator<Item = PageNumber>,
{
    let prog_id_hash = prog_id.into_origin();
    let mut lay_pages_peekable = lazy_pages.peekable();

    gear_ri::reset_lazy_pages_info();

    let addr = match mem.get_buffer_host_addr() {
        None => {
            return if lay_pages_peekable.peek().is_none() {
                // In this case wasm buffer cannot be undefined
                Err(Error::WasmMemBufferIsUndefined)
            } else {
                Ok(())
            };
        }
        Some(addr) => addr,
    };
    gear_ri::set_wasm_mem_begin_addr(addr).map_err(|e| {
        log::error!("{} (it's better to stop node now)", e);
        e
    })?;

    crate::save_page_lazy_info(prog_id_hash, lay_pages_peekable);

    mprotect_lazy_pages(mem, true)
}

/// Lazy pages contract post execution actions
pub fn post_execution_actions(
    mem: &dyn Memory,
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
    mprotect_lazy_pages(mem, false)
}

/// Remove lazy-pages protection, returns wasm memory begin addr
pub fn remove_lazy_pages_prot(mem: &dyn Memory) -> Result<(), Error> {
    mprotect_lazy_pages(mem, false)
}

/// Protect lazy-pages and set new wasm mem addr if it has been changed
pub fn protect_lazy_pages_and_update_wasm_mem_addr(
    mem: &dyn Memory,
    old_mem_addr: Option<HostPointer>,
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
    mprotect_lazy_pages(mem, true)
}

/// Returns list of current lazy pages numbers
pub fn get_lazy_pages_numbers() -> Vec<PageNumber> {
    gear_ri::get_lazy_pages_numbers()
        .iter()
        .map(|p| PageNumber(*p))
        .collect()
}

/// Returns list of realeased pages numbers
pub fn get_released_pages() -> Vec<PageNumber> {
    gear_ri::get_released_pages()
        .iter()
        .map(|p| PageNumber(*p))
        .collect()
}
