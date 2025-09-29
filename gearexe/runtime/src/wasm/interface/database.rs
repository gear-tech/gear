// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
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

use crate::wasm::interface;
use core::slice;
use gearexe_runtime_common::unpack_i64_to_u32;
use gprimitives::H256;
use parity_scale_codec::{Decode, Encode, Error as CodecError};

interface::declare! {
    pub(super) fn ext_database_read_by_hash_version_1(hash: *const H256) -> i64;
    pub(super) fn ext_database_write_version_1(data: *const u8, len: i32) -> *const H256;
    pub(super) fn ext_get_block_height_version_1() -> i32;
    pub(super) fn ext_get_block_timestamp_version_1() -> i64;
    pub(super) fn ext_update_state_hash_version_1(hash: *const H256);
}

// TODO(romanm): consider to move into separate RI module
pub fn update_state_hash(hash: &H256) {
    unsafe {
        sys::ext_update_state_hash_version_1(hash.as_ptr() as _);
    }
}

pub fn read<D: Decode>(hash: &H256) -> Option<Result<D, CodecError>> {
    let mut slice = read_raw(hash)?;

    Some(D::decode(&mut slice))
}

pub fn read_unwrapping<D: Decode>(hash: &H256) -> Option<D> {
    read(hash).map(|v| v.unwrap())
}

pub fn read_raw(hash: &H256) -> Option<&[u8]> {
    unsafe {
        let ptr_len = sys::ext_database_read_by_hash_version_1(hash.as_ptr() as _);

        (ptr_len != 0).then(|| {
            let (ptr, len) = unpack_i64_to_u32(ptr_len);
            slice::from_raw_parts(ptr as _, len as usize)
        })
    }
}

pub fn write(data: impl Encode) -> H256 {
    write_raw(data.encode())
}

pub fn write_raw(data: impl AsRef<[u8]>) -> H256 {
    let data = data.as_ref();

    let ptr = data.as_ptr();
    let len = data.len();

    unsafe {
        let hash_ptr = sys::ext_database_write_version_1(ptr as _, len as i32);
        let slice = slice::from_raw_parts(hash_ptr as *const u8, size_of::<H256>());
        H256::from_slice(slice)
    }
}

pub fn get_block_height() -> u32 {
    unsafe { sys::ext_get_block_height_version_1() as u32 }
}

pub fn get_block_timestamp() -> u64 {
    unsafe { sys::ext_get_block_timestamp_version_1() as u64 }
}
