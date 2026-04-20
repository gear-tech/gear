// This file is part of Gear.

// Copyright (C) 2026 Gear Technologies Inc.
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

//! Native hash primitives exposed as `gr_*` syscalls.
//!
//! These wrappers call native implementations on both Vara and ethexe,
//! avoiding WASM-interpreted arithmetic. Gas is charged per call plus
//! per input byte.

/// Compute the BLAKE2b-256 hash of `data`.
///
/// Dispatches to `gsys::gr_blake2b_256`. On Vara the work runs as native
/// `sp_core::hashing::blake2_256`; on ethexe the same native implementation
/// runs on the host side of a wasmtime `ext_blake2b_256_v1` import.
///
/// # Examples
///
/// ```rust,ignore
/// let digest = gcore::hash::blake2b_256(b"hello");
/// assert_eq!(digest.len(), 32);
/// ```
pub fn blake2b_256(data: &[u8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    unsafe {
        gsys::gr_blake2b_256(data.as_ptr() as _, data.len() as u32, out.as_mut_ptr() as _);
    }
    out
}
