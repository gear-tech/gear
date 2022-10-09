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

#![cfg_attr(not(feature = "std"), no_std)]

use core::ops::RangeInclusive;
use gear_core::memory::HostPointer;
use sp_runtime_interface::runtime_interface;

static_assertions::const_assert!(
    core::mem::size_of::<HostPointer>() >= core::mem::size_of::<usize>()
);

#[cfg(feature = "std")]
use gear_core::memory::PageNumber;
#[cfg(feature = "std")]
use gear_lazy_pages as lazy_pages;

pub use sp_std::{convert::TryFrom, result::Result, vec::Vec};

#[cfg(test)]
mod tests;

#[cfg(feature = "std")]
#[derive(Debug, derive_more::Display)]
pub enum MprotectError {
    #[display(
        fmt = "Syscall mprotect error for interval {:#x?}, mask = {}, reason: {}",
        interval,
        mask,
        reason
    )]
    SyscallError {
        interval: RangeInclusive<usize>,
        mask: region::Protection,
        reason: region::Error,
    },
    #[display(fmt = "Zero size is restricted for mprotect")]
    ZeroSizeError,
    #[display(fmt = "Offset {:#x} is bigger then wasm mem size {:#x}", _0, _1)]
    OffsetOverflow(usize, usize),
}

/// Mprotect native memory interval [`addr`, `addr` + `size`].
/// Protection mask is set according to protection arguments.
#[cfg(feature = "std")]
pub(crate) unsafe fn sys_mprotect_interval(
    addr: usize,
    size: usize,
    prot_read: bool,
    prot_write: bool,
    prot_exec: bool,
) -> Result<(), MprotectError> {
    if size == 0 {
        return Err(MprotectError::ZeroSizeError);
    }

    let mut mask = region::Protection::NONE;
    if prot_read {
        mask |= region::Protection::READ;
    }
    if prot_write {
        mask |= region::Protection::WRITE;
    }
    if prot_exec {
        mask |= region::Protection::EXECUTE;
    }
    let res = region::protect(addr as *mut (), size, mask);
    if let Err(reason) = res {
        return Err(MprotectError::SyscallError {
            interval: addr..=addr + size,
            mask,
            reason,
        });
    }
    log::trace!("mprotect interval: {addr:#x}, size: {size:#x}, mask: {mask}");
    Ok(())
}

/// Protect all pages in memory interval, except pages from `except_pages`.
/// If `protect` is true then restrict read/write access, else allow them.
#[cfg(feature = "std")]
fn mprotect_mem_interval_except_pages(
    mem_addr: usize,
    start_offset: usize,
    mem_size: usize,
    except_pages: impl Iterator<Item = PageNumber>,
    protect: bool,
) -> Result<(), MprotectError> {
    let mprotect = |start, end| {
        let addr = mem_addr + start;
        let size = end - start;
        unsafe { sys_mprotect_interval(addr, size, !protect, !protect, false) }
    };

    if start_offset > mem_size {
        return Err(MprotectError::OffsetOverflow(start_offset, mem_size));
    }

    let mut interval_offset = start_offset;
    for page in except_pages {
        let page_offset = page.offset();
        if page_offset > interval_offset {
            mprotect(interval_offset, page_offset)?;
        }
        interval_offset = page_offset.saturating_add(PageNumber::size());
    }
    if mem_size > interval_offset {
        mprotect(interval_offset, mem_size)
    } else {
        Ok(())
    }
}

/// Runtime interface for gear node and runtime.
/// Note: name is expanded as gear_ri
#[runtime_interface]
pub trait GearRI {
    /// Init lazy pages for `on_idle`.
    /// Returns whether initialization was successful.
    fn init_lazy_pages() -> bool {
        use lazy_pages::{DefaultUserSignalHandler, LazyPagesVersion};

        lazy_pages::init::<DefaultUserSignalHandler>(LazyPagesVersion::Version1)
    }

    /// Init lazy pages context for current program.
    /// Panic if some goes wrong during initialization.
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
        )
        .map_err(|e| e.to_string())
        .expect("Cannot initialize lazy pages for current program");

        if let Some(addr) = wasm_mem_addr {
            let stack_end = stack_end_page.map(|p| p.offset()).unwrap_or(0);
            let size = wasm_mem_size.offset();
            let except_pages = std::iter::empty::<PageNumber>();
            mprotect_mem_interval_except_pages(addr, stack_end, size, except_pages, true)
                .map_err(|err| err.to_string())
                .expect("Cannot set protection for wasm memory");
        }
    }

    /// Mprotect all wasm mem buffer except released pages.
    /// If `protect` argument is true then restrict all accesses to pages,
    /// else allows read and write accesses.
    fn mprotect_lazy_pages(protect: bool) {
        mprotect_mem_interval_except_pages(
            lazy_pages::get_wasm_mem_addr()
                .expect("Wasm mem addr must be set before calling mprotect"),
            lazy_pages::get_stack_end_wasm_addr() as usize,
            lazy_pages::get_wasm_mem_size()
                .expect("Wasm mem size must be set before calling mprotect") as usize,
            lazy_pages::get_released_pages().iter().copied(),
            protect,
        )
        .map_err(|err| err.to_string())
        .expect("Cannot set mprotection for lazy pages");
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
