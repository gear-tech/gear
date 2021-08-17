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

use crate::prelude::Vec;
use crate::MessageHandle;
use crate::{MessageId, ProgramId};

mod sys {
    extern "C" {
        pub fn gr_gas_available() -> u64;
        pub fn gr_msg_id(val: *mut u8);
        pub fn gr_read(at: u32, len: u32, dest: *mut u8);
        pub fn gr_reply(data_ptr: *const u8, data_len: u32, gas_limit: u64, value_ptr: *const u8);
        pub fn gr_reply_push(data_ptr: *const u8, data_len: u32);
        pub fn gr_reply_to(dest: *mut u8);
        pub fn gr_send(
            program: *const u8,
            data_ptr: *const u8,
            data_len: u32,
            gas_limit: u64,
            value_ptr: *const u8,
            message_id_ptr: *mut u8,
        );
        pub fn gr_send_commit(
            handle: u32,
            message_id_ptr: *mut u8,
            program: *const u8,

            gas_limit: u64,
            value_ptr: *const u8,
        );
        pub fn gr_send_init() -> u32;
        pub fn gr_send_push(handle: u32, data_ptr: *const u8, data_len: u32);
        pub fn gr_size() -> u32;
        pub fn gr_source(program: *mut u8);
        pub fn gr_value(val: *mut u8);
        pub fn gr_wait() -> !;
        pub fn gr_wake(waker_id_ptr: *const u8);
    }
}

pub fn gas_available() -> u64 {
    unsafe { sys::gr_gas_available() }
}

pub fn id() -> MessageId {
    let mut msg_id = MessageId::default();
    unsafe { sys::gr_msg_id(msg_id.0.as_mut_ptr()) }
    msg_id
}

pub fn load() -> Vec<u8> {
    unsafe {
        let message_size = sys::gr_size() as usize;
        let mut data = Vec::with_capacity(message_size);
        data.set_len(message_size);
        sys::gr_read(0, message_size as _, data.as_mut_ptr() as _);
        data
    }
}

pub fn reply(payload: &[u8], gas_limit: u64, value: u128) {
    unsafe {
        sys::gr_reply(
            payload.as_ptr(),
            payload.len() as _,
            gas_limit,
            value.to_le_bytes().as_ptr(),
        )
    }
}

pub fn reply_push(payload: &[u8]) {
    unsafe { sys::gr_reply_push(payload.as_ptr(), payload.len() as _) }
}

pub fn reply_to() -> MessageId {
    let mut message_id = MessageId::default();
    unsafe { sys::gr_reply_to(message_id.0.as_mut_ptr()) }
    message_id
}

pub fn send(program: ProgramId, payload: &[u8], gas_limit: u64) -> MessageId {
    send_with_value(program, payload, gas_limit, 0u128)
}

pub fn send_commit(
    handle: MessageHandle,
    program: ProgramId,
    gas_limit: u64,
    value: u128,
) -> MessageId {
    unsafe {
        let mut message_id = MessageId::default();
        sys::gr_send_commit(
            handle.0,
            message_id.as_mut_slice().as_mut_ptr(),
            program.as_slice().as_ptr(),
            gas_limit,
            value.to_le_bytes().as_ptr(),
        );
        message_id
    }
}

pub fn send_init() -> MessageHandle {
    unsafe { MessageHandle(sys::gr_send_init()) }
}

pub fn send_push(handle: &MessageHandle, payload: &[u8]) {
    unsafe { sys::gr_send_push(handle.0, payload.as_ptr(), payload.len() as _) }
}

pub fn send_with_value(
    program: ProgramId,
    payload: &[u8],
    gas_limit: u64,
    value: u128,
) -> MessageId {
    unsafe {
        let mut message_id = MessageId::default();
        sys::gr_send(
            program.as_slice().as_ptr(),
            payload.as_ptr(),
            payload.len() as _,
            gas_limit,
            value.to_le_bytes().as_ptr(),
            message_id.as_mut_slice().as_mut_ptr(),
        );
        message_id
    }
}

pub fn source() -> ProgramId {
    let mut program_id = ProgramId::default();
    unsafe { sys::gr_source(program_id.as_mut_slice().as_mut_ptr()) }
    program_id
}

pub fn value() -> u128 {
    let mut value_data = [0u8; 16];
    unsafe {
        sys::gr_value(value_data.as_mut_ptr());
    }
    u128::from_le_bytes(value_data)
}

pub fn wait() -> ! {
    unsafe { sys::gr_wait() }
}

pub fn wake(waker_id: MessageId) {
    unsafe {
        sys::gr_wake(waker_id.as_slice().as_ptr());
    }
}
