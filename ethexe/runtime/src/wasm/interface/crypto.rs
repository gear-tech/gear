// This file is part of Gear.
//
// Copyright (C) 2024-2026 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use super::utils;
use crate::wasm::interface;

interface::declare! {
    pub(super) fn ext_sr25519_verify_v1(pk: i32, msg: i64, sig: i32) -> i32;
    pub(super) fn ext_ed25519_verify_v1(pk: i32, msg: i64, sig: i32) -> i32;
}

// Called from `NativeRuntimeInterface::sr25519_verify` in
// `ethexe/runtime/src/wasm/storage.rs`, which is in turn invoked from
// `Ext<RI>::sr25519_verify` via the `RI: RuntimeInterface` seam.
pub fn sr25519_verify(pk: &[u8; 32], msg: &[u8], sig: &[u8; 64]) -> bool {
    let pk_ptr = pk.as_ptr() as i32;
    let msg_packed = utils::repr_ri_slice(msg);
    let sig_ptr = sig.as_ptr() as i32;

    let result = unsafe { sys::ext_sr25519_verify_v1(pk_ptr, msg_packed, sig_ptr) };

    result != 0
}

// Mirrors `sr25519_verify` shape. ed25519 keys and signatures are also
// 32 and 64 bytes respectively, so the ABI is identical — the only
// difference is the curve used server-side.
pub fn ed25519_verify(pk: &[u8; 32], msg: &[u8], sig: &[u8; 64]) -> bool {
    let pk_ptr = pk.as_ptr() as i32;
    let msg_packed = utils::repr_ri_slice(msg);
    let sig_ptr = sig.as_ptr() as i32;

    let result = unsafe { sys::ext_ed25519_verify_v1(pk_ptr, msg_packed, sig_ptr) };

    result != 0
}
