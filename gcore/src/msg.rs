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
//! This module contains an API to process incoming messages and synchronously
//! send outgoing ones. Messages are the primary communication interface
//! between actors (users and programs).
//!
//! Every Gear program has code that handles messages. During message
//! processing, a program can send messages to other programs and users,
//! including a reply to the initial message.
//!
//! When some actor (user or program) sends a message to the program, it invokes
//! this program by executing the `handle` function. The invoked program can
//! obtain details of incoming messages by using this module's API ([`source`],
//! [`size`], [`read`], [`id`], [`value`], etc.).
//!
//! Optionally the program can send one or more messages to other actors. Also,
//! it can send a reply that differs from a regular message in two ways:
//! - There can be no more than one reply;
//! - It is impossible to choose the reply's destination, as it is always sent
//!   to the program invoker.
//!
//! Note that messages and a reply are not sent immediately but collected during
//! the program execution and enqueued after the execution successfully ends.

use crate::{
    errors::{Result, SyscallError},
    ActorId, MessageHandle, MessageId, ReservationId,
};
use gear_core_errors::ExtError;
use gsys::{HashWithValue, LengthWithCode, LengthWithHandle, LengthWithHash, TwoHashesWithValue};

const PTR_SPECIAL: *const u128 = u32::MAX as *const u128;

fn value_ptr(value: &u128) -> *const u128 {
    if *value == 0 {
        PTR_SPECIAL
    } else {
        value as *const u128
    }
}

/// Get the status code of the message being processed.
///
/// This function is used in the reply handler to check whether the message was
/// processed successfully or not.
///
/// # Examples
///
/// ```
/// use gcore::msg;
///
/// #[no_mangle]
/// extern "C" fn handle_reply() {
///     let status_code = msg::status_code().expect("Unable to get status code");
/// }
/// ```
pub fn status_code() -> Result<i32> {
    let mut res: LengthWithCode = Default::default();

    unsafe { gsys::gr_status_code(res.as_mut_ptr()) }
    SyscallError(res.length).into_result()?;

    Ok(res.code)
}

/// Get an identifier of the message that is currently being processed.
///
/// One can get an identifier for the currently processing message; each send
/// and reply function also returns a message identifier.
///
/// # Examples
///
/// ```
/// use gcore::{msg, MessageId};
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     let current_message_id = msg::id();
///     if current_message_id != MessageId::zero() {
///         msg::reply(b"Real message", 0).expect("Unable to reply");
///     }
/// }
/// ```
pub fn id() -> MessageId {
    let mut message_id = MessageId::default();
    unsafe { gsys::gr_message_id(message_id.as_mut_ptr()) }
    message_id
}

// TODO: issue #1859
/// Get a payload of the message that is currently being processed.
///
/// This function loads the message's payload into a buffer with a message size
/// that can be obtained using the [`size`] function.
///
/// # Examples
///
/// ```
/// use gcore::msg;
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     let mut payload = vec![0u8; 4 + msg::size()];
///     msg::read(&mut payload).expect("Unable to read");
/// }
/// ```
pub fn read(buffer: &mut [u8]) -> Result<()> {
    let size = size();

    if size > buffer.len() {
        return Err(ExtError::SyscallUsage);
    }

    let mut len = 0u32;

    if size > 0 {
        unsafe { gsys::gr_read(0, size as u32, buffer.as_mut_ptr(), &mut len as *mut u32) }
    }

    SyscallError(len).into_result()
}

/// Send a new message as a reply to the message that is currently being
/// processed.
///
/// Some programs can reply to other programs, e.g., check another program's
/// state and use it as a parameter for its business logic.
///
/// This function allows sending such replies, which are similar to standard
/// messages in terms of payload and different only in how the message
/// processing is handled by a dedicated program function called `handle_reply`.
///
/// The first argument is the payload buffer. The second argument is the value
/// to be transferred from the current program account to the reply message
/// target account.
///
/// Reply message transactions will be posted after processing is finished,
/// similar to the standard message [`send`].
///
/// # Examples
///
/// ```
/// use gcore::{exec, msg};
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     msg::reply(b"PING", exec::value_available()).expect("Unable to reply");
/// }
/// ```
///
/// # See also
///
/// - [`reply_delayed`] function sends a reply after the delay.
/// - [`reply_push`] function allows forming a reply message in parts.
/// - [`send`] function sends a new message to the program or user.
pub fn reply(payload: &[u8], value: u128) -> Result<MessageId> {
    reply_delayed(payload, value, 0)
}

/// Same as [`reply`], but sends the reply after the `delay` expressed in
/// block count.
///
/// # Examples
///
/// Reply after 100 blocks:
///
/// ```
/// use gcore::{exec, msg};
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     msg::reply_delayed(b"PING", 0, 100).expect("Unable to reply");
/// }
/// ```
pub fn reply_delayed(payload: &[u8], value: u128, delay: u32) -> Result<MessageId> {
    let mut res: LengthWithHash = Default::default();

    let payload_len = payload
        .len()
        .try_into()
        .map_err(|_| ExtError::SyscallUsage)?;

    let value_ptr = value_ptr(&value);

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

/// Same as [`reply`], but it spends gas from a reservation instead of borrowing
/// it from the gas limit provided with the incoming message.
///
/// The first argument is the reservation identifier [`ReservationId`] obtained
/// by calling the corresponding API. The second argument is the payload buffer.
/// The last argument is the value to be transferred from the current program
/// account to the reply message target account.
///
/// # Examples
///
/// ```
/// use gcore::{exec, msg};
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     let reservation_id = exec::reserve_gas(5_000_000, 100).expect("Unable to reserve");
///     msg::reply_from_reservation(reservation_id, b"PING", 0).unwrap();
/// }
/// ```
///
/// # See also
///
/// - [`send_from_reservation`] function sends a new message to the program or
///   user by using gas from a reservation.
pub fn reply_from_reservation(id: ReservationId, payload: &[u8], value: u128) -> Result<MessageId> {
    reply_delayed_from_reservation(id, payload, value, 0)
}

/// Same as [`reply_from_reservation`], but sends the reply after the `delay`
/// expressed in block count.
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

/// Same as [`reply`], but with an explicit gas limit.
///
/// # Examples
///
/// ```
/// use gcore::{exec, msg};
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     msg::reply_with_gas(b"PING", exec::gas_available() / 2, 0).expect("Unable to reply");
/// }
/// ```
///
/// # See also
///
/// - [`reply_push`] function allows forming a reply message in parts.
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

    let value_ptr = value_ptr(&value);

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

/// Finalize and send the current reply message.
///
/// Some programs can rely on their messages to other programs, i.e., check
/// another program's state and use it as a parameter for its own business
/// logic. The basic implementation is covered in [`reply`](crate::msg::reply)
/// function.
///
/// This function allows sending a reply message filled with payload parts via
/// [`reply_push`] during the message handling. The [`reply_commit`] function
/// finalizes the reply message and sends it to the program invoker.
///
/// The only argument is the value to be transferred from the current program
/// account to the reply message target account.
///
/// Note that an incomplete reply message will be dropped if the
/// [`reply_commit`] function has not been called before the current execution
/// ends.
///
/// # Examples
///
/// ```
/// use gcore::msg;
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     msg::reply_push(b"Hello,").expect("Unable to push");
///     msg::reply_push(b" world!").expect("Unable to push");
///     msg::reply_commit(42).expect("Unable to commit");
/// }
/// ```
///
/// # See also
///
/// - [`reply_push`] function allows forming a reply message in parts.
/// - [`send_commit`] function finalizes and sends a message formed in parts.
pub fn reply_commit(value: u128) -> Result<MessageId> {
    reply_commit_delayed(value, 0)
}

/// Same as [`reply_commit`], but sends the reply after the `delay` expressed
/// in block count.
pub fn reply_commit_delayed(value: u128, delay: u32) -> Result<MessageId> {
    let mut res: LengthWithHash = Default::default();

    let value_ptr = value_ptr(&value);

    unsafe { gsys::gr_reply_commit(value_ptr, delay, res.as_mut_ptr()) }
    SyscallError(res.length).into_result()?;

    Ok(MessageId(res.hash))
}

/// Same as [`reply_commit`], but with an explicit gas limit.
///
/// # Examples
///
/// ```
/// use gcore::{exec, msg};
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     msg::reply_push(b"Hello, ").expect("Unable to push");
///     msg::reply_push(b", world!").expect("Unable to push");
///     msg::reply_commit_with_gas(exec::gas_available() / 2, 0).expect("Unable to commit");
/// }
/// ```
///
/// # See also
///
/// - [`reply_push`] function allows forming a reply message in parts.
pub fn reply_commit_with_gas(gas_limit: u64, value: u128) -> Result<MessageId> {
    reply_commit_with_gas_delayed(gas_limit, value, 0)
}

/// Same as [`reply_commit_with_gas`], but sends the reply after the `delay`
/// expressed in block count.
pub fn reply_commit_with_gas_delayed(gas_limit: u64, value: u128, delay: u32) -> Result<MessageId> {
    let mut res: LengthWithHash = Default::default();

    let value_ptr = value_ptr(&value);

    unsafe { gsys::gr_reply_commit_wgas(gas_limit, value_ptr, delay, res.as_mut_ptr()) }
    SyscallError(res.length).into_result()?;

    Ok(MessageId(res.hash))
}

/// Same as [`reply_commit`], but it spends gas from a reservation instead of
/// borrowing it from the gas limit provided with the incoming message.
///
/// # Examples
///
/// ```
/// use gcore::{exec, msg};
///
/// extern "C" fn handle() {
///     msg::reply_push(b"Hello,").expect("Unable to push");
///     msg::reply_push(b" world!").expect("Unable to push");
///     let reservation_id = exec::reserve_gas(5_000_000, 100).expect("Unable to reserves");
///     msg::reply_commit_from_reservation(reservation_id, 42).expect("Unable to commit");
/// }
/// ```
///
/// # See also
///
/// - [`reply_push`] function allows forming a reply message in parts.
pub fn reply_commit_from_reservation(id: ReservationId, value: u128) -> Result<MessageId> {
    reply_commit_delayed_from_reservation(id, value, 0)
}

/// Same as [`reply_commit_from_reservation`], but sends the message after the
/// `delay` expressed in block count.
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
/// Some programs can rely on their messages to other programs, i.e., check
/// another program's state and use it as a parameter for its own business
/// logic. The basic implementation is covered in the [`reply`] function.
///
/// This function allows filling the reply `payload` parts via [`reply_push`]
/// during the message handling. The payload can consist of several parts.
///
/// Note that an incomplete reply message will be dropped if the
/// [`reply_commit`] function has not been called before the current execution
/// ends.
///
/// # Examples
///
/// See the [`reply_commit`] examples.
///
/// # See also
///
/// - [`reply_commit`] function finalizes and sends the current reply message.
pub fn reply_push(payload: &[u8]) -> Result<()> {
    let payload_len = payload
        .len()
        .try_into()
        .map_err(|_| ExtError::SyscallUsage)?;

    let mut len = 0u32;
    unsafe { gsys::gr_reply_push(payload.as_ptr(), payload_len, &mut len as *mut u32) };
    SyscallError(len).into_result()
}

/// Get an identifier of the initial message on which the current `handle_reply`
/// function is called.
///
/// The Gear program processes the reply to the message using the `handle_reply`
/// function. Therefore, a program should call this function to obtain the
/// original message identifier on which the reply has been posted.
///
/// # Examples
///
/// ```
/// use gcore::msg;
///
/// #[no_mangle]
/// extern "C" fn handle_reply() {
///     let original_message_id = msg::reply_to().unwrap();
/// }
/// ```
pub fn reply_to() -> Result<MessageId> {
    let mut res: LengthWithHash = Default::default();

    unsafe { gsys::gr_reply_to(res.as_mut_ptr()) };
    SyscallError(res.length).into_result()?;

    Ok(MessageId(res.hash))
}

/// Get an identifier of the message which issued a signal.
///
/// The Gear program processes the signal using the `handle_signal`
/// function. Therefore, a program should call this function to obtain the
/// original message identifier which issued a signal.
///
/// # Examples
///
/// ```
/// use gcore::msg;
///
/// #[no_mangle]
/// extern "C" fn handle_signal() {
///     let erroneous_message = msg::signal_from().unwrap();
/// }
/// ```
pub fn signal_from() -> Result<MessageId> {
    let mut res: LengthWithHash = Default::default();

    unsafe { gsys::gr_signal_from(res.as_mut_ptr()) };
    SyscallError(res.length).into_result()?;

    Ok(MessageId(res.hash))
}

/// Same as [`reply`], but relays the incoming message payload.
pub fn reply_input(value: u128, offset: u32, len: u32) -> Result<MessageId> {
    reply_input_delayed(value, offset, len, 0)
}

/// Same as [`reply_input`], but sends delayed.
pub fn reply_input_delayed(value: u128, offset: u32, len: u32, delay: u32) -> Result<MessageId> {
    let mut res: LengthWithHash = Default::default();

    let value_ptr = value_ptr(&value);

    unsafe {
        gsys::gr_reply_input(offset, len, value_ptr, delay, res.as_mut_ptr());
    }

    SyscallError(res.length).into_result()?;

    Ok(MessageId(res.hash))
}

/// Same as [`reply_push`] but uses the input buffer as a payload source.
///
/// The first and second arguments are the offset and length of the input
/// buffer's piece that is to be pushed back to the output.
///
/// # Examples
///
/// Send half of the incoming payload back to the sender as a reply.
///
/// ```
/// use gcore::msg;
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     msg::reply_push_input(0, msg::size() as u32 / 2).expect("Unable to push");
///     msg::reply_commit(0).expect("Unable to commit");
/// }
/// ```
///
/// # See also
///
/// - [`send_push_input`] function allows using the input buffer as a payload
///   source for an outcoming message.
pub fn reply_push_input(offset: u32, len: u32) -> Result<()> {
    let mut result_len = 0u32;
    unsafe { gsys::gr_reply_push_input(offset, len, &mut result_len as _) };
    SyscallError(result_len).into_result()
}

/// Same as [`reply_input`], but with explicit gas limit.
pub fn reply_input_with_gas(
    gas_limit: u64,
    value: u128,
    offset: u32,
    len: u32,
) -> Result<MessageId> {
    reply_input_with_gas_delayed(gas_limit, value, offset, len, 0)
}

/// Same as [`reply_input_with_gas`], but sends delayed.
pub fn reply_input_with_gas_delayed(
    gas_limit: u64,
    value: u128,
    offset: u32,
    len: u32,
    delay: u32,
) -> Result<MessageId> {
    let mut res: LengthWithHash = Default::default();

    let value_ptr = value_ptr(&value);

    unsafe {
        gsys::gr_reply_input_wgas(offset, len, gas_limit, value_ptr, delay, res.as_mut_ptr());
    }
    SyscallError(res.length).into_result()?;

    Ok(MessageId(res.hash))
}

/// Same as [`send`] but uses the input buffer as a payload source.
///
/// The first argument is the address of the target account ([`ActorId`]). The
/// second argument is the value to be transferred from the current program
/// account to the message target account. The third and last arguments are the
/// offset and length of the input buffer's piece to be sent back.
///
/// # Examples
///
/// Send half of the incoming payload back to the sender.
///
/// ```
/// use gcore::msg;
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     msg::send_input(msg::source(), 0, 0, msg::size() as u32 / 2).expect("Unable to send");
/// }
/// ```
///
/// # See also
///
/// - [`send_push_input`] function allows using the input buffer as a payload
///   source for an outcoming message.
pub fn send_input(destination: ActorId, value: u128, offset: u32, len: u32) -> Result<MessageId> {
    send_input_delayed(destination, value, offset, len, 0)
}

/// Same as [`send_input`], but sends delayed.
pub fn send_input_delayed(
    destination: ActorId,
    value: u128,
    offset: u32,
    len: u32,
    delay: u32,
) -> Result<MessageId> {
    let pid_value = HashWithValue {
        hash: destination.0,
        value,
    };

    let mut res: LengthWithHash = Default::default();

    unsafe {
        gsys::gr_send_input(pid_value.as_ptr(), offset, len, delay, res.as_mut_ptr());
    }
    SyscallError(res.length).into_result()?;

    Ok(MessageId(res.hash))
}

/// Send a new message to the program or user.
///
/// Gear allows programs to communicate with each other and users via messages.
/// For example, the [`send`] function allows sending such messages.
///
/// The first argument is the address of the target account ([`ActorId`]). The
/// second argument is the payload buffer. The last argument is the value to be
/// transferred from the current program account to the message target account.
///
/// Send transaction will be posted after processing is finished, similar to the
/// reply message [`reply`].
///
/// # Examples
///
/// Send a message with value to the arbitrary address (don't repeat it in your
/// program!):
///
/// ```
/// use gcore::{msg, ActorId};
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     // Receiver id is collected from bytes from 0 to 31
///     let id: [u8; 32] = core::array::from_fn(|i| i as u8);
///     msg::send(ActorId(id), b"HELLO", 42);
/// }
/// ```
///
/// # See also
///
/// - [`reply`] function sends a new message as a reply to the message that is
///   currently being processed.
/// - [`send_init`], [`send_push`], and [`send_commit`] functions allow forming
///   a message to send in parts.
pub fn send(destination: ActorId, payload: &[u8], value: u128) -> Result<MessageId> {
    send_delayed(destination, payload, value, 0)
}

/// Same as [`send`], but it spends gas from a reservation instead of borrowing
/// it from the gas limit provided with the incoming message.
///
/// The first argument is the reservation identifier [`ReservationId`] obtained
/// by calling the corresponding API. The second argument is the address of the
/// target account ([`ActorId`]). The third argument is the payload buffer.
/// Finally, the last argument is the value to be transferred from the current
/// program account to the message target account.
///
/// # Examples
///
/// Send a message with value to the arbitrary address (don't repeat it in your
/// program!):
///
/// ```
/// use gcore::{exec, msg, ActorId};
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     // Reserve 5 million of gas for 100 blocks
///     let reservation_id = exec::reserve_gas(5_000_000, 100).expect("Unable to reserve");
///     // Receiver id is collected from bytes from 0 to 31
///     let actor_id: [u8; 32] = core::array::from_fn(|i| i as u8);
///     msg::send_from_reservation(reservation_id, ActorId(actor_id), b"HELLO", 42)
///         .expect("Unable to send");
/// }
/// ```
///
/// # See also
///
/// - [`reply_from_reservation`] function sends a reply to the program or user
///   by using gas from a reservation.
/// - [`send_init`],[`send_push`], [`send_commit_from_reservation`] functions
///   allows forming a message to send in parts.
pub fn send_from_reservation(
    reservation_id: ReservationId,
    destination: ActorId,
    payload: &[u8],
    value: u128,
) -> Result<MessageId> {
    send_delayed_from_reservation(reservation_id, destination, payload, value, 0)
}

/// Same as [`send_from_reservation`], but sends the message after the`delay`
/// expressed in block count.
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

/// Same as [`send_push`] but uses the input buffer as a payload source.
///
/// The first argument is the message handle [`MessageHandle`] that specifies a
/// particular message built in parts. The second and third arguments are the
/// offset and length of the input buffer's piece that is to be pushed back to
/// the output.
///
/// # Examples
///
/// Send half of the incoming payload back to the sender.
///
/// ```
/// use gcore::msg;
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     let msg_handle = msg::send_init().expect("Unable to init");
///     msg::send_push_input(msg_handle, 0, msg::size() as u32 / 2).expect("Unable to push");
///     msg::send_commit(msg_handle, msg::source(), 0).expect("Unable to commit");
/// }
/// ```
///
/// /// # See also
///
/// - [`reply_push_input`] function allows using the input buffer as a payload
///   source for a reply message.
pub fn send_push_input(handle: MessageHandle, offset: u32, len: u32) -> Result<()> {
    let mut result_len = 0u32;
    unsafe {
        gsys::gr_send_push_input(handle.0, offset, len, &mut result_len as _);
    }
    SyscallError(result_len).into_result()
}

/// Same as [`send_input`], but with explicit gas limit.
pub fn send_input_with_gas(
    destination: ActorId,
    gas_limit: u64,
    value: u128,
    offset: u32,
    len: u32,
) -> Result<MessageId> {
    send_input_with_gas_delayed(destination, gas_limit, value, offset, len, 0)
}

/// Same as [`send_input_with_gas`], but sends delayed.
pub fn send_input_with_gas_delayed(
    destination: ActorId,
    gas_limit: u64,
    value: u128,
    offset: u32,
    len: u32,
    delay: u32,
) -> Result<MessageId> {
    let pid_value = HashWithValue {
        hash: destination.0,
        value,
    };

    let mut res: LengthWithHash = Default::default();

    unsafe {
        gsys::gr_send_input_wgas(
            pid_value.as_ptr(),
            offset,
            len,
            gas_limit,
            delay,
            res.as_mut_ptr(),
        );
    }
    SyscallError(res.length).into_result()?;

    Ok(MessageId(res.hash))
}

/// Same as [`send_commit`], but it spends gas from a reservation instead of
/// borrowing it from the gas limit provided with the incoming message.
///
/// # Examples
///
/// ```
/// use gcore::{exec, msg};
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     let reservation_id = exec::reserve_gas(5_000_000, 100).expect("Unable to reserve");
///     let msg_handle = msg::send_init().expect("Unable to init");
///     msg::send_push(msg_handle, b"Hello,").expect("Unable to push");
///     msg::send_push(msg_handle, b" world!").expect("Unable to push");
///     msg::send_commit_from_reservation(reservation_id, msg_handle, msg::source(), 42)
///         .expect("Unable to commit");
/// }
/// ```
///
/// # See also
///
/// - [`send_from_reservation`] allows sending message by using gas from
///   reservation.
/// - [`send_push`], [`send_init`] functions allows forming message to send in
///   parts.
pub fn send_commit_from_reservation(
    reservation_id: ReservationId,
    handle: MessageHandle,
    destination: ActorId,
    value: u128,
) -> Result<MessageId> {
    send_commit_delayed_from_reservation(reservation_id, handle, destination, value, 0)
}

/// Same as [`send_commit_from_reservation`], but sends the message after the
/// `delay` expressed in block count.
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

/// Same as [`send`], but sends the message after the `delay` expressed in block
/// count.
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

/// Same as [`send`], but with an explicit gas limit.
///
/// # Examples
///
/// ```
/// use gcore::{msg, ActorId};
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     let id: [u8; 32] = core::array::from_fn(|i| i as u8);
///     msg::send_with_gas(ActorId(id), b"HELLO", 5_000_000, 42);
/// }
/// ```
///
/// # See also
///
/// - [`reply_with_gas`] function sends a reply with an explicit gas limit.
/// - [`send_init`],[`send_push`], [`send_commit_with_gas`] functions allow
///   forming a message to send in parts with an explicit gas limit.
pub fn send_with_gas(
    destination: ActorId,
    payload: &[u8],
    gas_limit: u64,
    value: u128,
) -> Result<MessageId> {
    send_with_gas_delayed(destination, payload, gas_limit, value, 0)
}

/// Same as [`send_with_gas`], but sends the message after the `delay` expressed
/// in block count.
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

/// Finalize and send the message formed in parts.
///
/// Gear allows programs to work with messages that consist of several parts.
/// This function finalizes the message built in parts and sends it.
///
/// The first argument is the message handle [`MessageHandle`] that specifies a
/// particular message built in parts. The second argument is the address of the
/// target account. The third argument is the maximum gas allowed to be utilized
/// during the message processing. Finally, the last argument is the value to be
/// transferred from the current program account to the message target account.
///
/// # Examples
///
/// ```
/// use gcore::msg;
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     let msg_handle = msg::send_init().expect("Unable to init");
///     msg::send_push(msg_handle, b"Hello, ").expect("Unable to push");
///     msg::send_push(msg_handle, b" world!").expect("Unable to push");
///     msg::send_commit(msg_handle, msg::source(), 42).expect("Unable to commit");
/// }
/// ```
///
/// # See also
///
/// - [`send`] function allows sending a message in one step.
/// - [`send_push`], [`send_init`] functions allow forming a message to send in
///   parts.
pub fn send_commit(handle: MessageHandle, destination: ActorId, value: u128) -> Result<MessageId> {
    send_commit_delayed(handle, destination, value, 0)
}

/// Same as [`send_commit`], but sends the message after the `delay` expressed
/// in block count.
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

/// Same as [`send_commit`], but with an explicit gas limit.
///
/// # Examples
///
/// ```
/// use gcore::msg;
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     let msg_handle = msg::send_init().expect("Unable to init");
///     msg::send_push(msg_handle, b"Hello,").expect("Unable to push");
///     msg::send_push(msg_handle, b" world!").expect("Unable to push");
///     msg::send_commit_with_gas(msg_handle, msg::source(), 10_000_000, 42)
///         .expect("Unable to commit");
/// }
/// ```
///
/// # See also
///
/// - [`send`] function allows sending a message in one step.
/// - [`send_push`], [`send_init`] functions allows forming a message to send in
///   parts.
pub fn send_commit_with_gas(
    handle: MessageHandle,
    destination: ActorId,
    gas_limit: u64,
    value: u128,
) -> Result<MessageId> {
    send_commit_with_gas_delayed(handle, destination, gas_limit, value, 0)
}

/// Same as [`send_commit_with_gas`], but sends the message after the `delay`
/// expressed in block count.
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
/// This function initializes a message built in parts and returns the
/// corresponding [`MessageHandle`].
///
/// # Examples
///
/// ```
/// use gcore::msg;
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     let msg_handle = msg::send_init().expect("Unable to init");
///     msg::send_push(msg_handle, b"Hello,").expect("Unable to push");
///     msg::send_push(msg_handle, b" world!").expect("Unable to push");
///     msg::send_commit(msg_handle, msg::source(), 42).expect("Unable to commit");
/// }
/// ```
///
/// # See also
/// - [`send`] function allows sending message in one step.
/// - [`send_push`], [`send_commit`] functions allows forming a message to send
///   in parts.
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
/// This function adds a `payload` part to the message specified by the message
/// `handle`.
///
/// The first argument is the message handle [`MessageHandle`] that specifies a
/// particular message built in parts. The second argument is the payload
/// buffer.
///
/// # Examples
///
/// ```
/// use gcore::msg;
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     let msg_handle = msg::send_init().expect("Unable to init");
///     msg::send_push(msg_handle, b"Hello,").expect("Unable to push");
///     msg::send_push(msg_handle, b" world!").expect("Unable to push");
///     msg::send_commit(msg_handle, msg::source(), 42).expect("Unable to commit");
/// }
/// ```
///
/// # See also
///
/// - [`reply_push`] function allows forming a reply message in parts.
/// - [`send`] function allows sending a message in one step.
/// - [`send_init`], [`send_commit`] functions allows forming a message in parts
///   and send it.
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

/// Get the payload size of the message that is being processed.
///
/// This function returns the payload size of the current message that is being
/// processed.
///
/// # Examples
///
/// ```
/// use gcore::msg;
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     let payload_size = msg::size();
/// }
/// ```
pub fn size() -> usize {
    let mut size = 0u32;
    unsafe { gsys::gr_size(&mut size as *mut u32) };
    size as usize
}

/// Get the identifier of the message source (256-bit address).
///
/// This function is used to obtain the [`ActorId`] of the account that sends
/// the currently processing message (either a program or a user).
///
/// # Examples
///
/// ```
/// use gcore::msg;
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     let who_sends_message = msg::source();
/// }
/// ```
pub fn source() -> ActorId {
    let mut source = ActorId::default();
    unsafe { gsys::gr_source(source.as_mut_ptr()) }
    source
}

/// Get the value associated with the message that is being processed.
///
/// This function returns the value that has been sent along with a current
/// message that is being processed.
///
/// # Examples
///
/// ```
/// use gcore::msg;
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     let amount_sent_with_message = msg::value();
/// }
/// ```
pub fn value() -> u128 {
    let mut value = 0u128;
    unsafe { gsys::gr_value(&mut value as *mut u128) };
    value
}
