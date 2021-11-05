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

//! Crate provides support for wasm runtime.

#![no_std]
#![warn(missing_docs)]

#[macro_use]
extern crate alloc;

cfg_if::cfg_if! {
    if #[cfg(feature = "wasmtime_backend")] {
        pub mod wasmtime;
        pub use crate::wasmtime::env::Environment;
    } else if #[cfg(feature = "sandbox_backend")] {
        pub mod sandbox;
        pub use crate::sandbox::env::Environment;
    }
}

#[cfg(feature = "wasmtime_backend")]
mod funcs;

use alloc::vec::Vec;
use gear_core::env::Ext;

pub(crate) const EXIT_TRAP_STR: &str = "exit";

// Helper functions
pub(crate) fn is_exit_trap(trap: &str) -> bool {
    trap.starts_with(EXIT_TRAP_STR)
}

pub(crate) fn get_id<E: Ext>(ext: &E, ptr: i32) -> [u8; 32] {
    let mut id = [0u8; 32];
    ext.get_mem(ptr as _, &mut id);
    id
}

pub(crate) fn get_u128<E: Ext>(ext: &E, ptr: i32) -> u128 {
    let mut u128_le = [0u8; 16];
    ext.get_mem(ptr as _, &mut u128_le);
    u128::from_le_bytes(u128_le)
}

pub(crate) fn get_vec<E: Ext>(ext: &E, ptr: i32, len: i32) -> Vec<u8> {
    let mut vec = vec![0u8; len as _];
    ext.get_mem(ptr as _, &mut vec);
    vec
}

pub(crate) fn set_u128<E: Ext>(ext: &mut E, ptr: i32, val: u128) {
    ext.set_mem(ptr as _, &val.to_le_bytes());
}
