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

//! Wrappers around system memory protections.

use crate::{
    pages::{PageDynSize, PagesIterInclusive, SizeManager},
    utils,
};
use core::ops::RangeInclusive;
use std::fmt::Debug;

#[derive(Debug, derive_more::Display)]
pub enum MprotectError {
    #[display(
        fmt = "Syscall mprotect error for interval {interval:#x?}, mask = {mask}, reason: {reason}"
    )]
    SyscallError {
        interval: RangeInclusive<usize>,
        mask: region::Protection,
        reason: region::Error,
    },
    #[display(fmt = "Interval overflows usize: {_0:#x} +/- {_1:#x}")]
    Overflow(usize, usize),
    #[display(fmt = "Zero size is restricted for mprotect")]
    ZeroSizeError,
    #[display(fmt = "Offset {_0:#x} is bigger then wasm mem size {_1:#x}")]
    OffsetOverflow(usize, usize),
}

/// Mprotect native memory interval [`addr`, `addr` + `size`].
/// Protection mask is set according to protection arguments.
unsafe fn sys_mprotect_interval(
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

/// Mprotect native memory interval [`addr`, `addr` + `size`].
/// Protection mask is set according to protection arguments, `prot_exec` is set as false always.
pub(crate) fn mprotect_interval(
    addr: usize,
    size: usize,
    prot_read: bool,
    prot_write: bool,
) -> Result<(), MprotectError> {
    unsafe { sys_mprotect_interval(addr, size, prot_read, prot_write, false) }
}

/// Protect all pages in memory interval, except pages from `except_pages`.
/// If `protect` is true then restrict read/write access, else allow them.
pub(crate) fn mprotect_mem_interval_except_pages<S: SizeManager, P: PageDynSize>(
    mem_addr: usize,
    start_offset: usize,
    mem_size: usize,
    except_pages: impl Iterator<Item = P>,
    size_ctx: &S,
    prot_read: bool,
    prot_write: bool,
) -> Result<(), MprotectError> {
    let mprotect = |start, end: usize| {
        let addr = mem_addr
            .checked_add(start)
            .ok_or(MprotectError::Overflow(mem_addr, start))?;
        let size = end
            .checked_sub(start)
            .ok_or(MprotectError::Overflow(end, start))?;
        unsafe { sys_mprotect_interval(addr, size, prot_read, prot_write, false) }
    };

    if start_offset > mem_size {
        return Err(MprotectError::OffsetOverflow(start_offset, mem_size));
    }

    let mut interval_offset = start_offset;
    for page in except_pages {
        let page_offset = page.offset(size_ctx) as usize;
        if page_offset > interval_offset {
            mprotect(interval_offset, page_offset)?;
        }
        interval_offset = page.end_offset(size_ctx) as usize + 1;
    }
    if mem_size > interval_offset {
        mprotect(interval_offset, mem_size)
    } else {
        Ok(())
    }
}

/// Mprotect all pages from `pages`.
pub(crate) fn mprotect_pages<S: SizeManager, P: PageDynSize + Ord>(
    mem_addr: usize,
    pages: impl Iterator<Item = P>,
    size_ctx: &S,
    prot_read: bool,
    prot_write: bool,
) -> Result<(), MprotectError> {
    let mprotect = |interval: PagesIterInclusive<P>| {
        let start = if let Some(start) = interval.current() {
            start
        } else {
            // Interval is empty
            return Ok(());
        };
        let end = interval.end();

        let addr = mem_addr
            .checked_add(start.offset(size_ctx) as usize)
            .ok_or(MprotectError::Overflow(
                mem_addr,
                start.offset(size_ctx) as usize,
            ))?;

        // `+ P::size()` because range is inclusive, and it's safe, because both are u32.
        let size = end
            .checked_sub(start)
            .unwrap_or_else(|| unreachable!("`end` cannot be less than `start`"))
            .offset(size_ctx) as usize
            + P::size(size_ctx) as usize;
        unsafe { sys_mprotect_interval(addr, size, prot_read, prot_write, false) }
    };

    utils::with_inclusive_ranges(&pages.collect(), mprotect)
}
