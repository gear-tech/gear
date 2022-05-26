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
use gear_core::{
    ids::ProgramId,
    memory::{PageBuf, PageNumber, Memory, HostPointer},
};
use gear_runtime_interface::{gear_ri, GetReleasedPageError, MprotectError};
use sp_std::{
    collections::{btree_map::BTreeMap, btree_set::BTreeSet},
    vec::Vec,
};

#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display)]
pub enum Error {
    #[display(fmt = "{}", _0)]
    Mprotect(MprotectError),
    #[display(fmt = "{}", _0)]
    GetReleasedPage(GetReleasedPageError),
    #[display(fmt = "Cannot convert vec to page data")]
    VecToPageData,
    #[display(fmt = "Cannot find page data in storage")]
    NoPageDataInStorage,
    #[display(fmt = "Released page {:?} has initial data", _0)]
    ReleasedPageHasData(PageNumber),
}

impl From<MprotectError> for Error {
    fn from(err: MprotectError) -> Self {
        Self::Mprotect(err)
    }
}

impl From<GetReleasedPageError> for Error {
    fn from(err: GetReleasedPageError) -> Self {
        Self::GetReleasedPage(err)
    }
}

fn mprotect_lazy_pages(mem: &dyn Memory, protect: bool) -> Result<(), Error> {
    let wasm_mem_addr = mem.get_buffer_host_addr();
    gear_ri::mprotect_lazy_pages(wasm_mem_addr, protect).map_err(Into::into)
}

/// Try to enable and initialize lazy pages env
pub fn try_to_enable_lazy_pages() -> bool {
    if !gear_ri::init_lazy_pages() {
        // TODO: lazy-pages must be disabled in validators in relay-chain,
        // but it can be fixed in future only.
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
pub fn protect_pages_and_init_info(
    mem: &dyn Memory,
    lazy_pages: &BTreeSet<PageNumber>,
    prog_id: ProgramId,
) -> Result<(), Error> {
    let prog_id_hash = prog_id.into_origin();

    gear_ri::reset_lazy_pages_info();

    gear_ri::set_wasm_mem_begin_addr(mem.get_buffer_host_addr());

    lazy_pages.iter().for_each(|p| {
        crate::save_page_lazy_info(prog_id_hash, *p);
    });

    mprotect_lazy_pages(mem, true)
}

/// Lazy pages contract post execution actions
pub fn post_execution_actions(
    mem: &dyn Memory,
    memory_pages: &mut BTreeMap<PageNumber, PageBuf>,
) -> Result<(), Error> {
    // Loads data for released lazy pages. Data which was before execution.
    let released_pages = gear_ri::get_released_pages();
    for page in released_pages {
        let data = gear_ri::get_released_page_old_data(page)?;
        if memory_pages.insert(page.into(), data).is_some() {
            return Err(Error::ReleasedPageHasData(page.into()));
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
    old_mem_addr: HostPointer,
) -> Result<(), Error> {
    let new_mem_addr = mem.get_buffer_host_addr();
    if new_mem_addr != old_mem_addr {
        log::debug!(
            "backend executor has changed wasm mem buff: from {:#x} to {:#x}",
            old_mem_addr,
            new_mem_addr
        );
        gear_ri::set_wasm_mem_begin_addr(new_mem_addr);
    }
    mprotect_lazy_pages(mem, true)
}

/// Returns list of current lazy pages numbers
pub fn get_lazy_pages_numbers() -> Vec<PageNumber> {
    gear_ri::get_wasm_lazy_pages_numbers()
        .iter()
        .map(|p| PageNumber(*p))
        .collect()
}
