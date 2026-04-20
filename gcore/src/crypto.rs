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

//! Native signature-verification primitives exposed as `gr_*` syscalls.
//!
//! Performing a signature check via these wrappers costs ~150M gas,
//! versus ~17B gas for the equivalent pure-WASM implementation.

/// Verify an sr25519 (schnorrkel/Ristretto25519) signature.
///
/// Returns `true` when `sig` is a valid signature of `msg` under `pk`,
/// `false` otherwise. Malformed keys or signatures return `false` without
/// trapping.
///
/// Dispatches to `gsys::gr_sr25519_verify`. On Vara the work runs as
/// native `sp_core::sr25519::Pair::verify`; on ethexe the same native
/// implementation runs on the host side of a wasmtime
/// `ext_sr25519_verify_v1` import.
///
/// # Examples
///
/// ```rust,ignore
/// let ok = gcore::crypto::sr25519_verify(&pk, b"hello", &sig);
/// assert!(ok);
/// ```
pub fn sr25519_verify(pk: &[u8; 32], msg: &[u8], sig: &[u8; 64]) -> bool {
    let mut ok: u8 = 0;
    unsafe {
        gsys::gr_sr25519_verify(
            pk.as_ptr() as _,
            msg.as_ptr() as _,
            msg.len() as u32,
            sig.as_ptr() as _,
            &mut ok,
        );
    }
    ok != 0
}

/// Verify an ed25519 signature.
///
/// Same shape and error convention as [`sr25519_verify`]; the only
/// difference is the curve used server-side.
pub fn ed25519_verify(pk: &[u8; 32], msg: &[u8], sig: &[u8; 64]) -> bool {
    let mut ok: u8 = 0;
    unsafe {
        gsys::gr_ed25519_verify(
            pk.as_ptr() as _,
            msg.as_ptr() as _,
            msg.len() as u32,
            sig.as_ptr() as _,
            &mut ok,
        );
    }
    ok != 0
}
