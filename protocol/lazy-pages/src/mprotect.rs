// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Wrappers around system memory protections.

use crate::pages::{Page, PagesAmount, PagesAmountTrait, SizeManager, SizeNumber};
use core::ops::RangeInclusive;
use numerated::interval::Interval;
use std::fmt::Debug;

#[derive(Debug, derive_more::Display)]
pub enum MprotectError {
    #[display(
        "Syscall mprotect error for interval {interval:#x?}, mask = {mask}, reason: {reason}"
    )]
    SyscallError {
        interval: RangeInclusive<usize>,
        mask: region::Protection,
        reason: region::Error,
    },
    #[display("Interval size or page address overflow")]
    Overflow,
    #[display("Zero size is restricted for mprotect")]
    ZeroSizeError,
}

/// Mprotect native memory interval [`addr`, `addr` + `size`].
/// Protection mask is set according to protection arguments.
unsafe fn sys_mprotect_interval(
    addr: usize,
    size: usize,
    allow_read: bool,
    allow_write: bool,
    allow_exec: bool,
) -> Result<(), MprotectError> {
    if size == 0 {
        return Err(MprotectError::ZeroSizeError);
    }

    let mut mask = region::Protection::NONE;
    if allow_read {
        mask |= region::Protection::READ;
    }
    if allow_write {
        mask |= region::Protection::WRITE;
    }
    if allow_exec {
        mask |= region::Protection::EXECUTE;
    }
    let res = unsafe { region::protect(addr as *mut (), size, mask) };
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
    allow_read: bool,
    allow_write: bool,
) -> Result<(), MprotectError> {
    unsafe { sys_mprotect_interval(addr, size, allow_read, allow_write, false) }
}

/// Mprotect all pages from `pages`.
pub(crate) fn mprotect_pages<M: SizeManager, S: SizeNumber, I: Into<Interval<Page<S>>>>(
    mem_addr: usize,
    pages: impl Iterator<Item = I>,
    size_ctx: &M,
    allow_read: bool,
    allow_write: bool,
) -> Result<(), MprotectError> {
    for interval in pages {
        let interval: Interval<Page<S>> = interval.into();

        let start = interval.start();

        let addr = mem_addr
            .checked_add(start.offset(size_ctx) as usize)
            .ok_or(MprotectError::Overflow)?;

        let size = interval
            .raw_len()
            .and_then(|raw| PagesAmount::<S>::new(size_ctx, raw))
            .ok_or(MprotectError::Overflow)?
            .offset(size_ctx);

        unsafe {
            sys_mprotect_interval(addr, size, allow_read, allow_write, false)?;
        }
    }
    Ok(())
}
