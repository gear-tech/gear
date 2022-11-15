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

use crate::{
    error::{Result, SyscallError},
    ActorId, MessageHandle, MessageId, ReservationId,
};
use gear_core_errors::ExtError;
use gsys::{HashWithValue, LengthWithCode, LengthWithHandle, LengthWithHash, TwoHashesWithValue};

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
    let mut res: LengthWithCode = Default::default();

    unsafe { gsys::gr_exit_code(res.as_mut_ptr()) }
    SyscallError(res.length).into_result()?;

    Ok(res.code)
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
    unsafe { gsys::gr_message_id(message_id.as_mut_ptr()) }
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
///     let mut result = vec![0u8; 4 + msg::size() as usize];
///     msg::read(&mut result[..]);
/// }
/// ```
pub fn read(buffer: &mut [u8]) -> Result<()> {
    let size = size();

    if size as usize != buffer.len() {
        return Err(ExtError::SyscallUsage);
    }

    let mut len = 0u32;

    if size != 0 {
        unsafe { gsys::gr_read(0, size, buffer.as_mut_ptr(), &mut len as *mut u32) }
    }

    SyscallError(len).into_result()
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
    let mut res: LengthWithHash = Default::default();

    let payload_len = payload
        .len()
        .try_into()
        .map_err(|_| ExtError::SyscallUsage)?;

    let value_ptr = if value == 0 {
        i32::MAX as *const u128
    } else {
        &value as *const u128
    };

    unsafe {
        gsys::gr_reply(
            payload.as_ptr(),
            payload_len,
            value_ptr,
            delay,
            res.as_mut_ptr(),
        )
    };
    SyscallError(res.length).into_result()?;

    Ok(MessageId(res.hash))
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
/// First argument is reservation ID.
/// Second argument is the reply message payload in bytes.
/// Third argument is
/// Last argument `value` is the value to be transferred from the current
/// program account to the reply message target account.
///
/// Reply message transactions will be posted only after processing is finished,
/// similar to the standard message [`send_from_reservation`](crate::msg::send).
///
/// # Examples
///
/// ```
/// use gcore::{exec, msg};
///
/// unsafe extern "C" fn handle() {
///     // ...
///     let id = exec::reserve_gas(5_000_000, 100).expect("enough gas");
///     // ...
///     msg::reply_from_reservation(id, b"PING", 0).unwrap();
/// }
/// ```
///
/// # See also
///
/// [`reply_push`] function allows to form a reply message in parts.
pub fn reply_from_reservation(id: ReservationId, payload: &[u8], value: u128) -> Result<MessageId> {
    reply_delayed_from_reservation(id, payload, value, 0)
}

/// Same as [`reply_from_reservation`], but sends delayed.
pub fn reply_delayed_from_reservation(
    id: ReservationId,
    payload: &[u8],
    value: u128,
    delay: u32,
) -> Result<MessageId> {
    let rid_value = HashWithValue { hash: id.0, value };

    let mut res: LengthWithHash = Default::default();

    let payload_len = payload
        .len()
        .try_into()
        .map_err(|_| ExtError::SyscallUsage)?;

    unsafe {
        gsys::gr_reservation_reply(
            rid_value.as_ptr(),
            payload.as_ptr(),
            payload_len,
            delay,
            res.as_mut_ptr(),
        )
    };
    SyscallError(res.length).into_result()?;

    Ok(MessageId(res.hash))
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
    let mut res: LengthWithHash = Default::default();

    let payload_len = payload
        .len()
        .try_into()
        .map_err(|_| ExtError::SyscallUsage)?;

    let value_ptr = if value == 0 {
        i32::MAX as *const u128
    } else {
        &value as *const u128
    };

    unsafe {
        gsys::gr_reply_wgas(
            payload.as_ptr(),
            payload_len,
            gas_limit,
            value_ptr,
            delay,
            res.as_mut_ptr(),
        )
    };
    SyscallError(res.length).into_result()?;

    Ok(MessageId(res.hash))
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
    let mut res: LengthWithHash = Default::default();

    let value_ptr = if value == 0 {
        i32::MAX as *const u128
    } else {
        &value as *const u128
    };

    unsafe { gsys::gr_reply_commit(value_ptr, delay, res.as_mut_ptr()) }
    SyscallError(res.length).into_result()?;

    Ok(MessageId(res.hash))
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
    let mut res: LengthWithHash = Default::default();

    let value_ptr = if value == 0 {
        i32::MAX as *const u128
    } else {
        &value as *const u128
    };

    unsafe { gsys::gr_reply_commit_wgas(gas_limit, value_ptr, delay, res.as_mut_ptr()) }
    SyscallError(res.length).into_result()?;

    Ok(MessageId(res.hash))
}

/// Some programs can reply on their messages to other programs from
/// reservation, i.e. check another program's state and use it as a parameter
/// for its own business logic. Basic implementation is covered in
/// [`reply`](crate::msg::reply_from_reservation) function.
///
/// This function allows sending reply messages filled with payload parts sent
/// via [`reply_push`] during the message handling. Finalization of the
/// reply message is done via [`reply_commit_from_reservation`] function similar
/// to [`send_commit_from_reservation`].
///
/// # Examples
///
/// ```
/// use gcore::{exec, msg};
///
/// unsafe extern "C" fn handle() {
///     // ...
///     let id = exec::reserve_gas(5_000_000, 100).expect("enough gas");
///     // ...
///     msg::reply_push(b"Part 1").unwrap();
///     // ...
///     msg::reply_push(b"Part 2").unwrap();
///     // ...
///     msg::reply_commit_from_reservation(id, 42).unwrap();
/// }
/// ```
///
/// # See also
///
/// [`reply_push`] function allows to form a reply message in parts.
pub fn reply_commit_from_reservation(id: ReservationId, value: u128) -> Result<MessageId> {
    reply_commit_delayed_from_reservation(id, value, 0)
}

/// Same as [`reply_commit_from_reservation`], but sends delayed.
pub fn reply_commit_delayed_from_reservation(
    id: ReservationId,
    value: u128,
    delay: u32,
) -> Result<MessageId> {
    let rid_value = HashWithValue { hash: id.0, value };

    let mut res: LengthWithHash = Default::default();

    unsafe { gsys::gr_reservation_reply_commit(rid_value.as_ptr(), delay, res.as_mut_ptr()) };
    SyscallError(res.length).into_result()?;

    Ok(MessageId(res.hash))
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

    let mut len = 0u32;
    unsafe { gsys::gr_reply_push(payload.as_ptr(), payload_len, &mut len as *mut u32) };
    SyscallError(len).into_result()
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
    let mut res: LengthWithHash = Default::default();

    unsafe { gsys::gr_reply_to(res.as_mut_ptr()) };
    SyscallError(res.length).into_result()?;

    Ok(MessageId(res.hash))
}

/// Send a new message to the program or user from reservation.
///
/// Gear allows programs to communicate to each other and users via messages.
/// [`send_from_reservation`](crate::msg::send_from_reservation) function allows
/// sending such messages.
///
/// First argument is reservation ID.
/// Second argument is the address of the target account.
/// Third argument is message payload in bytes.
/// Last argument is the value to be transferred from the current program
/// account to the message target account.
/// Send transaction will be posted only after the execution of processing is
/// finished, similar to the reply message [`reply`](crate::msg::reply).
///
/// # Examples
///
/// ```
/// use gcore::{exec, msg, ActorId};
///
/// unsafe extern "C" fn handle() {
///     // ...
///     let id = exec::reserve_gas(5_000_000, 100).expect("enough gas");
///     let mut actor_id: [u8; 32] = [0; 32];
///     for i in 0..actor_id.len() {
///         actor_id[i] = i as u8;
///     }
///
///     msg::send_from_reservation(id, ActorId(actor_id), b"HELLO", 12345678)
///         .expect("successful sending");
/// }
/// ```
///
/// # See also
///
/// [`send_init`],[`send_push`], [`send_commit_from_reservation`] functions
/// allows to form a message to send in parts.
pub fn send_from_reservation(
    reservation_id: ReservationId,
    destination: ActorId,
    payload: &[u8],
    value: u128,
) -> Result<MessageId> {
    send_delayed_from_reservation(reservation_id, destination, payload, value, 0)
}

/// Same as [`send_from_reservation`], but sends delayed.
pub fn send_delayed_from_reservation(
    reservation_id: ReservationId,
    destination: ActorId,
    payload: &[u8],
    value: u128,
    delay: u32,
) -> Result<MessageId> {
    let rid_pid_value = TwoHashesWithValue {
        hash1: reservation_id.0,
        hash2: destination.0,
        value,
    };

    let mut res: LengthWithHash = Default::default();

    let payload_len = payload
        .len()
        .try_into()
        .map_err(|_| ExtError::SyscallUsage)?;

    unsafe {
        gsys::gr_reservation_send(
            rid_pid_value.as_ptr(),
            payload.as_ptr(),
            payload_len,
            delay,
            res.as_mut_ptr(),
        )
    };
    SyscallError(res.length).into_result()?;

    Ok(MessageId(res.hash))
}

/// Finalize and send message formed in parts from reservation.
///
/// Gear allows programs to work with messages that consist of several parts.
/// This function finalizes the message built in parts and sends it.
///
/// First argument is reservation ID.
/// Second argument is the message handle [MessageHandle] which specifies a
/// particular message built in parts.
/// Third argument is the address of the target account.
/// Fourth argument is gas_limit - maximum gas allowed to be utilized during
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
///     let id = exec::reserve_gas(5_000_000, 100).expect("enough gas");
///     let msg_handle = msg::send_init().unwrap();
///     msg::send_push(msg_handle, b"PING");
///     msg::send_commit_from_reservation(id, msg_handle, msg::source(), 42);
/// }
/// ```
///
/// # See also
///
/// [`send_from_reservation`](crate::msg::send_from_reservation) allows to send
/// message in one step.
///
/// [`send_push`], [`send_init`] functions allows to form a message to send in
/// parts.
pub fn send_commit_from_reservation(
    reservation_id: ReservationId,
    handle: MessageHandle,
    destination: ActorId,
    value: u128,
) -> Result<MessageId> {
    send_commit_delayed_from_reservation(reservation_id, handle, destination, value, 0)
}

/// Same as [`send_commit_from_reservation`], but sends delayed.
pub fn send_commit_delayed_from_reservation(
    reservation_id: ReservationId,
    handle: MessageHandle,
    destination: ActorId,
    value: u128,
    delay: u32,
) -> Result<MessageId> {
    let rid_pid_value = TwoHashesWithValue {
        hash1: reservation_id.0,
        hash2: destination.0,
        value,
    };

    let mut res: LengthWithHash = Default::default();

    unsafe {
        gsys::gr_reservation_send_commit(handle.0, rid_pid_value.as_ptr(), delay, res.as_mut_ptr())
    };
    SyscallError(res.length).into_result()?;

    Ok(MessageId(res.hash))
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
    let pid_value = HashWithValue {
        hash: destination.0,
        value,
    };

    let mut res: LengthWithHash = Default::default();

    let payload_len = payload
        .len()
        .try_into()
        .map_err(|_| ExtError::SyscallUsage)?;

    unsafe {
        gsys::gr_send(
            pid_value.as_ptr(),
            payload.as_ptr(),
            payload_len,
            delay,
            res.as_mut_ptr(),
        )
    };
    SyscallError(res.length).into_result()?;

    Ok(MessageId(res.hash))
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
    let pid_value = HashWithValue {
        hash: destination.0,
        value,
    };

    let mut res: LengthWithHash = Default::default();

    let payload_len = payload
        .len()
        .try_into()
        .map_err(|_| ExtError::SyscallUsage)?;

    unsafe {
        gsys::gr_send_wgas(
            pid_value.as_ptr(),
            payload.as_ptr(),
            payload_len,
            gas_limit,
            delay,
            res.as_mut_ptr(),
        )
    }
    SyscallError(res.length).into_result()?;

    Ok(MessageId(res.hash))
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
    let pid_value = HashWithValue {
        hash: destination.0,
        value,
    };

    let mut res: LengthWithHash = Default::default();

    unsafe { gsys::gr_send_commit(handle.0, pid_value.as_ptr(), delay, res.as_mut_ptr()) };
    SyscallError(res.length).into_result()?;

    Ok(MessageId(res.hash))
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
    let pid_value = HashWithValue {
        hash: destination.0,
        value,
    };

    let mut res: LengthWithHash = Default::default();

    unsafe {
        gsys::gr_send_commit_wgas(
            handle.0,
            pid_value.as_ptr(),
            gas_limit,
            delay,
            res.as_mut_ptr(),
        )
    }
    SyscallError(res.length).into_result()?;

    Ok(MessageId(res.hash))
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
        let mut res: LengthWithHandle = Default::default();
        gsys::gr_send_init(res.as_mut_ptr());
        SyscallError(res.length).into_result()?;
        Ok(MessageHandle(res.handle))
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

    let mut len = 0u32;
    unsafe {
        gsys::gr_send_push(
            handle.0,
            payload.as_ptr(),
            payload_len,
            &mut len as *mut u32,
        )
    };
    SyscallError(len).into_result()
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
    let mut size = 0u32;
    unsafe { gsys::gr_size(&mut size as *mut u32) };
    size
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
    unsafe { gsys::gr_source(source.as_mut_ptr()) }
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
    let mut value = 0u128;
    unsafe { gsys::gr_value(&mut value as *mut u128) };
    value
}
