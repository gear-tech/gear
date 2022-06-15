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

#![no_std]

use core::{mem, ptr};
use gcore::{msg, ActorId};

#[cfg(feature = "debug")]
use gcore::ext;

static mut PROGRAM: ActorId = ActorId([0; 32]);
static mut MESSAGE: &mut [u8] = &mut [0u8; 1024];
static mut MESSAGE_LEN: usize = 0;
static mut GAS_LIMIT: u64 = 0;
static mut VALUE: u128 = 0;
static mut GAS: u64 = 0;

#[cfg(feature = "debug")]
static mut DEBUG_MSG: &mut [u8] = &mut [0u8; 1024];
#[cfg(feature = "debug")]
static mut DEBUG_MSG_LEN: usize = 0;

mod sys {
    use super::*;

    #[no_mangle]
    unsafe extern "C" fn gr_charge(gas: u64) {
        GAS += gas;
    }

    #[cfg(feature = "debug")]
    #[no_mangle]
    unsafe extern "C" fn gr_debug(msg_ptr: *const u8, msg_len: u32) {
        DEBUG_MSG_LEN = msg_len as _;
        ptr::copy(msg_ptr, DEBUG_MSG.as_mut_ptr(), msg_len as _);
    }

    #[no_mangle]
    unsafe extern "C" fn gr_read(at: u32, len: u32, dest: *mut u8) {
        let src = MESSAGE.as_ptr();
        ptr::copy(src.offset(at as _), dest, len as _);
    }

    #[no_mangle]
    unsafe extern "C" fn gr_send_wgas(
        program: *const u8,
        data_ptr: *const u8,
        data_len: u32,
        gas_limit: u64,
        value_ptr: *const u8,
        _message_id_ptr: *mut u8,
    ) -> i32 {
        ptr::copy(program, PROGRAM.0.as_mut_ptr(), 32);
        MESSAGE_LEN = data_len as _;
        ptr::copy(data_ptr, MESSAGE.as_mut_ptr(), data_len as _);
        GAS_LIMIT = gas_limit;
        VALUE = *(value_ptr as *const u128);

        0
    }

    #[no_mangle]
    unsafe extern "C" fn gr_size() -> u32 {
        MESSAGE.len() as u32
    }

    #[no_mangle]
    unsafe extern "C" fn gr_source(program: *mut u8) {
        for i in 0..PROGRAM.0.len() {
            *program.add(i) = PROGRAM.0[i];
        }
    }

    #[no_mangle]
    unsafe extern "C" fn gr_value(val: *mut u8) {
        let src = VALUE.to_le_bytes().as_ptr();
        ptr::copy(src, val, mem::size_of::<u128>());
    }

    #[no_mangle]
    unsafe extern "C" fn gr_error(_data: *mut u8) {
        unreachable!()
    }
}

#[test]
fn messages() {
    let mut id: [u8; 32] = [0; 32];
    for (i, elem) in id.iter_mut().enumerate() {
        *elem = i as u8;
    }

    msg::send_with_gas(ActorId(id), b"HELLO", 1000, 12345678).unwrap();

    let msg_source = msg::source();
    assert_eq!(msg_source, ActorId(id));
}

#[cfg(feature = "debug")]
#[test]
fn debug() {
    ext::debug("DBG: test message");

    unsafe {
        assert_eq!(&DEBUG_MSG[0..DEBUG_MSG_LEN], "DBG: test message".as_bytes());
    }
}
