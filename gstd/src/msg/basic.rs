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

use crate::{
    async_runtime::signals,
    errors::{IntoContractResult, Result},
    msg::{utils, CodecMessageFuture, MessageFuture},
    prelude::{convert::AsRef, ops::RangeBounds, vec, Vec},
    ActorId, MessageId, ReservationId,
};
use gstd_codegen::wait_for_reply;
use scale_info::scale::{Decode, Output};

/// Message handle.
///
/// Gear allows users and program interaction via
/// messages. Message creation consists of the following parts: message
/// initialization, filling the message with payload (can be gradual), and
/// message sending.
///
/// /// Here are the functions that constitute the parts of forming and sending
/// messages:
///
/// - [`MessageHandle::init`] initializes the message
/// - [`MessageHandle::push`] adds a payload to a message
/// - [`MessageHandle::commit`] sends a message
///
/// The send transaction will be posted only after the execution of the message
/// processing has been finished.
///
/// To identify a message that is being built from parts of a program, you
/// should use `MessageHandle` obtained via [`MessageHandle::init`].
///
/// # Examples
///
/// ```
/// use gstd::msg::{self, MessageHandle};
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     let msg_handle = MessageHandle::init().expect("Unable to init");
///     msg_handle.push(b"Hello,").expect("Unable to push");
///     msg_handle.push(b" world!").expect("Unable to push");
///     msg_handle
///         .commit(msg::source(), 0)
///         .expect("Unable to commit");
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MessageHandle(gcore::MessageHandle);

impl MessageHandle {
    /// Initialize a message to send formed in parts.
    ///
    /// Gear allows programs to work with messages that consist of several
    /// parts. This function initializes a message built in parts and
    /// returns the corresponding `MessageHandle`.
    pub fn init() -> Result<Self> {
        gcore::msg::send_init().into_contract_result()
    }

    /// Push a payload part of the message to be sent in parts.
    ///
    /// Gear allows programs to work with messages in parts.
    /// This function adds a `payload` part to the message.
    pub fn push<T: AsRef<[u8]>>(&self, payload: T) -> Result<()> {
        gcore::msg::send_push(self.0, payload.as_ref()).into_contract_result()
    }

    /// Same as [`push`](Self::push) but uses the input buffer as a payload
    /// source.
    ///
    /// The argument of this method is the index range defining the input
    /// buffer's piece to be pushed back to the output.
    ///
    /// # Examples
    ///
    /// Send half of the incoming payload back to the sender.
    ///
    /// ```
    /// use gstd::msg::{self, MessageHandle};
    ///
    /// #[no_mangle]
    /// extern "C" fn handle() {
    ///     let msg_handle = MessageHandle::init().expect("Unable to init");
    ///     msg_handle
    ///         .push_input(0..msg::size() / 2)
    ///         .expect("Unable to push");
    ///     msg_handle
    ///         .commit(msg::source(), 0)
    ///         .expect("Unable to commit");
    /// }
    /// ```
    pub fn push_input<Range: RangeBounds<usize>>(&self, range: Range) -> Result<()> {
        let (offset, len) = utils::decay_range(range);
        gcore::msg::send_push_input(self.0, offset, len).into_contract_result()
    }

    /// Finalize and send the message formed in parts.
    ///
    /// Gear allows programs to work with messages that consist of several
    /// parts. This function finalizes the message built in parts and sends
    /// it.
    ///
    /// The first argument is the address of the target account. The second
    /// argument is the value to be transferred from the current program account
    /// to the message target account.
    #[wait_for_reply(self)]
    pub fn commit(self, program: ActorId, value: u128) -> Result<MessageId> {
        gcore::msg::send_commit(self.0, program.into(), value).into_contract_result()
    }

    /// Same as [`commit`](Self::commit), but sends the message after the
    /// `delay` expressed in block count.
    pub fn commit_delayed(self, program: ActorId, value: u128, delay: u32) -> Result<MessageId> {
        gcore::msg::send_commit_delayed(self.0, program.into(), value, delay).into_contract_result()
    }

    /// Same as [`commit`](Self::commit), but with an explicit gas
    /// limit.
    ///
    /// # Examples
    ///
    /// ```
    /// use gstd::msg::{self, MessageHandle};
    ///
    /// #[no_mangle]
    /// extern "C" fn handle() {
    ///     let msg_handle = MessageHandle::init().expect("Unable to init");
    ///     msg_handle.push(b"Hello,").expect("Unable to push");
    ///     msg_handle.push(b" world!").expect("Unable to push");
    ///     msg_handle
    ///         .commit_with_gas(msg::source(), 10_000_000, 42)
    ///         .expect("Unable to commit");
    /// }
    /// ```
    #[wait_for_reply(self)]
    pub fn commit_with_gas(
        self,
        program: ActorId,
        gas_limit: u64,
        value: u128,
    ) -> Result<MessageId> {
        gcore::msg::send_commit_with_gas(self.0, program.into(), gas_limit, value)
            .into_contract_result()
    }

    /// Same as [`commit_with_gas`](Self::commit_with_gas), but sends
    /// the message after the `delay` expressed in block count.
    pub fn commit_with_gas_delayed(
        self,
        program: ActorId,
        gas_limit: u64,
        value: u128,
        delay: u32,
    ) -> Result<MessageId> {
        gcore::msg::send_commit_with_gas_delayed(self.0, program.into(), gas_limit, value, delay)
            .into_contract_result()
    }

    /// Same as [`commit`](Self::commit), but it spends gas from the
    /// reservation instead of borrowing from the gas limit provided with the
    /// incoming message.
    ///
    /// # Examples
    ///
    /// ```
    /// use gstd::{
    ///     msg::{self, MessageHandle},
    ///     ReservationId,
    /// };
    ///
    /// #[no_mangle]
    /// extern "C" fn handle() {
    ///     let reservation_id = ReservationId::reserve(5_000_000, 100).expect("Unable to reserve");
    ///     let msg_handle = MessageHandle::init().expect("Unable to init");
    ///     msg_handle.push(b"Hello,").expect("Unable to push");
    ///     msg_handle.push(b" world!").expect("Unable to push");
    ///     msg_handle
    ///         .commit_from_reservation(reservation_id, msg::source(), 42)
    ///         .expect("Unable to commit");
    /// }
    /// ```
    #[wait_for_reply(self)]
    pub fn commit_from_reservation(
        self,
        id: ReservationId,
        program: ActorId,
        value: u128,
    ) -> Result<MessageId> {
        gcore::msg::send_commit_from_reservation(id.into(), self.into(), program.into(), value)
            .into_contract_result()
    }

    /// Same as [`commit_from_reservation`](Self::commit_from_reservation), but
    /// sends the message after the `delay` expressed in block count.
    pub fn commit_delayed_from_reservation(
        self,
        id: ReservationId,
        program: ActorId,
        value: u128,
        delay: u32,
    ) -> Result<MessageId> {
        gcore::msg::send_commit_delayed_from_reservation(
            id.into(),
            self.into(),
            program.into(),
            value,
            delay,
        )
        .into_contract_result()
    }
}

impl Output for MessageHandle {
    fn write(&mut self, bytes: &[u8]) {
        self.push(bytes).unwrap();
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

/// Get the status code of the message being processed.
///
/// This function is used in the reply handler to check whether the message was
/// processed successfully or not.
///
/// # Examples
///
/// ```
/// use gstd::msg;
///
/// #[no_mangle]
/// extern "C" fn handle_reply() {
///     let status_code = msg::status_code().expect("Unable to get status code");
/// }
/// ```
pub fn status_code() -> Result<i32> {
    gcore::msg::status_code().map_err(Into::into)
}

/// Get an identifier of the message that is currently being processed.
///
/// One can get an identifier for the currently processing message; each send
/// and reply function also returns a message identifier.
///
/// # Examples
///
/// ```
/// use gstd::{msg, MessageId};
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
    gcore::msg::id().into()
}

/// Get a payload of the message that is currently being processed.
///
/// This function returns the message's payload as a byte vector.
///
/// # Examples
///
/// ```
/// use gstd::msg;
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     let payload = msg::load_bytes().expect("Unable to load");
/// }
/// ```
///
/// # See also
///
/// - [`load`](super::load) function returns a decoded payload of a custom type.
pub fn load_bytes() -> Result<Vec<u8>> {
    let mut result = vec![0u8; size()];
    gcore::msg::read(result.as_mut())?;
    Ok(result)
}

/// Send a new message as a reply to the message that is currently being
/// processed.
///
/// Various programs can communicate with each other, e.g., check another
/// program's state and use it as a parameter for its business logic.
///
/// This function allows sending such replies, which are similar to standard
/// messages in terms of payload and different only in how the message
/// processing is handled by a dedicated program function called `handle_reply`.
///
/// The first argument is the payload buffer. The second argument is the value
/// to be transferred from the current program account to the reply message
/// target account.
///
/// Reply message transactions will be posted after processing is complete,
/// similar to the standard message-sending function (e.g. [`send_bytes`]).
///
/// # Examples
///
/// ```
/// use gstd::{exec, msg};
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     msg::reply_bytes(b"PING", exec::value_available()).expect("Unable to reply");
/// }
/// ```
///
/// # See also
///
/// - [`reply`](super::reply) function sends a reply with an encoded payload.
/// - [`reply_bytes_delayed`] function sends a reply after the delay.
/// - [`reply_push`], [`reply_commit`] functions allow forming a reply message
///   in parts.
/// - [`send_bytes`] function sends a new message to the program or user.
#[wait_for_reply]
pub fn reply_bytes(payload: impl AsRef<[u8]>, value: u128) -> Result<MessageId> {
    gcore::msg::reply(payload.as_ref(), value).into_contract_result()
}

/// Same as [`reply_bytes`], but sends the reply after the `delay` expressed in
/// block count.
pub fn reply_bytes_delayed(
    payload: impl AsRef<[u8]>,
    value: u128,
    delay: u32,
) -> Result<MessageId> {
    gcore::msg::reply_delayed(payload.as_ref(), value, delay).into_contract_result()
}

/// Same as [`reply_bytes`], but it spends gas from a reservation instead of
/// borrowing it from the gas limit provided with the incoming message.
///
/// The first argument is the reservation identifier [`ReservationId`] obtained
/// by calling the corresponding API. The second argument is the payload buffer.
/// The last argument is the value to be transferred from the current program
/// account to the reply message target account.
///
/// # Examples
///
/// ```
/// use gstd::{msg, ReservationId};
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     let reservation_id = ReservationId::reserve(5_000_000, 100).expect("Unable to reserve");
///     msg::reply_from_reservation(reservation_id, b"PING", 0).unwrap();
/// }
/// ```
///
/// # See also
///
/// - [`send_bytes_from_reservation`] function sends a new message to the
///   program or user by using gas from a reservation.
#[wait_for_reply]
pub fn reply_bytes_from_reservation(
    id: ReservationId,
    payload: impl AsRef<[u8]>,
    value: u128,
) -> Result<MessageId> {
    gcore::msg::reply_from_reservation(id.into(), payload.as_ref(), value).into_contract_result()
}

/// Same as [`reply_bytes_from_reservation`], but sends the reply after the
/// `delay` expressed in block count.
pub fn reply_bytes_delayed_from_reservation(
    id: ReservationId,
    payload: impl AsRef<[u8]>,
    value: u128,
    delay: u32,
) -> Result<MessageId> {
    gcore::msg::reply_delayed_from_reservation(id.into(), payload.as_ref(), value, delay)
        .into_contract_result()
}

/// Same as [`reply_bytes`], but with an explicit gas limit.
///
/// # Examples
///
/// ```
/// use gstd::{exec, msg};
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     msg::reply_bytes_with_gas(b"PING", exec::gas_available() / 2, 0).expect("Unable to reply");
/// }
/// ```
#[wait_for_reply]
pub fn reply_bytes_with_gas(
    payload: impl AsRef<[u8]>,
    gas_limit: u64,
    value: u128,
) -> Result<MessageId> {
    gcore::msg::reply_with_gas(payload.as_ref(), gas_limit, value).into_contract_result()
}

/// Same as [`reply_bytes_with_gas`], but sends the reply after the `delay`
/// expressed in block count.
pub fn reply_bytes_with_gas_delayed(
    payload: impl AsRef<[u8]>,
    gas_limit: u64,
    value: u128,
    delay: u32,
) -> Result<MessageId> {
    gcore::msg::reply_with_gas_delayed(payload.as_ref(), gas_limit, value, delay)
        .into_contract_result()
}

/// Finalize and send the current reply message.
///
/// Some programs can rely on their messages to other programs, i.e., check
/// another program's state and use it as a parameter for its own business
/// logic. The basic implementation is covered in [`reply`](super::reply)
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
/// use gstd::msg;
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
/// - [`MessageHandle::commit`] function finalizes and sends a message formed in
///   parts.
#[wait_for_reply]
pub fn reply_commit(value: u128) -> Result<MessageId> {
    gcore::msg::reply_commit(value).into_contract_result()
}

/// Same as [`reply_commit`], but sends the reply after the `delay` expressed in
/// block count.
pub fn reply_commit_delayed(value: u128, delay: u32) -> Result<MessageId> {
    gcore::msg::reply_commit_delayed(value, delay).into_contract_result()
}

/// Same as [`reply_commit`], but it spends gas from a reservation instead of
/// borrowing it from the gas limit provided with the incoming message.
///
/// # Examples
///
/// ```
/// use gstd::{msg, ReservationId};
///
/// extern "C" fn handle() {
///     msg::reply_push(b"Hello,").expect("Unable to push");
///     msg::reply_push(b" world!").expect("Unable to push");
///     let resevation_id = ReservationId::reserve(5_000_000, 100).expect("Unable to reserves");
///     msg::reply_commit_from_reservation(resevation_id, 42).expect("Unable to commit");
/// }
/// ```
///
/// # See also
///
/// - [`reply_push`] function allows forming a reply message in parts.
/// - [`ReservationId`] struct allows reserve gas for later use.
#[wait_for_reply]
pub fn reply_commit_from_reservation(id: ReservationId, value: u128) -> Result<MessageId> {
    gcore::msg::reply_commit_from_reservation(id.into(), value).into_contract_result()
}

/// Same as [`reply_commit_from_reservation`], but sends the message after the
/// `delay` expressed in block count.
pub fn reply_commit_delayed_from_reservation(
    id: ReservationId,
    value: u128,
    delay: u32,
) -> Result<MessageId> {
    gcore::msg::reply_commit_delayed_from_reservation(id.into(), value, delay)
        .into_contract_result()
}

/// Same as [`reply_commit`], but with an explicit gas limit.
///
/// # Examples
///
/// ```
/// use gstd::{exec, msg};
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
#[wait_for_reply]
pub fn reply_commit_with_gas(gas_limit: u64, value: u128) -> Result<MessageId> {
    gcore::msg::reply_commit_with_gas(gas_limit, value).into_contract_result()
}

/// Same as [`reply_commit_with_gas`], but sends the reply after the `delay`
/// expressed in block count.
pub fn reply_commit_with_gas_delayed(gas_limit: u64, value: u128, delay: u32) -> Result<MessageId> {
    gcore::msg::reply_commit_with_gas_delayed(gas_limit, value, delay).into_contract_result()
}

/// Push a payload part to the current reply message.
///
/// Some programs can rely on their messages to other programs, i.e., check
/// another program's state and use it as a parameter for its own business
/// logic. The basic implementation is covered in the [`reply_bytes`] function.
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
pub fn reply_push<T: AsRef<[u8]>>(payload: T) -> Result<()> {
    gcore::msg::reply_push(payload.as_ref()).into_contract_result()
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
/// use gstd::msg;
///
/// #[no_mangle]
/// extern "C" fn handle_reply() {
///     let original_message_id = msg::reply_to().unwrap();
/// }
/// ```
pub fn reply_to() -> Result<MessageId> {
    gcore::msg::reply_to().into_contract_result()
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
/// use gstd::msg;
///
/// #[no_mangle]
/// extern "C" fn handle_signal() {
///     let erroneous_message = msg::signal_from().unwrap();
/// }
/// ```
pub fn signal_from() -> Result<MessageId> {
    gcore::msg::signal_from().into_contract_result()
}

/// Same as [`reply_push`] but uses the input buffer as a payload source.
///
/// The argument of this method is the index range defining the input
/// buffer's piece to be pushed back to the output.
///
/// # Examples
///
/// Send half of the incoming payload back to the sender as a reply.
///
/// ```
/// use gstd::msg;
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     msg::reply_push_input(0..msg::size() / 2).expect("Unable to push");
///     msg::reply_commit(0).expect("Unable to commit");
/// }
/// ```
///
/// # See also
///
/// - [`MessageHandle::push_input`] function allows using the input buffer as a
///   payload source for an outcoming message.
pub fn reply_push_input<Range: RangeBounds<usize>>(range: Range) -> Result<()> {
    let (offset, len) = utils::decay_range(range);

    gcore::msg::reply_push_input(offset, len).into_contract_result()
}

/// Send a new message to the program or user.
///
/// Gear allows programs to communicate with each other and users via messages.
/// For example, the [`send_bytes`] function allows sending such messages.
///
/// The first argument is the address of the target account ([`ActorId`]). The
/// second argument is the payload buffer. The last argument is the value to be
/// transferred from the current program account to the message target account.
///
/// Send transaction will be posted after processing is finished, similar to the
/// reply message [`reply_bytes`].
///
/// # Examples
///
/// Send a message with value to the arbitrary address (don't repeat it in your
/// program!):
///
/// ```
/// use gstd::{msg, ActorId};
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     // Receiver id is collected from bytes from 0 to 31
///     let id: [u8; 32] = core::array::from_fn(|i| i as u8);
///     msg::send_bytes(ActorId::new(id), b"HELLO", 42);
/// }
/// ```
///
/// # See also
///
/// - [`reply_bytes`] function sends a new message as a reply to the message
///   that is currently being processed.
/// - [`MessageHandle::init`], [`MessageHandle::push`], and
///   [`MessageHandle::commit`] functions allow forming a message to send in
///   parts.
#[wait_for_reply]
pub fn send_bytes<T: AsRef<[u8]>>(program: ActorId, payload: T, value: u128) -> Result<MessageId> {
    gcore::msg::send(program.into(), payload.as_ref(), value).into_contract_result()
}

/// Same as [`send_bytes`], but sends the message after the `delay` expressed in
/// block count.
pub fn send_bytes_delayed<T: AsRef<[u8]>>(
    program: ActorId,
    payload: T,
    value: u128,
    delay: u32,
) -> Result<MessageId> {
    gcore::msg::send_delayed(program.into(), payload.as_ref(), value, delay).into_contract_result()
}

/// Same as [`send_bytes`], but with an explicit gas limit.
///
/// # Examples
///
/// Send a message with gas limit and value to the arbitrary address (don't
/// repeat it in your program!):
///
/// ```
/// use gstd::{msg, ActorId};
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     // Receiver id is collected from bytes from 0 to 31
///     let id: [u8; 32] = core::array::from_fn(|i| i as u8);
///     msg::send_bytes_with_gas(ActorId::new(id), b"HELLO", 5_000_000, 42);
/// }
/// ```
///
/// # See also
///
/// - [`reply_bytes_with_gas`] function sends a reply with an explicit gas
///   limit.
/// - [`MessageHandle::init`], [`MessageHandle::push`], and
///   [`MessageHandle::commit`] functions allow forming a message to send in
///   parts.
#[wait_for_reply]
pub fn send_bytes_with_gas<T: AsRef<[u8]>>(
    program: ActorId,
    payload: T,
    gas_limit: u64,
    value: u128,
) -> Result<MessageId> {
    gcore::msg::send_with_gas(program.into(), payload.as_ref(), gas_limit, value)
        .into_contract_result()
}

/// Same as [`send_bytes_with_gas`], but sends the message after the `delay`
/// expressed in block count.
pub fn send_bytes_with_gas_delayed<T: AsRef<[u8]>>(
    program: ActorId,
    payload: T,
    gas_limit: u64,
    value: u128,
    delay: u32,
) -> Result<MessageId> {
    gcore::msg::send_with_gas_delayed(program.into(), payload.as_ref(), gas_limit, value, delay)
        .into_contract_result()
}

/// Same as [`send_bytes`], but it spends gas from a reservation instead of
/// borrowing it from the gas limit provided with the incoming message.
///
/// The first argument is the reservation identifier [`ReservationId`] obtained
/// by calling the corresponding API. The second argument is the address of the
/// target account ([`ActorId`]). The third argument is the payload buffer.
/// Finally, the last argument is the value to be transferred from the current
/// program account to the message target account.
///
/// # Examples
///
/// Send a message with value to the sender's address:
///
/// ```
/// use gstd::{msg, ReservationId};
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     // Reserve 5 million of gas for 100 blocks
///     let reservation_id = ReservationId::reserve(5_000_000, 100).expect("Unable to reserve");
///     // Receiver id is the message source
///     let actor_id = msg::source();
///     msg::send_from_reservation(reservation_id, actor_id, b"HELLO", 42).expect("Unable to send");
/// }
/// ```
///
/// # See also
///
/// - [`reply_bytes_from_reservation`] function sends a reply to the program or
///   user by using gas from a reservation.
/// - [`MessageHandle::init`], [`MessageHandle::push`], and
///   [`MessageHandle::commit`] functions allow forming a message to send in
///   parts.
#[wait_for_reply]
pub fn send_bytes_from_reservation<T: AsRef<[u8]>>(
    id: ReservationId,
    program: ActorId,
    payload: T,
    value: u128,
) -> Result<MessageId> {
    gcore::msg::send_from_reservation(id.into(), program.into(), payload.as_ref(), value)
        .into_contract_result()
}

/// Same as [`send_bytes_from_reservation`], but sends the message after the
/// `delay` expressed in block count.
pub fn send_bytes_delayed_from_reservation<T: AsRef<[u8]>>(
    id: ReservationId,
    program: ActorId,
    payload: T,
    value: u128,
    delay: u32,
) -> Result<MessageId> {
    gcore::msg::send_delayed_from_reservation(
        id.into(),
        program.into(),
        payload.as_ref(),
        value,
        delay,
    )
    .into_contract_result()
}

/// Get the payload size of the message that is being processed.
///
/// This function returns the payload size of the current message that is being
/// processed.
///
/// # Examples
///
/// ```
/// use gstd::msg;
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     let payload_size = msg::size();
/// }
/// ```
pub fn size() -> usize {
    gcore::msg::size()
}

/// Get the identifier of the message source (256-bit address).
///
/// This function is used to obtain the [`ActorId`] of the account that sends
/// the currently processing message (either a program or a user).
///
/// # Examples
///
/// ```
/// use gstd::msg;
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     let who_sends_message = msg::source();
/// }
/// ```
pub fn source() -> ActorId {
    gcore::msg::source().into()
}

/// Get the value associated with the message that is being processed.
///
/// This function returns the value sent along with a current
/// message being processed.
///
/// # Examples
///
/// ```
/// use gstd::msg;
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     let amount_sent_with_message = msg::value();
/// }
/// ```
pub fn value() -> u128 {
    gcore::msg::value()
}
