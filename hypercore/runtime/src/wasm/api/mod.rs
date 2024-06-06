// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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

use alloc::{boxed::Box, vec::Vec};
use parity_scale_codec::Encode;

mod instrument;
mod verify;

#[no_mangle]
extern "C" fn instrument(code_ptr: i32, code_len: i32) -> i64 {
    let code = unsafe { Vec::from_raw_parts(code_ptr as _, code_len as usize, code_len as usize) };
    return_val(instrument::instrument(code))
}

#[no_mangle]
extern "C" fn verify(code_ptr: i32, code_len: i32) -> i64 {
    let code =
        unsafe { Vec::from_raw_parts(code_ptr as *mut u8, code_len as usize, code_len as usize) };
    return_val(verify::verify(code))
}

fn return_val(val: impl Encode) -> i64 {
    let encoded = val.encode();
    let len = encoded.len() as i32;
    let ptr = Box::leak(Box::new(encoded)).as_ptr() as i32;

    unsafe { core::mem::transmute([ptr, len]) }
}
