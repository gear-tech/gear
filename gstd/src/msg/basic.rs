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

//! Module with basic messaging functions wrapped from `gcore` to `gstd`.

use crate::prelude::{convert::AsRef, vec, Vec};
use crate::{ActorId, MessageId};
use codec::Output;

/// Message handle.
///
/// Gear allows users and programs to interact with other users and programs via
/// messages. Message creation consists of the following parts - message
/// initialisation, filling the message with payload (can be gradual), message
/// sending.
///
/// Here are the functions that make up the parts of building and sending
/// messages:
/// [`msg::send_init`](crate::msg::send_init) - message initialization.
/// [`msg::send_push`](crate::msg::send_push) - adds a `payload` part to the
/// message  specified by `MessageHandle`.
/// [`msg::send_commit`](crate::msg::send_commit) - send a message with the
/// following arguments:
///     - the address of the target account.
///     - the gas_limit - maximum gas allowed to be utilized during
///     reply message processing.
///     - the value to be transferred from the current program account
///     to the message target account.
///
/// Send transaction will be posted only after the execution of message
/// processing is finished.
///
/// In order to identify a message that is being built from parts of a program
/// you should use `MessageHandle` obtained via
/// [`msg::send_init`](crate::msg::send_init).
///
/// # Examples
///
/// ```
/// use gstd::msg;
///
/// pub unsafe extern "C" fn handle() {
///     // ...
///     let msg_handle = msg::send_init();
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MessageHandle(gcore::MessageHandle);

impl MessageHandle {
    pub fn init() -> Self {
        send_init()
    }

    pub fn push<T: AsRef<[u8]>>(&self, payload: T) {
        send_push(self, payload);
    }

    pub fn commit(self, program: ActorId, gas_limit: u64, value: u128) -> MessageId {
        send_commit(self, program, gas_limit, value)
    }
}

impl Output for MessageHandle {
    fn write(&mut self, bytes: &[u8]) {
        self.push(bytes);
    }
}

impl AsRef<gcore::MessageHandle> for MessageHandle {
    fn as_ref(&self) -> &gcore::MessageHandle {
        &self.0
    }
}

impl From<MessageHandle> for gcore::MessageHandle {
    fn from(other: MessageHandle) -> Self {
        other.0
    }
}

impl From<gcore::MessageHandle> for MessageHandle {
    fn from(other: gcore::MessageHandle) -> Self {
        Self(other)
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
/// use gstd::msg;
///
/// pub unsafe extern "C" fn handle() {
///     // ...
///     let exit_code = msg::exit_code();
/// }
/// ```
pub fn exit_code() -> i32 {
    gcore::msg::exit_code()
}

/// Obtain an identifier of the message currently being processed.
///
/// Message identifiers can be obtained for the currently processed message,
/// also each send and reply functions return a message identifier.
///
/// # Examples
///
/// ```
/// use gstd::msg;
///
/// pub unsafe extern "C" fn handle() {
///     let current_message_id = msg::id();
/// }
/// ```
pub fn id() -> MessageId {
    gcore::msg::id().into()
}

/// Get a payload of the message currently being processed.
///
/// Loads payload of the message into a buffer with a message size which can be
/// obtained using the [`size`] function.
///
/// # Examples
///
/// ```
/// use gstd::msg;
///
/// pub unsafe extern "C" fn handle() {
///     let payload_bytes = msg::load_bytes();
/// }
/// ```
pub fn load_bytes() -> Vec<u8> {
    let mut result = vec![0u8; size()];
    gcore::msg::load(&mut result[..]);
    result
}

/// Send a new message as a reply to the message currently being processed.
///
/// Some programs can reply to other programs, i.e. check another program's
/// state and use it as a parameter for its own business logic [`MessageId`].
///
/// This function allows sending such replies, which are similar to standard
/// messages in terms of payload and different only in the way the message
/// processing is handled by a separate program function called
/// `handle_reply`.
///
/// First argument is the reply message payload in bytes.
/// Second argument is `gas_limit` - maximum gas allowed to be utilized
/// during the reply message processing.
/// Last argument `value` is the value to be transferred from the current
/// program account to the reply message target account.
///
/// Reply message transactions will be posted only after processing is finished,
/// similar to the standard message [`send`].
///
/// # Examples
///
/// ```
/// use gstd::{exec, msg};
///
/// pub unsafe extern "C" fn handle() {
///     // ...
///     msg::reply_bytes(b"PING", exec::gas_available(), 0);
/// }
/// ```
///
/// # See also
///
/// [`reply_push`] function allows to form a reply message in parts.
pub fn reply_bytes<T: AsRef<[u8]>>(payload: T, gas_limit: u64, value: u128) -> MessageId {
    gcore::msg::reply(payload.as_ref(), gas_limit, value).into()
}

/// Finalize and send a current reply message.
///
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
/// use gstd::{exec, msg};
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
    gcore::msg::reply_commit(gas_limit, value).into()
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
/// use gstd::msg;
///
/// pub unsafe extern "C" fn handle() {
///     // ...
///     msg::reply_push(b"Part 1");
///     // ...
///     msg::reply_push(b"Part 2");
/// }
/// ```
pub fn reply_push<T: AsRef<[u8]>>(payload: T) {
    gcore::msg::reply_push(payload.as_ref());
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
/// use gstd::msg;
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
    gcore::msg::reply_to().into()
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
///
/// Send transaction will be posted only after the execution of processing is
/// finished, similar to the reply message [`reply`].
///
/// # Examples
///
/// ```
/// use gstd::{msg, ActorId};
///
/// pub unsafe extern "C" fn handle() {
///     // ...
///     let id = msg::source();
///
///     msg::send_bytes(id, b"HELLO", 1000, 12345678);
/// }
/// ```
///
/// # See also
///
/// [`send_init`],[`send_push`], [`send_commit`] functions allows to form a
/// message to send in parts.
pub fn send_bytes<T: AsRef<[u8]>>(
    program: ActorId,
    payload: T,
    gas_limit: u64,
    value: u128,
) -> MessageId {
    gcore::msg::send(program.into(), payload.as_ref(), gas_limit, value).into()
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
/// use gstd::{exec, msg};
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
    gcore::msg::send_commit(handle.into(), program.into(), gas_limit, value).into()
}

/// Initialize a message to send, formed in parts.
///
/// Gear allows programs to work with messages that consist of several parts.
/// This function initializes a message built in parts and returns corresponding
/// message `handle`.
///
/// # Examples
///
/// ```
/// use gstd::{exec, msg};
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
    gcore::msg::send_init().into()
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
/// use gstd::{exec, msg};
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
pub fn send_push<T: AsRef<[u8]>>(handle: &MessageHandle, payload: T) {
    gcore::msg::send_push(handle.as_ref(), payload.as_ref())
}

/// Get the payload size of the message being processed.
///
/// This function is used to obtain the payload size of the current message
/// being processed.
///
/// # Examples
///
/// ```
/// use gstd::msg;
///
/// pub unsafe extern "C" fn handle() {
///     // ...
///     let payload_size = msg::size();
/// }
/// ```
pub fn size() -> usize {
    gcore::msg::size()
}

/// Get the identifier of the message source (256-bit address).
///
/// This function is used to obtain [`ActorId`] of the account that sends
/// the currently processing message (either a program or a user).
///
/// # Examples
///
/// ```
/// use gstd::msg;
///
/// pub unsafe extern "C" fn handle() {
///     // ...
///     let who_sends_message = msg::source();
/// }
/// ```
pub fn source() -> ActorId {
    gcore::msg::source().into()
}

/// Get the value associated with the message being processed.
///
/// This function is used to obtain the value that has been sent along with
/// a current message being processed.
///
/// # Examples
///
/// ```
/// use gstd::msg;
///
/// pub unsafe extern "C" fn handle() {
///     // ...
///     let amount_sent_with_message = msg::value();
/// }
/// ```
pub fn value() -> u128 {
    gcore::msg::value()
}
