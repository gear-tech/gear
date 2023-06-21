// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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

//! This module is for caching const syscalls.

use crate::ActorId;

extern "C" {
    /// Infallible `gr_is_getter_called` helper syscall.
    /// Will be replaced by raw instructions on instrumentation.
    ///
    /// Arguments type:
    /// - `id`: `u32` syscall id.
    ///
    /// Returns `bool`: was such const syscall called?
    pub fn gr_is_getter_called(id: u32) -> bool;

    /// Infallible `gr_set_getter_called` helper syscall.
    /// Will be replaced by raw instructions on instrumentation.
    ///
    /// Arguments type:
    /// - `id`: `u32` syscall id.
    pub fn gr_set_getter_called(id: u32);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
#[repr(u32)]
pub enum GetterSysCallsEnumeration {
    ProgramId,
    Size,
    Source,
    Value,
}

pub fn get<T: Copy, G: FnOnce() -> T>(
    id: GetterSysCallsEnumeration,
    getter: G,
    storage: &'static mut Option<T>,
) -> T {
    unsafe {
        if gr_is_getter_called(id as u32) {
            storage.unwrap_unchecked()
        } else {
            let value = getter();
            storage.replace(value);
            gr_set_getter_called(id as u32);
            value
        }
    }
}

pub static mut GR_PROGRAM_ID: Option<ActorId> = None;

pub static mut GR_SIZE: Option<usize> = None;

pub static mut GR_SOURCE: Option<ActorId> = None;

pub static mut GR_VALUE: Option<u128> = None;
