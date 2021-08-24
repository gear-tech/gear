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

use core::ptr;

static mut MESSAGE: Vec<u8> = Vec::new();
static mut MESSAGE_ID: u64 = 0;

mod sys {
    use super::*;
    #[no_mangle]
    unsafe extern "C" fn gr_msg_id(_val: *mut u8) {}

    #[no_mangle]
    unsafe extern "C" fn gr_send(
        _program: *const u8,
        data_ptr: *const u8,
        data_len: u32,
        _gas_limit: u64,
        _value_ptr: *const u8,
        _message_id_ptr: *mut u8,
    ) {
        MESSAGE.resize(data_len as _, 0);
        ptr::copy(data_ptr, MESSAGE.as_mut_ptr(), data_len as _);
    }

    #[no_mangle]
    unsafe extern "C" fn gr_size() -> u32 {
        MESSAGE.len() as u32
    }

    #[no_mangle]
    unsafe extern "C" fn gr_read(at: u32, len: u32, dest: *mut u8) {
        let src = MESSAGE.as_ptr();
        ptr::copy(src.offset(at as _), dest, len as _);
    }

    #[no_mangle]
    unsafe extern "C" fn gr_reply(
        data_ptr: *const u8,
        data_len: u32,
        _gas_limit: u64,
        _value_ptr: *const u8,
    ) {
        MESSAGE.resize(data_len as _, 0);
        ptr::copy(data_ptr, MESSAGE.as_mut_ptr(), data_len as _);
    }

    #[no_mangle]
    unsafe extern "C" fn gr_reply_to(dest: *mut u8) {
        ptr::write_bytes(dest, 0, 32);
        ptr::copy(&MESSAGE_ID, dest as _, 1);
    }

    #[no_mangle]
    unsafe extern "C" fn gr_wait() {}
}

#[test]
fn async_send() {}
