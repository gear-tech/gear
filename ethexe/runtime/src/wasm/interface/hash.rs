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
    pub(super) fn ext_blake2b_256_v1(data: i64, out: i32);
    pub(super) fn ext_sha256_v1(data: i64, out: i32);
    pub(super) fn ext_keccak256_v1(data: i64, out: i32);
}

// Called from `NativeRuntimeInterface::blake2b_256` in
// `ethexe/runtime/src/wasm/storage.rs`, which is in turn invoked from
// `Ext<RI>::blake2b_256` via the `RI: RuntimeInterface` seam.
pub fn blake2b_256(data: &[u8]) -> [u8; 32] {
    let data_packed = utils::repr_ri_slice(data);
    let mut out = [0u8; 32];

    unsafe {
        sys::ext_blake2b_256_v1(data_packed, out.as_mut_ptr() as i32);
    }

    out
}

pub fn sha256(data: &[u8]) -> [u8; 32] {
    let data_packed = utils::repr_ri_slice(data);
    let mut out = [0u8; 32];

    unsafe {
        sys::ext_sha256_v1(data_packed, out.as_mut_ptr() as i32);
    }

    out
}

pub fn keccak256(data: &[u8]) -> [u8; 32] {
    let data_packed = utils::repr_ri_slice(data);
    let mut out = [0u8; 32];

    unsafe {
        sys::ext_keccak256_v1(data_packed, out.as_mut_ptr() as i32);
    }

    out
}
