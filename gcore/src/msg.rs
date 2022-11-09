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

//! Messaging API for Gear programs.
//!
//! This module contains sys calls API for incoming message processing and
//! synchronous message sending. Messages are the main interface for
//! communications between actors (users and programs). Every Gear program
//! contains code for processing an incoming message. During a message
//! processing a program can send messages to other programs and users including
//! reply to the initial message.

use gear_core_errors::ExtError;

use crate::{error::Result, ActorId, MessageHandle, MessageId};
use core::mem::MaybeUninit;

mod sys {
    use crate::{error::SyscallError, MessageHandle};

    extern "C" {
        pub fn gr_exit_code(exit_code_ptr: *mut i32) -> SyscallError;

        pub fn gr_message_id(message_id_ptr: *mut [u8; 32]);

        pub fn gr_read(at: u32, length: u32, buffer_ptr: *mut u8) -> SyscallError;

        #[allow(improper_ctypes)]
        pub fn gr_reply(
            payload_ptr: *const u8,
            payload_len: u32,
            value_ptr: *const u128,
            delay: u32,
            message_id_ptr: *mut [u8; 32],
        ) -> SyscallError;

        #[allow(improper_ctypes)]
        pub fn gr_reply_wgas(
            payload_ptr: *const u8,
            payload_len: u32,
            gas_limit: u64,
            value_ptr: *const u128,
            delay: u32,
            message_id_ptr: *mut [u8; 32],
        ) -> SyscallError;

        #[allow(improper_ctypes)]
        pub fn gr_reply_commit(
            value_ptr: *const u128,
            delay: u32,
            message_id_ptr: *mut [u8; 32],
        ) -> SyscallError;

        #[allow(improper_ctypes)]
        pub fn gr_reply_commit_wgas(
            gas_limit: u64,
            value_ptr: *const u128,
            delay: u32,
            message_id_ptr: *mut [u8; 32],
        ) -> SyscallError;

        pub fn gr_reply_push(payload_ptr: *const u8, payload_len: u32) -> SyscallError;

        pub fn gr_reply_to(message_id_ptr: *mut [u8; 32]) -> SyscallError;

        #[allow(improper_ctypes)]
        pub fn gr_rereply(
            value_ptr: *const u128,
            delay: u32,
            message_id_ptr: *mut [u8; 32],
        ) -> SyscallError;

        #[allow(improper_ctypes)]
        pub fn gr_rereply_wgas(
            gas_limit: u64,
            value_ptr: *const u128,
            delay: u32,
            message_id_ptr: *mut [u8; 32],
        ) -> SyscallError;

        pub fn gr_rereply_push() -> SyscallError;

        #[allow(improper_ctypes)]
        pub fn gr_resend(
            destination_ptr: *const [u8; 32],
            value_ptr: *const u128,
            delay: u32,
            message_id_ptr: *mut [u8; 32],
        ) -> SyscallError;

        pub fn gr_resend_push(
            handle: MessageHandle,
        ) -> SyscallError;

        #[allow(improper_ctypes)]
        pub fn gr_resend_wgas(
            destination_ptr: *const [u8; 32],
            gas_limit: u64,
            value_ptr: *const u128,
            delay: u32,
            message_id_ptr: *mut [u8; 32],
        ) -> SyscallError;

        #[allow(improper_ctypes)]
        pub fn gr_send(
            destination_ptr: *const [u8; 32],
            payload_ptr: *const u8,
            payload_len: u32,
            value_ptr: *const u128,
            delay: u32,
            message_id_ptr: *mut [u8; 32],
        ) -> SyscallError;

        #[allow(improper_ctypes)]
        pub fn gr_send_wgas(
            destination_ptr: *const [u8; 32],
            payload_ptr: *const u8,
            data_len: u32,
            gas_limit: u64,
            value_ptr: *const u128,
            delay: u32,
            message_id_ptr: *mut [u8; 32],
        ) -> SyscallError;

        #[allow(improper_ctypes)]
        pub fn gr_send_commit(
            handle: MessageHandle,
            destination_ptr: *const [u8; 32],
            value_ptr: *const u128,
            delay: u32,
            message_id_ptr: *mut [u8; 32],
        ) -> SyscallError;

        #[allow(improper_ctypes)]
        pub fn gr_send_commit_wgas(
            handle: MessageHandle,
            destination_ptr: *const [u8; 32],
            gas_limit: u64,
            value_ptr: *const u128,
            delay: u32,
            message_id_ptr: *mut [u8; 32],
        ) -> SyscallError;

        pub fn gr_send_init(handle_ptr: *mut u32) -> SyscallError;

        pub fn gr_send_push(
            handle: MessageHandle,
            payload_ptr: *const u8,
            payload_len: u32,
        ) -> SyscallError;

        pub fn gr_size() -> u32;

        pub fn gr_source(source_ptr: *mut [u8; 32]);

        #[allow(improper_ctypes)]
        pub fn gr_value(value_ptr: *mut u128);
    }
}

/// Get the exit code of the message being processed.
///
/// This function is used in reply handler to check the message
/// was processed successfully or not.
///
/// # Examples
///
/// ```
/// use gcore::msg;
///
/// unsafe extern "C" fn handle_reply() {
///     // ...
///     let exit_code = msg::exit_code().unwrap();
/// }
/// ```
pub fn exit_code() -> Result<i32> {
    let mut bytes = 0i32.to_le_bytes();

    unsafe { sys::gr_exit_code(bytes.as_mut_ptr() as *mut i32).into_result()? }

    Ok(i32::from_le_bytes(bytes))
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
/// unsafe extern "C" fn handle() {
///     let current_message_id = msg::id();
/// }
/// ```
pub fn id() -> MessageId {
    let mut message_id = MessageId::default();

    unsafe { sys::gr_message_id(message_id.as_mut_ptr()) }

    message_id
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
/// unsafe extern "C" fn handle() {
///     let mut result = vec![0u8; msg::size() as usize];
///     msg::read(&mut result[..]);
/// }
/// ```
pub fn read(buffer: &mut [u8]) -> Result<()> {
    let size = size();

    if size as usize != buffer.len() {
        return Err(ExtError::SyscallUsage);
    }

    if size != 0 {
        unsafe { sys::gr_read(0, size, buffer.as_mut_ptr()).into_result()? }
    }

    Ok(())
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
/// Last argument `value` is the value to be transferred from the current
/// program account to the reply message target account.
///
/// Reply message transactions will be posted only after processing is finished,
/// similar to the standard message [`send`](crate::msg::send).
///
/// # Examples
///
/// ```
/// use gcore::{exec, msg};
///
/// unsafe extern "C" fn handle() {
///     // ...
///     msg::reply(b"PING", 0).unwrap();
/// }
/// ```
///
/// # See also
///
/// [`reply_push`] function allows to form a reply message in parts.
pub fn reply(payload: &[u8], value: u128) -> Result<MessageId> {
    reply_delayed(payload, value, 0)
}

/// Same as [`reply`], but sends delayed.
pub fn reply_delayed(payload: &[u8], value: u128, delay: u32) -> Result<MessageId> {
    let mut message_id = MessageId::default();

    let payload_len = payload
        .len()
        .try_into()
        .map_err(|_| ExtError::SyscallUsage)?;

    unsafe {
        sys::gr_reply(
            payload.as_ptr(),
            payload_len,
            value.to_le_bytes().as_ptr() as *const u128,
            delay,
            message_id.as_mut_ptr(),
        )
        .into_result()?
    }

    Ok(message_id)
}

/// Same as [`reply`], but with explicit gas limit.
///
/// # Examples
///
/// ```
/// use gcore::{exec, msg};
///
/// unsafe extern "C" fn handle() {
///     // ...
///     msg::reply_with_gas(b"PING", 0, 0).unwrap();
/// }
/// ```
///
/// # See also
///
/// [`reply_push`] function allows to form a reply message in parts.
pub fn reply_with_gas(payload: &[u8], gas_limit: u64, value: u128) -> Result<MessageId> {
    reply_with_gas_delayed(payload, gas_limit, value, 0)
}

/// Same as [`reply_with_gas`], but sends delayed.
pub fn reply_with_gas_delayed(
    payload: &[u8],
    gas_limit: u64,
    value: u128,
    delay: u32,
) -> Result<MessageId> {
    let mut message_id = MessageId::default();

    let payload_len = payload
        .len()
        .try_into()
        .map_err(|_| ExtError::SyscallUsage)?;

    unsafe {
        sys::gr_reply_wgas(
            payload.as_ptr(),
            payload_len,
            gas_limit,
            value.to_le_bytes().as_ptr() as *const u128,
            delay,
            message_id.as_mut_ptr(),
        )
        .into_result()?
    }

    Ok(message_id)
}

/// Finalize and send a current reply message.

/// Some programs can reply on their messages to other programs, i.e. check
/// another program's state and use it as a parameter for its own business
/// logic. Basic implementation is covered in [`reply`](crate::msg::reply)
/// function.
///
/// This function allows sending reply messages filled with payload parts sent
/// via [`reply_push`] during the message handling. Finalization of the
/// reply message is done via [`reply_commit`] function similar to
/// [`send_commit`].
///
/// # Examples
///
/// ```
/// use gcore::{exec, msg};
///
/// unsafe extern "C" fn handle() {
///     // ...
///     msg::reply_push(b"Part 1").unwrap();
///     // ...
///     msg::reply_push(b"Part 2").unwrap();
///     // ...
///     msg::reply_commit(42).unwrap();
/// }
/// ```
///
/// # See also
///
/// [`reply_push`] function allows to form a reply message in parts.
pub fn reply_commit(value: u128) -> Result<MessageId> {
    reply_commit_delayed(value, 0)
}

/// Same as [`reply_commit`], but sends delayed.
pub fn reply_commit_delayed(value: u128, delay: u32) -> Result<MessageId> {
    let mut message_id = MessageId::default();

    unsafe {
        sys::gr_reply_commit(
            value.to_le_bytes().as_ptr() as *const u128,
            delay,
            message_id.as_mut_ptr(),
        )
        .into_result()?
    }

    Ok(message_id)
}

/// Same as [`reply_commit`], but with explicit gas limit.
///
/// # Examples
///
/// ```
/// use gcore::{exec, msg};
///
/// unsafe extern "C" fn handle() {
///     // ...
///     msg::reply_push(b"Part 1").unwrap();
///     // ...
///     msg::reply_push(b"Part 2").unwrap();
///     // ...
///     msg::reply_commit_with_gas(42, 0).unwrap();
/// }
/// ```
///
/// # See also
///
/// [`reply_push`] function allows to form a reply message with in parts.
pub fn reply_commit_with_gas(gas_limit: u64, value: u128) -> Result<MessageId> {
    reply_commit_with_gas_delayed(gas_limit, value, 0)
}

/// Same as [`reply_commit_with_gas`], but sends delayed.
pub fn reply_commit_with_gas_delayed(gas_limit: u64, value: u128, delay: u32) -> Result<MessageId> {
    let mut message_id = MessageId::default();

    unsafe {
        sys::gr_reply_commit_wgas(
            gas_limit,
            value.to_le_bytes().as_ptr() as *const u128,
            delay,
            message_id.as_mut_ptr(),
        )
        .into_result()?
    }

    Ok(message_id)
}

/// Push a payload part to the current reply message.
///
/// Some programs can reply on their messages to other programs, i.e. check
/// another program's state and use it as a parameter for its own business
/// logic. Basic implementation is covered in [`reply`](crate::msg::reply)
/// function.
///
/// This function allows filling the reply payload parts via [`reply_push`]
/// during the message `handling`. The payload can consist of several parts.
///
/// # Examples
///
/// ```
/// use gcore::msg;
///
/// unsafe extern "C" fn handle() {
///     // ...
///     msg::reply_push(b"Part 1").unwrap();
///     // ...
///     msg::reply_push(b"Part 2").unwrap();
/// }
/// ```
pub fn reply_push(payload: &[u8]) -> Result<()> {
    let payload_len = payload
        .len()
        .try_into()
        .map_err(|_| ExtError::SyscallUsage)?;

    unsafe { sys::gr_reply_push(payload.as_ptr(), payload_len).into_result() }
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
/// unsafe extern "C" fn handle_reply() {
///     // ...
///     let original_message_id = msg::reply_to();
/// }
/// ```
///
/// # Panics
///
/// Panics if called in a context other than `handle_reply()`.
pub fn reply_to() -> Result<MessageId> {
    let mut message_id = MessageId::default();

    unsafe { sys::gr_reply_to(message_id.as_mut_ptr()).into_result()? }

    Ok(message_id)
}

/// Same as [`reply`], but relays the incoming message payload.
pub fn rereply(value: u128) -> Result<MessageId> {
    rereply_delayed(value, 0)
}

/// Same as [`rereply`], but sends delayed.
pub fn rereply_delayed(value: u128, delay: u32) -> Result<MessageId> {
    let mut message_id = MessageId::default();

    unsafe {
        sys::gr_rereply(
            value.to_le_bytes().as_ptr() as _,
            delay,
            message_id.as_mut_ptr(),
        )
        .into_result()?
    }

    Ok(message_id)
}

/// Same as [`reply_push`], but pushes the incoming message payload.
pub fn rereply_push() -> Result<()> {
    unsafe { sys::gr_rereply_push().into_result() }
}

/// Same as [`rereply`], but with explicit gas limit.
pub fn rereply_with_gas(gas_limit: u64, value: u128) -> Result<MessageId> {
    rereply_with_gas_delayed(gas_limit, value, 0)
}

/// Same as [`rereply_with_gas`], but sends delayed.
pub fn rereply_with_gas_delayed(gas_limit: u64, value: u128, delay: u32) -> Result<MessageId> {
    let mut message_id = MessageId::default();

    unsafe {
        sys::gr_rereply_wgas(
            gas_limit,
            value.to_le_bytes().as_ptr() as _,
            delay,
            message_id.as_mut_ptr(),
        )
        .into_result()?
    }

    Ok(message_id)
}

/// Same as [`send`], but resends the incoming message.
pub fn resend(destination: ActorId, value: u128) -> Result<MessageId> {
    resend_delayed(destination, value, 0)
}

/// Same as [`resend`], but sends delayed.
pub fn resend_delayed(
    destination: ActorId,
    value: u128,
    delay: u32,
) -> Result<MessageId> {
    let mut message_id = MessageId::default();

    unsafe {
        sys::gr_resend(
            destination.as_ptr(),
            value.to_le_bytes().as_ptr() as _,
            delay,
            message_id.as_mut_ptr(),
        )
        .into_result()?
    }

    Ok(message_id)
}

/// Same as [`send_push`], but pushes the incoming message payload.
pub fn resend_push(handle: MessageHandle) -> Result<()> {
    unsafe { sys::gr_resend_push(handle).into_result() }
}

/// Same as [`resend`], but resends the incoming message.
pub fn resend_with_gas(
    destination: ActorId,
    gas_limit: u64,
    value: u128,
) -> Result<MessageId> {
    resend_with_gas_delayed(destination, gas_limit, value, 0)
}

/// Same as [`resend_with_gas`], but sends delayed.
pub fn resend_with_gas_delayed(
    destination: ActorId,
    gas_limit: u64,
    value: u128,
    delay: u32,
) -> Result<MessageId> {
    let mut message_id = MessageId::default();

    unsafe {
        sys::gr_resend_wgas(
            destination.as_ptr(),
            gas_limit,
            value.to_le_bytes().as_ptr() as _,
            delay,
            message_id.as_mut_ptr(),
        )
        .into_result()?
    }

    Ok(message_id)
}

/// Send a new message to the program or user.
///
/// Gear allows programs to communicate to each other and users via messages.
/// [`send`](crate::msg::send) function allows sending such messages.
///
/// First argument is the address of the target account.
/// Second argument is message payload in bytes.
/// Last argument is the value to be transferred from the current program
/// account to the message target account.
/// Send transaction will be posted only after the execution of processing is
/// finished, similar to the reply message [`reply`](crate::msg::reply).
///
/// # Examples
///
/// ```
/// use gcore::{msg, ActorId};
///
/// unsafe extern "C" fn handle() {
///     // ...
///     let mut id: [u8; 32] = [0; 32];
///     for i in 0..id.len() {
///         id[i] = i as u8;
///     }
///
///     msg::send(ActorId(id), b"HELLO", 12345678);
/// }
/// ```
///
/// # See also
///
/// [`send_init`],[`send_push`], [`send_commit`] functions allows to form a
/// message to send in parts.
pub fn send(destination: ActorId, payload: &[u8], value: u128) -> Result<MessageId> {
    send_delayed(destination, payload, value, 0)
}

/// Same as [`send`], but sends delayed.
pub fn send_delayed(
    destination: ActorId,
    payload: &[u8],
    value: u128,
    delay: u32,
) -> Result<MessageId> {
    let mut message_id = MessageId::default();

    let payload_len = payload
        .len()
        .try_into()
        .map_err(|_| ExtError::SyscallUsage)?;

    unsafe {
        sys::gr_send(
            destination.as_ptr(),
            payload.as_ptr(),
            payload_len,
            value.to_le_bytes().as_ptr() as *const u128,
            delay,
            message_id.as_mut_ptr(),
        )
        .into_result()?
    }

    Ok(message_id)
}

/// Same as [`send`], but with explicit gas limit.
///
/// # Examples
///
/// ```
/// use gcore::{msg, ActorId};
///
/// unsafe extern "C" fn handle() {
///     // ...
///     let mut id: [u8; 32] = [0; 32];
///     for i in 0..id.len() {
///         id[i] = i as u8;
///     }
///
///     msg::send(ActorId(id), b"HELLO", 12345678);
/// }
/// ```
///
/// # See also
///
/// [`send_init`],[`send_push`], [`send_commit`] functions allows to form a
/// message to send in parts.
pub fn send_with_gas(
    destination: ActorId,
    payload: &[u8],
    gas_limit: u64,
    value: u128,
) -> Result<MessageId> {
    send_with_gas_delayed(destination, payload, gas_limit, value, 0)
}

/// Same as [`send_with_gas`], but sends delayed.
pub fn send_with_gas_delayed(
    destination: ActorId,
    payload: &[u8],
    gas_limit: u64,
    value: u128,
    delay: u32,
) -> Result<MessageId> {
    let mut message_id = MessageId::default();

    let payload_len = payload
        .len()
        .try_into()
        .map_err(|_| ExtError::SyscallUsage)?;

    unsafe {
        sys::gr_send_wgas(
            destination.as_ptr(),
            payload.as_ptr(),
            payload_len,
            gas_limit,
            value.to_le_bytes().as_ptr() as *const u128,
            delay,
            message_id.as_mut_ptr(),
        )
        .into_result()?
    }

    Ok(message_id)
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
/// unsafe extern "C" fn handle() {
///     // ...
///     let msg_handle = msg::send_init().unwrap();
///     msg::send_push(&msg_handle, b"PING");
///     msg::send_commit(msg_handle, msg::source(), 42);
/// }
/// ```
///
/// # See also
///
/// [`send`](crate::msg::send) allows to send message in one step.
///
/// [`send_push`], [`send_init`] functions allows to form a message to send in
/// parts.
pub fn send_commit(handle: MessageHandle, destination: ActorId, value: u128) -> Result<MessageId> {
    send_commit_delayed(handle, destination, value, 0)
}

/// Same as [`send_commit`], but with explicit gas limit.
pub fn send_commit_delayed(
    handle: MessageHandle,
    destination: ActorId,
    value: u128,
    delay: u32,
) -> Result<MessageId> {
    let mut message_id = MessageId::default();

    unsafe {
        sys::gr_send_commit(
            handle,
            destination.as_ptr(),
            value.to_le_bytes().as_ptr() as *const u128,
            delay,
            message_id.as_mut_ptr(),
        )
        .into_result()?
    }

    Ok(message_id)
}

/// Same as [`send_commit`], but with explicit gas limit.
///
/// # Examples
///
/// ```
/// use gcore::{exec, msg};
///
/// unsafe extern "C" fn handle() {
///     // ...
///     let msg_handle = msg::send_init().unwrap();
///     msg::send_push(&msg_handle, b"PING");
///     msg::send_commit_with_gas(msg_handle, msg::source(), 10_000_000, 42);
/// }
/// ```
///
/// # See also
///
/// [`send`](crate::msg::send) allows to send message in one step.
///
/// [`send_push`], [`send_init`] functions allows to form a message to send in
/// parts.
pub fn send_commit_with_gas(
    handle: MessageHandle,
    destination: ActorId,
    gas_limit: u64,
    value: u128,
) -> Result<MessageId> {
    send_commit_with_gas_delayed(handle, destination, gas_limit, value, 0)
}

/// Same as [`send_commit_with_gas`], but with explicit gas limit.
pub fn send_commit_with_gas_delayed(
    handle: MessageHandle,
    destination: ActorId,
    gas_limit: u64,
    value: u128,
    delay: u32,
) -> Result<MessageId> {
    let mut message_id = MessageId::default();

    unsafe {
        sys::gr_send_commit_wgas(
            handle,
            destination.as_ptr(),
            gas_limit,
            value.to_le_bytes().as_ptr() as *const u128,
            delay,
            message_id.as_mut_ptr(),
        )
        .into_result()?
    }

    Ok(message_id)
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
/// unsafe extern "C" fn handle() {
///     // ...
///     let msg_handle = msg::send_init().unwrap();
///     msg::send_push(&msg_handle, b"PING");
///     msg::send_commit(msg_handle, msg::source(), 42);
/// }
/// ```
///
/// # See also
/// [`send`](crate::msg::send) allows to send message in one step.
///
/// [`send_push`], [`send_commit`] functions allows to form a message to send in
/// parts.
pub fn send_init() -> Result<MessageHandle> {
    unsafe {
        let mut handle = MaybeUninit::uninit();
        sys::gr_send_init(handle.as_mut_ptr()).into_result()?;
        Ok(MessageHandle(handle.assume_init()))
    }
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
/// unsafe extern "C" fn handle() {
///     // ...
///     let msg_handle = msg::send_init().unwrap();
///     msg::send_push(&msg_handle, b"PING");
///     msg::send_commit(msg_handle, msg::source(), 42);
/// }
/// ```
///
/// # See also
///
/// [`send`](crate::msg::send) allows to send a message in one step.
///
/// [`send_init`], [`send_commit`] functions allows to form and send a message
/// to send in parts.
pub fn send_push(handle: MessageHandle, payload: &[u8]) -> Result<()> {
    let payload_len = payload
        .len()
        .try_into()
        .map_err(|_| ExtError::SyscallUsage)?;

    unsafe { sys::gr_send_push(handle, payload.as_ptr(), payload_len).into_result() }
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
/// unsafe extern "C" fn handle() {
///     // ...
///     let payload_size = msg::size();
/// }
/// ```
pub fn size() -> u32 {
    unsafe { sys::gr_size() }
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
/// unsafe extern "C" fn handle() {
///     // ...
///     let who_sends_message = msg::source();
/// }
/// ```
pub fn source() -> ActorId {
    let mut source = ActorId::default();

    unsafe { sys::gr_source(source.as_mut_ptr()) }

    source
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
/// unsafe extern "C" fn handle() {
///     // ...
///     let amount_sent_with_message = msg::value();
/// }
/// ```
pub fn value() -> u128 {
    let mut bytes = 0u128.to_le_bytes();

    unsafe {
        sys::gr_value(bytes.as_mut_ptr() as *mut u128);
    }

    u128::from_le_bytes(bytes)
}
