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

//! Deprecated backend for runtime interface.
//! TODO: remove before release

use codec::{Decode, Encode};
use gear_core::memory::HostPointer;

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, derive_more::Display)]
pub enum MprotectError {
    #[display(fmt = "Page error")]
    PageError,
    #[display(fmt = "OS error")]
    OsError,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, derive_more::Display)]
#[display(fmt = "Failed to get released page")]
pub struct GetReleasedPageError;

#[cfg(feature = "std")]
#[cfg(unix)]
pub(crate) unsafe fn sys_mprotect_wasm_pages(
    from_ptr: HostPointer,
    pages_nums: &[u32],
    prot_read: bool,
    prot_write: bool,
    prot_exec: bool,
) -> Result<(), MprotectError> {
    use gear_core::memory::WasmPageNumber;

    let mut prot_mask = libc::PROT_NONE;
    if prot_read {
        prot_mask |= libc::PROT_READ;
    }
    if prot_write {
        prot_mask |= libc::PROT_WRITE;
    }
    if prot_exec {
        prot_mask |= libc::PROT_EXEC;
    }
    for page in pages_nums {
        let addr = from_ptr as usize + *page as usize * WasmPageNumber::size();
        let res = libc::mprotect(addr as *mut libc::c_void, WasmPageNumber::size(), prot_mask);
        if res != 0 {
            log::error!(
                "Cannot set page protection for {:#x}: {}",
                addr,
                errno::errno()
            );
            return Err(MprotectError::PageError);
        }
        log::trace!("mprotect wasm page: {:#x}, mask {:#x}", addr, prot_mask);
    }
    Ok(())
}

#[cfg(feature = "std")]
#[cfg(not(unix))]
pub(crate) unsafe fn sys_mprotect_wasm_pages(
    _from_ptr: HostPointer,
    _pages_nums: &[u32],
    _prot_read: bool,
    _prot_write: bool,
    _prot_exec: bool,
) -> Result<(), MprotectError> {
    log::error!("unsupported OS for pages protectection");
    Err(MprotectError::OsError)
}
