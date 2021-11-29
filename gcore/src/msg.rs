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

//! Messaging API for Gear programs.
//!
//! This module contains sys calls API for incoming message processing and
//! synchronous message sending. Messages are the main interface for
//! communications between actors (users and programs). Every Gear program
//! contains code for processing an incoming message. During a message
//! processing a program can send messages to other programs and users including
//! reply to the initial message.

use crate::MessageHandle;
use crate::{ActorId, MessageId};

mod sys {
    extern "C" {
        pub fn gr_exit_code() -> i32;
        pub fn gr_msg_id(val: *mut u8);
        pub fn gr_read(at: u32, len: u32, dest: *mut u8);
        pub fn gr_reply(
            data_ptr: *const u8,
            data_len: u32,
            gas_limit: u64,
            value_ptr: *const u8,
            message_id_ptr: *mut u8,
        );
        pub fn gr_reply_commit(message_id_ptr: *mut u8, gas_limit: u64, value_ptr: *const u8);
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

/// Get the exit code of the message being processed.
///
/// This function is used to check the reply message was processed
/// successfully or not.
///
/// # Examples
///
/// ```
/// use gcore::msg;
///
/// pub unsafe extern "C" fn handle() {
///     // ...
///     let exit_code = msg::exit_code();
/// }
/// ```
pub fn exit_code() -> i32 {
    unsafe { sys::gr_exit_code() }
}

/// Obtain an identifier of the message currently being processed.
///
/// Message identifiers can be obtained for the currently processed message,
/// also each send and reply functions return a message identifier.
///
/// # Examples
///
/// ```
/// use gcore::msg;
///
/// pub unsafe extern "C" fn handle() {
///     let current_message_id = msg::id();
/// }
/// ```
pub fn id() -> MessageId {
    let mut msg_id = MessageId::default();
    unsafe { sys::gr_msg_id(msg_id.0.as_mut_ptr()) }
    msg_id
}

/// Get a payload of the message currently being processed.
///
/// Loads payload of the message into a buffer with a message size which can be
/// obtained using the [`size`] function.
///
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

/// Send a new message as a reply to the message currently being processed.
///
/// Some programs can reply to other programs, i.e. check another program's
/// state and use it as a parameter for its own business logic [`MessageId`].
/// This function allows sending such replies, which are similar to standard
/// messages in terms of payload and different only in the way the message
/// processing is handled by a separate program function called
/// `handle_reply`.
///
/// First argument is the reply message payload in bytes. Second argument is
/// `gas_limit`
/// - maximum gas allowed to be utilized during the reply message processing.
/// Last argument `value` is the value to be transferred from the current
/// program account to the reply message target account.
///
/// Reply message transactions will be posted only after processing is finished,
/// similar to the standard message [`send`].
///
/// # Examples
///
/// ```
/// use gcore::{exec, msg};
///
/// pub unsafe extern "C" fn handle() {
///     // ...
///     msg::reply(b"PING", exec::gas_available(), 0);
/// }
/// ```
///
/// # See also
///
/// [`reply_push`] function allows to form a reply message in parts.
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

/// Finalize and send a current reply message.

/// Some programs can reply on their messages to other programs, i.e. check
/// another program's state and use it as a parameter for its own business
/// logic. Basic implementation is covered in [`reply`] function.
///
/// This function allows sending reply messages filled with payload parts sent
/// via ['reply_push'] during the message handling. Finalization of the
/// reply message is done via [`reply_commit`] function similar to
/// [`send_commit`].
///
/// # Examples
///
/// ```
/// use gcore::{exec, msg};
///
/// pub unsafe extern "C" fn handle() {
///     // ...
///     msg::reply_push(b"Part 1");
///     // ...
///     msg::reply_push(b"Part 2");
///     // ...
///     msg::reply_commit(exec::gas_available(), 42);
/// }
/// ```
///
/// # See also
///
/// [`reply_push`] function allows to form a reply message in parts.
pub fn reply_commit(gas_limit: u64, value: u128) -> MessageId {
    unsafe {
        let mut message_id = MessageId::default();
        sys::gr_reply_commit(
            message_id.as_mut_slice().as_mut_ptr(),
            gas_limit,
            value.to_le_bytes().as_ptr(),
        );
        message_id
    }
}

/// Push a payload part to the current reply message.
///
/// Some programs can reply on their messages to other programs, i.e. check
/// another program's state and use it as a parameter for its own business
/// logic. Basic implementation is covered in [`reply`] function.
///
/// This function allows filling the reply payload parts via ['reply_push']
/// during the message `handling`. The payload can consist of several parts.
///
/// # Examples
///
/// ```
/// use gcore::msg;
///
/// pub unsafe extern "C" fn handle() {
///     // ...
///     msg::reply_push(b"Part 1");
///     // ...
///     msg::reply_push(b"Part 2");
/// }
/// ```
pub fn reply_push(payload: &[u8]) {
    unsafe { sys::gr_reply_push(payload.as_ptr(), payload.len() as _) }
}

/// Get an identifier of the initial message which the current handle_reply
/// function is called on.
///
/// Processing the reply to the message in Gear program is performed using
/// `handle_reply` function. In order to obtain the original message id on
/// which reply has been posted, a program should call this function.
///
/// # Examples
///
/// ```
/// use gcore::msg;
///
/// pub unsafe extern "C" fn handle_reply() {
///     // ...
///     let orginal_message_id = msg::reply_to();
/// }
/// ```
///
/// # Panics
///
/// Panics if called in a context other than `handle_reply()`.
pub fn reply_to() -> MessageId {
    let mut message_id = MessageId::default();
    unsafe { sys::gr_reply_to(message_id.0.as_mut_ptr()) }
    message_id
}

/// Send a new message to the program or user.
///
/// Gear allows programs to communicate to each other and users via messages.
/// [`send`] function allows sending such messages.
///
/// First argument is the address of the target account.
/// Second argument is message payload in bytes.
/// Third argument is gas_limit - maximum gas allowed to be utilized during the
/// sent message processing.
/// Last argument is the value to be transferred from the current program
/// account to the message target account.
/// Send transaction will be posted only after the execution of processing is
/// finished, similar to the reply message [`reply`].
///
/// # Examples
///
/// ```
/// use gcore::{msg, ActorId};
///
/// pub unsafe extern "C" fn handle() {
///     // ...
///     let mut id: [u8; 32] = [0; 32];
///     for i in 0..id.len() {
///         id[i] = i as u8;
///     }
///
///     msg::send(ActorId(id), b"HELLO", 1000, 12345678);
/// }
/// ```
///
/// # See also
///
/// [`send_init`],[`send_push`], [`send_commit`] functions allows to form a
/// message to send in parts.
pub fn send(program: ActorId, payload: &[u8], gas_limit: u64, value: u128) -> MessageId {
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

/// Finalize and send message formed in parts.
///
/// Gear allows programs to work with messages that consist of several parts.
/// This function finalizes the message built in parts and sends it.
///
/// First argument is the message handle [MessageHandle] which specifies a
/// particular message built in parts.
/// Second argument is the address of the target account.
/// Third argument is gas_limit - maximum gas allowed to be utilized during
/// reply message processing.
/// Last argument is the value to be transferred from the current program
/// account to the message target account.
/// Send transaction will be posted only after the execution of processing is
/// finished.
///
/// # Examples
///
/// ```
/// use gcore::{exec, msg};
///
/// pub unsafe extern "C" fn handle() {
///     // ...
///     let msg_handle = msg::send_init();
///     msg::send_push(&msg_handle, b"PING");
///     msg::send_commit(msg_handle, msg::source(), exec::gas_available(), 42);
/// }
/// ```
///
/// # See also
///
/// [`send`] allows to send message in one step.
///
/// [`send_push`], [`send_init`] functions allows to form a message to send in
/// parts.
pub fn send_commit(
    handle: MessageHandle,
    program: ActorId,
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

/// Initialize a message to send formed in parts.
///
/// Gear allows programs to work with messages that consist of several parts.
/// This function initializes a message built in parts and returns corresponding
/// message `handle`.
///
/// # Examples
///
/// ```
/// use gcore::{exec, msg};
///
/// pub unsafe extern "C" fn handle() {
///     // ...
///     let msg_handle = msg::send_init();
///     msg::send_push(&msg_handle, b"PING");
///     msg::send_commit(msg_handle, msg::source(), exec::gas_available(), 42);
/// }
/// ```
///
/// # See also
/// [`send`] allows to send message in one step.
///
/// [`send_push`], [`send_commit`] functions allows to form a message to send in
/// parts.
pub fn send_init() -> MessageHandle {
    unsafe { MessageHandle(sys::gr_send_init()) }
}

/// Push a payload part of the message to be sent in parts.
///
/// Gear allows programs to work with messages in parts.
/// This function adds a `payload` part to the message specified by message
/// `handle`.
///
/// # Examples
///
/// ```
/// use gcore::{exec, msg};
///
/// pub unsafe extern "C" fn handle() {
///     // ...
///     let msg_handle = msg::send_init();
///     msg::send_push(&msg_handle, b"PING");
///     msg::send_commit(msg_handle, msg::source(), exec::gas_available(), 42);
/// }
/// ```
///
/// # See also
///
/// [`send`] allows to send a message in one step.
///
/// [`send_init`], [`send_commit`] functions allows to form and send a message
/// to send in parts.
pub fn send_push(handle: &MessageHandle, payload: &[u8]) {
    unsafe { sys::gr_send_push(handle.0, payload.as_ptr(), payload.len() as _) }
}

/// Get the payload size of the message being processed.
///
/// This function is used to obtain the payload size of the current message
/// being processed.
///
/// # Examples
///
/// ```
/// use gcore::msg;
///
/// pub unsafe extern "C" fn handle() {
///     // ...
///     let payload_size = msg::size();
/// }
/// ```
pub fn size() -> usize {
    unsafe { sys::gr_size() as _ }
}

/// Get the identifier of the message source (256-bit address).
///
/// This function is used to obtain [`ActorId`] of the account that sends
/// the currently processing message (either a program or a user).
///
/// # Examples
///
/// ```
/// use gcore::msg;
///
/// pub unsafe extern "C" fn handle() {
///     // ...
///     let who_sends_message = msg::source();
/// }
/// ```
pub fn source() -> ActorId {
    let mut program_id = ActorId::default();
    unsafe { sys::gr_source(program_id.as_mut_slice().as_mut_ptr()) }
    program_id
}

/// Get the value associated with the message being processed.
///
/// This function is used to obtain the value that has been sent along with
/// a current message being processed.
///
/// # Examples
///
/// ```
/// use gcore::msg;
///
/// pub unsafe extern "C" fn handle() {
///     // ...
///     let amount_sent_with_message = msg::value();
/// }
/// ```
pub fn value() -> u128 {
    let mut value_data = [0u8; 16];
    unsafe {
        sys::gr_value(value_data.as_mut_ptr());
    }
    u128::from_le_bytes(value_data)
}
