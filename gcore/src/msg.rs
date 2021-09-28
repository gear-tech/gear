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

//! Messaging api for GEAR programs.
//!
//! This module contains sys calls api for incoming message processing and synchroneouse message sending,
//! Messages are main interface for communications between actors (users and programs).
//! Every GEAR program contains code for processing incoming message.
//! While processing message program can send messages to other programs and users and reply to initial message as well.

use crate::MessageHandle;
use crate::{MessageId, ProgramId};

mod sys {
    extern "C" {
        pub fn gr_msg_id(val: *mut u8);
        pub fn gr_read(at: u32, len: u32, dest: *mut u8);
        pub fn gr_reply(
            data_ptr: *const u8,
            data_len: u32,
            gas_limit: u64,
            value_ptr: *const u8,
            message_id_ptr: *mut u8,
        );
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
    }
}

/// Sys call to obtain id of message being currently handled
///
/// # Examples
///
/// ```
/// use gcore::msg;
///
/// pub unsafe extern "C" fn handle() {
///     let current_message_id = msg::id();
/// }
pub fn id() -> MessageId {
    let mut msg_id = MessageId::default();
    unsafe { sys::gr_msg_id(msg_id.0.as_mut_ptr()) }
    msg_id
}

/// Sys call to load content of message being currently handled  
///
/// Loads content of message into buffer with size of message size which can be obtained using sys call [size()](fn@size)
/// # Examples
///
/// ```
/// use gcore::msg;
///
/// pub unsafe extern "C" fn handle() {
///     let mut result = vec![0u8; msg::size()];
///     msg::load(&mut result[..]);
/// }
/// ```
pub fn load(buffer: &mut [u8]) {
    unsafe {
        let message_size = sys::gr_size() as usize;
        if message_size != buffer.len() {
            panic!("Cannot load message - buffer length does not match");
        }

        sys::gr_read(0, message_size as _, buffer.as_mut_ptr() as _);
    }
}

/// Send new message as a reply to message currently being processed  
///
/// Some programs can replies on their messages to other programs, i.e. check other program state and use it as a parameter for own business logic.
/// GEAR reply funciton allows to send such replies, which are similar in terms of payload to a standard messages and differs only in the way that message processing will be
/// handled by a separate programm function *handle_reply*. First argument is reply message payload in bytes.
/// Second argument is gas_limit - maximum gas allowed to be utilized during reply message processing.
/// Last argument value is value to be transferred from current program account to reply message target account.
/// Returns [MessageId] of reply message.
/// Send transaction will be posted only once execution of processing will be finished, same as for standard message [send](fn@send)
/// # Examples
///
/// ```
/// use gcore::{msg, exec};
///
/// pub unsafe extern "C" fn handle() {
///    // ...
///    msg::reply(b"PING", exec::gas_available(), 0);
/// }
/// ```
/// # See also
/// [reply_push](fn@reply_push) funciton allows to form reply message in parts.
pub fn reply(payload: &[u8], gas_limit: u64, value: u128) -> MessageId {
    unsafe {
        let mut message_id = MessageId::default();
        sys::gr_reply(
            payload.as_ptr(),
            payload.len() as _,
            gas_limit,
            value.to_le_bytes().as_ptr(),
            message_id.as_mut_slice().as_mut_ptr(),
        );
        message_id
    }
}

/// Push payload part to the current reply message   
///
/// Some programs can replies on their messages to other programs, i.e. check other program state and use it as a parameter for own business logic.
/// Basic implemetation is covered in [reply](fn@reply)
/// GEAR [reply_push](fn@reply_push) funciton allows to fill reply payload in parts, while handling message.
/// Finalization of reply message is done via [reply_commit](fn@reply_commit) function similar to [send_commit](fn@send_commit)
/// # Examples
///
/// ```
/// use gcore::msg;
///
/// pub unsafe extern "C" fn handle() {
///    // ...
///    msg::reply_push(b"Part 1");
///    // ...
///    msg::reply_push(b"Part 2");
/// }
/// ```
pub fn reply_push(payload: &[u8]) {
    unsafe { sys::gr_reply_push(payload.as_ptr(), payload.len() as _) }
}

/// Get id of initial message on which current handle_reply is called   
///
/// Processing reply to the message in GEAR program performed using handle_reply function.
/// In order to obtain orginal message id on which reply has been posted program should call system function [reply_to](fn@reply_to)
/// # Examples
///
/// ```
/// use gcore::msg;
///
/// pub unsafe extern "C" fn handle_reply() {
///    // ...
///    let orginal_message_id = msg::reply_to();
/// }
/// ```
/// # Panics
/// Call to [reply_to](fn@reply_to) will panic if performed in context other then handle_reply()
pub fn reply_to() -> MessageId {
    let mut message_id = MessageId::default();
    unsafe { sys::gr_reply_to(message_id.0.as_mut_ptr()) }
    message_id
}

/// Send new message to a program or user  
///
/// GEAR allows programs to communicate to each other and users via messages.
/// Send funciton allows to send such messages.
/// First argument is address of target account.
/// Second argument is message payload in bytes.
/// Third argument is gas_limit - maximum gas allowed to be utilized during reply message processing.
/// Last argument value is value to be transferred from current program account to message target account.
/// Send transaction will be posted only once execution of processing will be finished.
/// # Examples
///
/// ```
/// use gcore::{msg, ProgramId};
///
/// pub unsafe extern "C" fn handle() {
///     // ...
///     let mut id: [u8; 32] = [0; 32];
///     for i in 0..id.len() {
///         id[i] = i as u8;
///     }
///
///     msg::send(ProgramId(id), b"HELLO", 1000, 12345678);
/// }
/// ```
/// # See also
/// [send_init](fn@send_init),[send_push](fn@send_push),[send_commit](fn@send_commit) funcitons allows to form message to send in parts.
///
pub fn send(program: ProgramId, payload: &[u8], gas_limit: u64, value: u128) -> MessageId {
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

/// Finialize and send message formed in parts
///
/// GEAR allows programs to work with messages in parts.
/// send_commit funciton finilizes message build in parts and sends it.
/// First argument is message handle [MessageHandle] which specifies particular message built in parts.
/// Second argument is address of target account.
/// Third argument is gas_limit - maximum gas allowed to be utilized during reply message processing.
/// Last argument value is value to be transferred from current program account to message target account.
/// Send transaction will be posted only once execution of processing will be finished.
/// # Examples
///
/// ```
/// use gcore::{msg, exec};
///
/// pub unsafe extern "C" fn handle() {
///    // ...
///    let msg_handle = msg::send_init();
///    msg::send_push(&msg_handle, b"PING");
///    msg::send_commit(msg_handle, msg::source(), exec::gas_available(), 42);
/// }
/// ```
/// # See also
/// [send](fn@send) allows to send message in one step
///
/// [send_push](fn@send_push),[send_init](fn@send_init) funcitons allows to form message to send in parts.
///
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

/// Initialize message to send formed in parts
///
/// GEAR allows programs to work with messages in parts.
/// send_init funciton initialize message build in parts and returns corresponding message handle.
/// # Examples
///
/// ```
/// use gcore::{msg, exec};
///
/// pub unsafe extern "C" fn handle() {
///    // ...
///    let msg_handle = msg::send_init();
///    msg::send_push(&msg_handle, b"PING");
///    msg::send_commit(msg_handle, msg::source(), exec::gas_available(), 42);
/// }
/// ```
/// # See also
/// [send](fn@send) allows to send message in one step
///
/// [send_push](fn@send_push),[send_commit](fn@send_commit) funcitons allows to form message to send in parts.
///
pub fn send_init() -> MessageHandle {
    unsafe { MessageHandle(sys::gr_send_init()) }
}

/// Push payload part of message to be sent in parts
///
/// GEAR allows programs to work with messages in parts.
/// send_push function add payload part to message specified by message handle.
/// First argument is message handle.
/// Second argument is message payload part in bytes.
/// # Examples
///
/// ```
/// use gcore::{msg, exec};
///
/// pub unsafe extern "C" fn handle() {
///    // ...
///    let msg_handle = msg::send_init();
///    msg::send_push(&msg_handle, b"PING");
///    msg::send_commit(msg_handle, msg::source(), exec::gas_available(), 42);
/// }
/// ```
/// # See also
/// [send](fn@send) allows to send message in one step
///
/// [send_init](fn@send_init),[send_commit](fn@send_commit) funcitons allows to form and send message to send in parts.
///
pub fn send_push(handle: &MessageHandle, payload: &[u8]) {
    unsafe { sys::gr_send_push(handle.0, payload.as_ptr(), payload.len() as _) }
}

/// Get size of payload of a message being processed
///
/// size() function is used to obtain size of payload of current message being processed.
/// # Examples
///
/// ```
/// use gcore::msg;
///
/// pub unsafe extern "C" fn handle() {
///    // ...
///    let size_of_the_message = msg::size();
/// }
/// ```
pub fn size() -> usize {
    unsafe { sys::gr_size() as _ }
}

/// Get address of message source
///
/// source() function is used to obtain *ProgramId* of account who send currently processing message (either program or user).
/// # Examples
///
/// ```
/// use gcore::msg;
///
/// pub unsafe extern "C" fn handle() {
///    // ...
///    let who_send_message = msg::source();
/// }
/// ```
pub fn source() -> ProgramId {
    let mut program_id = ProgramId::default();
    unsafe { sys::gr_source(program_id.as_mut_slice().as_mut_ptr()) }
    program_id
}

/// Get value associated with a message being processed
///
/// value() function is used to obtain value that has been sent along with a current message being processed.
/// # Examples
///
/// ```
/// use gcore::msg;
///
/// pub unsafe extern "C" fn handle() {
///    // ...
///    let amount_sent_with_message = msg::value();
/// }
/// ```
pub fn value() -> u128 {
    let mut value_data = [0u8; 16];
    unsafe {
        sys::gr_value(value_data.as_mut_ptr());
    }
    u128::from_le_bytes(value_data)
}
