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

/// Extensions for additional features.
pub mod ext {
    #[cfg(any(feature = "debug", debug_assertions))]
    use crate::errors::{ExtError, Result};

    /// Add a `data` string to the debug log.
    ///
    /// # Examples
    ///
    /// ```
    /// use gcore::ext;
    ///
    /// #[no_mangle]
    /// extern "C" fn handle() {
    ///     ext::debug("Hello, world!").expect("Unable to log");
    /// }
    /// ```
    #[cfg(any(feature = "debug", debug_assertions))]
    pub fn debug(data: &str) -> Result<()> {
        let data_len = data.len().try_into().map_err(|_| ExtError::SyscallUsage)?;

        unsafe { gsys::gr_debug(data.as_ptr(), data_len) }

        Ok(())
    }

    /// Panic
    ///
    /// Function is completely free in terms of gas usage.
    ///
    /// # Examples
    ///
    /// ```
    /// use gcore::ext;
    ///
    /// #[no_mangle]
    /// extern "C" fn handle() {
    ///     ext::panic("I decided to panic");
    /// }
    /// ```
    pub fn panic(data: &str) -> ! {
        unsafe { gsys::gr_panic(data.as_ptr(), data.len() as u32) }
    }

    /// Out of memory panic
    ///
    /// Function is completely free in terms of gas usage.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// #![no_std]
    /// #![feature(alloc_error_handler)]
    /// #![feature(allocator_api)]
    ///
    /// extern crate alloc;
    ///
    /// use alloc::alloc::{Global, Layout, Allocator};
    /// use gcore::ext;
    ///
    /// #[alloc_error_handler]
    /// fn oom(_layout: Layout) -> ! {
    ///     ext::oom_panic()
    /// }
    ///
    /// #[no_mangle]
    /// extern "C" fn handle() {
    ///     let layout = Layout::new::<[u8; 64 * 1024]>();
    ///     if Global.allocate(layout).is_err() {
    ///         alloc::alloc::handle_alloc_error(layout);
    ///     }
    /// }
    /// ```
    pub fn oom_panic() -> ! {
        unsafe { gsys::gr_oom_panic() }
    }
}
