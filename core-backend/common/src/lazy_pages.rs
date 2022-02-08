// This file is part of Gear.

// Copyright (C) 2021 Gear Technologies Inc.
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

use crate::ExtInfo;
use alloc::{boxed::Box, collections::BTreeMap, vec::Vec};
use common::Origin;
use core::convert::TryFrom;
use gear_core::env::{Ext, LaterExt};
use gear_core::memory::{PageBuf, PageNumber};
use gear_runtime_interface as gear_ri;

pub struct LazyPagesEnabled;
pub struct HasNoDataPages;

/// Try to enable and initialize lazy pages env
pub fn try_to_enable_lazy_pages(
    memory_pages: &mut BTreeMap<PageNumber, Option<Box<PageBuf>>>,
) -> (Option<LazyPagesEnabled>, Option<HasNoDataPages>) {
    // Each page which has no data in `memory_pages` is supposed to be lazy page candidate
    if !memory_pages.iter().any(|(_, buf)| buf.is_none()) {
        log::debug!("lazy-pages: there is no pages to be lazy");
        (None, None)
    } else if cfg!(feature = "disable_lazy_pages")
        || cfg!(target_family = "wasm")
        || !gear_ri::gear_ri::init_lazy_pages()
    {
        // TODO: to support in Wasm runtime we must change embedded executor to host executor.
        // TODO: also we cannot support for validators in relay-chain,
        // but it can be fixed in future only.
        log::debug!("lazy-pages: disabled or unsupported");
        (None, Some(HasNoDataPages))
    } else {
        log::debug!("lazy-pages: enabled");
        (Some(LazyPagesEnabled), Some(HasNoDataPages))
    }
}

/// Protect and save storage keys for pages which has no data
pub fn protect_pages_and_init_info<E>(
    memory_pages: &BTreeMap<PageNumber, Option<Box<PageBuf>>>,
    ext: LaterExt<E>,
) where
    E: Ext + Into<ExtInfo> + 'static,
{
    let lazy_pages = memory_pages
        .iter()
        .filter(|(_num, buf)| buf.is_none())
        .map(|(num, _buf)| num.raw())
        .collect::<Vec<u32>>();
    let prog_id_hash = ext
        .with(|ext| ext.program_id())
        .expect("Must be correct")
        .into_origin();
    let wasm_mem_begin_addr = ext
        .with(|ext| ext.get_wasm_memory_begin_addr())
        .expect("Must be correct");

    gear_ri::gear_ri::set_wasm_mem_begin_addr(wasm_mem_begin_addr as u64);

    lazy_pages.iter().for_each(|p| {
        common::save_page_lazy_info(prog_id_hash, *p);
    });

    gear_ri::gear_ri::mprotect_wasm_pages(
        wasm_mem_begin_addr as u64,
        &lazy_pages,
        false,
        false,
        false,
    );
}

/// Lazy pages contract post execution actions
pub fn post_execution_actions<E>(
    memory_pages: &mut BTreeMap<PageNumber, Option<Box<PageBuf>>>,
    ext: LaterExt<E>,
) where
    E: Ext + Into<ExtInfo> + 'static,
{
    // Loads data for released lazy pages. Data which was before execution.
    let released_pages = gear_ri::gear_ri::get_released_pages();
    released_pages.into_iter().for_each(|page| {
        let data = gear_ri::gear_ri::get_released_page_old_data(page);
        memory_pages.insert(
            (page).into(),
            Option::from(Box::new(
                PageBuf::try_from(data).expect("Must be able to convert"),
            )),
        );
    });

    // Removes protections from lazy pages
    let wasm_mem_begin_addr = ext
        .with(|ext| ext.get_wasm_memory_begin_addr())
        .expect("Must be correct") as u64;
    let lazy_pages = gear_ri::gear_ri::get_wasm_lazy_pages_numbers();
    gear_ri::gear_ri::mprotect_wasm_pages(wasm_mem_begin_addr, &lazy_pages, true, true, false);

    gear_ri::gear_ri::reset_lazy_pages_info();
}

/// Remove lazy-pages protection, returns wasm memory begin addr
pub fn remove_lazy_pages_prot<E: Ext + Into<ExtInfo>>(ext: LaterExt<E>) -> usize {
    let mem_addr = ext
        .with(|ext| ext.get_wasm_memory_begin_addr())
        .expect("Must be correct");
    let lazy_pages = gear_ri::gear_ri::get_wasm_lazy_pages_numbers();
    gear_ri::gear_ri::mprotect_wasm_pages(mem_addr as u64, &lazy_pages, true, true, false);
    mem_addr
}

/// Protect lazy-pages and set new wasm mem addr if it has been changed
pub fn protect_lazy_pages_and_set_wasm_mem<E: Ext + Into<ExtInfo>>(
    ext: LaterExt<E>,
    old_mem_addr: usize,
) {
    let new_mem_addr = ext
        .with(|ext| ext.get_wasm_memory_begin_addr())
        .expect("Must be correct");
    if new_mem_addr != old_mem_addr {
        log::debug!(
            "sandbox executor has changed wasm mem buff: from {:#x} to {:#x}",
            old_mem_addr,
            new_mem_addr
        );
        gear_ri::gear_ri::set_wasm_mem_begin_addr(new_mem_addr as u64);
    }
    let lazy_pages = gear_ri::gear_ri::get_wasm_lazy_pages_numbers();
    gear_ri::gear_ri::mprotect_wasm_pages(new_mem_addr as u64, &lazy_pages, false, false, false);
}
