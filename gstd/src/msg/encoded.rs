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

//! Module with messaging functions (`load`, `reply`, `send`) for operating
//! with messages arguments as with data structure instead of bytes array
//! decoded/encoded via SCALE Codec (<https://docs.substrate.io/v3/advanced/scale-codec/>).

use crate::{
    async_runtime::signals,
    errors::{ContractError, IntoContractResult, Result},
    msg::{utils, CodecMessageFuture, MessageFuture},
    prelude::{convert::AsRef, ops::RangeBounds},
    ActorId, MessageId, ReservationId,
};
use gstd_codegen::wait_for_reply;
use scale_info::scale::{Decode, Encode};

/// Get a payload of the message that is currently being processed.
///
/// This function returns the decoded message's payload of a custom type.
///
/// Generic `D` type should implement the [`Decode`] trait.
///
/// # Examples
///
/// ```
/// use gstd::{msg, prelude::*};
///
/// #[derive(Decode)]
/// #[codec(crate = gstd::codec)]
/// struct Input {
///     field: String,
/// }
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     let payload: Input = msg::load().expect("Unable to load");
///     msg::reply(payload.field, 0).expect("Unable to reply");
/// }
/// ```
///
/// # See also
///
/// - [`load_bytes`](super::load_bytes) function returns a payload as a byte
///   vector.
pub fn load<D: Decode>() -> Result<D> {
    D::decode(&mut super::load_bytes()?.as_ref()).map_err(ContractError::Decode)
}

/// Send a new message as a reply to the message being
/// processed.
///
/// Some programs can reply to other programs, e.g., check another program's
/// state and use it as a parameter for its business logic.
///
/// This function allows sending such replies, which are similar to standard
/// messages in terms of payload and different only in how the message
/// processing is handled by a dedicated program function called `handle_reply`.
///
/// The first argument is the encodable payload. The second argument is the
/// value to be transferred from the current program account to the reply
/// message target account.
///
/// Reply message transactions will be posted after processing is finished,
/// similar to the standard message-sending function (e.g. [`send`]).
///
/// # Examples
///
/// ```
/// use gstd::{msg, prelude::*};
///
/// #[derive(Encode)]
/// #[codec(crate = gstd::codec)]
/// struct Reply {
///     a: i32,
///     b: Option<bool>,
/// }
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     let payload = Reply {
///         a: 42,
///         b: Some(true),
///     };
///
///     msg::reply(payload, 0).expect("Unable to reply");
/// }
/// ```
///
/// # See also
///
/// - [`reply_bytes`](super::reply_bytes) function sends a reply with an encoded
///   payload.
/// - [`reply_push`](super::reply_push), [`reply_commit`](super::reply_commit)
///   functions allow forming a reply message in parts.
/// - [`send`] function sends a new message to the program or user.
#[wait_for_reply]
pub fn reply<E: Encode>(payload: E, value: u128) -> Result<MessageId> {
    super::reply_bytes(payload.encode(), value)
}

/// Same as [`reply`], but sends the reply after the `delay` expressed in block
/// count.
pub fn reply_delayed<E: Encode>(payload: E, value: u128, delay: u32) -> Result<MessageId> {
    super::reply_bytes_delayed(payload.encode(), value, delay)
}

/// Same as [`reply`], but it spends gas from a reservation instead of
/// borrowing gas from the gas limit provided with the incoming message.
///
/// The first argument is the reservation identifier [`ReservationId`] obtained
/// by calling the corresponding API. The second argument is the encodable
/// payload. The last argument is the value to be transferred from the current
/// program account to the reply message target account.
///
/// # Examples
///
/// ```
/// use gstd::{msg, prelude::*, ReservationId};
///
/// #[derive(Encode)]
/// #[codec(crate = gstd::codec)]
/// struct Reply {
///     a: i32,
///     b: Option<bool>,
/// }
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     let reservation_id = ReservationId::reserve(5_000_000, 100).expect("Unable to reserve");
///     let payload = Reply {
///         a: 42,
///         b: Some(true),
///     };
///
///     msg::reply_from_reservation(reservation_id, payload, 0).unwrap();
/// }
/// ```
///
/// # See also
///
/// - [`send_from_reservation`] function sends a new message to the program or
///   user by using gas from a reservation.
#[wait_for_reply]
pub fn reply_from_reservation<E: Encode>(
    id: ReservationId,
    payload: E,
    value: u128,
) -> Result<MessageId> {
    super::reply_bytes_from_reservation(id, payload.encode(), value)
}

/// Same as [`reply_from_reservation`], but sends the reply after the `delay`
/// expressed in block count.
pub fn reply_delayed_from_reservation<E: Encode>(
    id: ReservationId,
    payload: E,
    value: u128,
    delay: u32,
) -> Result<MessageId> {
    super::reply_bytes_delayed_from_reservation(id, payload.encode(), value, delay)
}

/// Same as [`reply`], but with an explicit gas limit.
///
/// # Examples
///
/// ```
/// use gstd::{exec, msg, prelude::*};
///
/// #[derive(Encode)]
/// #[codec(crate = gstd::codec)]
/// struct Reply {
///     a: i32,
///     b: Option<bool>,
/// }
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     let payload = Reply {
///         a: 42,
///         b: Some(true),
///     };
///
///     msg::reply_with_gas(payload, exec::gas_available() / 2, 0).expect("Unable to reply");
/// }
/// ```
#[wait_for_reply]
pub fn reply_with_gas<E: Encode>(payload: E, gas_limit: u64, value: u128) -> Result<MessageId> {
    super::reply_bytes_with_gas(payload.encode(), gas_limit, value)
}

/// Same as [`reply_with_gas`], but sends the reply after the `delay` expressed
/// in block count.
pub fn reply_with_gas_delayed<E: Encode>(
    payload: E,
    gas_limit: u64,
    value: u128,
    delay: u32,
) -> Result<MessageId> {
    super::reply_bytes_with_gas_delayed(payload.encode(), gas_limit, value, delay)
}

/// Same as [`reply`] but uses the input buffer as a payload source.
///
/// The first argument is the value to be transferred from the current program
/// account to the reply message target account. The second argument is the
/// index range that defines the input buffer's piece to be pushed back to the
/// output.
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
///     msg::reply_input(0, 0..msg::size() / 2).expect("Unable to reply");
/// }
/// ```
///
/// # See also
///
/// - [`super::reply_push_input`] function pushes a payload part to the current
///   reply message using the input buffer as a payload source.
/// - [`MessageHandle::push_input`](super::MessageHandle::push_input) function
///   allows using the input buffer as a payload source for an outcoming
///   message.
#[wait_for_reply]
pub fn reply_input<Range: RangeBounds<usize>>(value: u128, range: Range) -> Result<MessageId> {
    let (offset, len) = utils::decay_range(range);

    gcore::msg::reply_input(value, offset, len).into_contract_result()
}

/// Same as [`reply_input`], but sends the reply after the `delay` expressed in
/// block count.
pub fn reply_input_delayed<Range: RangeBounds<usize>>(
    value: u128,
    range: Range,
    delay: u32,
) -> Result<MessageId> {
    let (offset, len) = utils::decay_range(range);

    gcore::msg::reply_input_delayed(value, offset, len, delay).into_contract_result()
}

/// Same as [`reply_input`], but with an explicit gas limit.
#[wait_for_reply]
pub fn reply_input_with_gas<Range: RangeBounds<usize>>(
    gas_limit: u64,
    value: u128,
    range: Range,
) -> Result<MessageId> {
    let (offset, len) = utils::decay_range(range);

    gcore::msg::reply_input_with_gas(gas_limit, value, offset, len).into_contract_result()
}

/// Same as [`reply_input_with_gas`], but sends the reply after the `delay`
/// expressed in block count.
pub fn reply_input_with_gas_delayed<Range: RangeBounds<usize>>(
    gas_limit: u64,
    value: u128,
    range: Range,
    delay: u32,
) -> Result<MessageId> {
    let (offset, len) = utils::decay_range(range);

    gcore::msg::reply_input_with_gas_delayed(gas_limit, value, offset, len, delay)
        .into_contract_result()
}

/// Same as [`send`] but uses the input buffer as a payload source.
///
/// The first argument is the address of the target account ([`ActorId`]). The
/// second argument is the value to be transferred from the current program
/// account to the message target account. The third argument is the index range
/// that defines the input buffer's piece to be sent to the target account.
///
/// # Examples
///
/// Send half of the incoming payload back to the sender.
///
/// ```
/// use gstd::msg;
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     msg::send_input(msg::source(), 0, 0..msg::size() / 2).expect("Unable to send");
/// }
/// ```
///
/// # See also
///
/// - [`MessageHandle::push_input`](super::MessageHandle::push_input) function
///   allows using the input buffer as a payload source for an outcoming
///   message.
#[wait_for_reply]
pub fn send_input<Range: RangeBounds<usize>>(
    program: ActorId,
    value: u128,
    range: Range,
) -> Result<MessageId> {
    let (offset, len) = utils::decay_range(range);

    gcore::msg::send_input(program.into(), value, offset, len).into_contract_result()
}

/// Same as [`send_input`], but sends the message after the `delay` expressed in
/// block count.
pub fn send_input_delayed<Range: RangeBounds<usize>>(
    program: ActorId,
    value: u128,
    range: Range,
    delay: u32,
) -> Result<MessageId> {
    let (offset, len) = utils::decay_range(range);

    gcore::msg::send_input_delayed(program.into(), value, offset, len, delay).into_contract_result()
}

/// Same as [`send_input`], but with an explicit gas limit.
#[wait_for_reply]
pub fn send_input_with_gas<Range: RangeBounds<usize>>(
    program: ActorId,
    gas_limit: u64,
    value: u128,
    range: Range,
) -> Result<MessageId> {
    let (offset, len) = utils::decay_range(range);

    gcore::msg::send_input_with_gas(program.into(), gas_limit, value, offset, len)
        .into_contract_result()
}

/// Same as [`send_input_with_gas`], but sends the message after the `delay`
/// expressed in block count.
pub fn send_input_with_gas_delayed<Range: RangeBounds<usize>>(
    program: ActorId,
    gas_limit: u64,
    value: u128,
    range: Range,
    delay: u32,
) -> Result<MessageId> {
    let (offset, len) = utils::decay_range(range);

    gcore::msg::send_input_with_gas_delayed(program.into(), gas_limit, value, offset, len, delay)
        .into_contract_result()
}

/// Send a new message to the program or user.
///
/// Gear allows programs to communicate with each other and users via messages.
/// For example, the [`send`] function allows sending such messages.
///
/// The first argument is the address of the target account ([`ActorId`]). The
/// second argument is the encodable payload. The last argument is the value to
/// be transferred from the current program account to the message target
/// account.
///
/// Send transaction will be posted after processing is finished, similar to the
/// reply message [`reply`].
///
/// # Examples
///
/// Send a message to the arbitrary address:
///
/// ```
/// use gstd::{msg, prelude::*, ActorId};
///
/// #[derive(Encode)]
/// #[codec(crate = gstd::codec)]
/// struct Output {
///     a: i32,
///     b: Option<bool>,
/// }
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     let payload = Output {
///         a: 42,
///         b: Some(true),
///     };
///
///     // Receiver id is collected from bytes from 0 to 31
///     let id: [u8; 32] = core::array::from_fn(|i| i as u8);
///     msg::send(ActorId::new(id), payload, 0);
/// }
/// ```
///
/// # See also
///
/// - [`reply`] function sends a new message as a reply to the message that is
///   currently being processed.
/// - [`MessageHandle::init`](super::MessageHandle::init),
///   [`MessageHandle::push`](super::MessageHandle::push), and
///   [`MessageHandle::commit`](super::MessageHandle::commit) functions allow
///   forming a message to send in parts.
#[wait_for_reply]
pub fn send<E: Encode>(program: ActorId, payload: E, value: u128) -> Result<MessageId> {
    super::send_bytes(program, payload.encode(), value)
}

/// Same as [`send`], but sends the message after the `delay` expressed in block
/// count.
pub fn send_delayed<E: Encode>(
    program: ActorId,
    payload: E,
    value: u128,
    delay: u32,
) -> Result<MessageId> {
    super::send_bytes_delayed(program, payload.encode(), value, delay)
}

/// Same as [`send`], but with an explicit gas limit.
#[wait_for_reply]
pub fn send_with_gas<E: Encode>(
    program: ActorId,
    payload: E,
    gas_limit: u64,
    value: u128,
) -> Result<MessageId> {
    super::send_bytes_with_gas(program, payload.encode(), gas_limit, value)
}

/// Same as [`send_with_gas`], but sends the message after the `delay` expressed
/// in block count.
pub fn send_with_gas_delayed<E: Encode>(
    program: ActorId,
    payload: E,
    gas_limit: u64,
    value: u128,
    delay: u32,
) -> Result<MessageId> {
    super::send_bytes_with_gas_delayed(program, payload.encode(), gas_limit, value, delay)
}

/// Same as [`send`], but it spends gas from a reservation instead of borrowing
/// it from the gas limit provided with the incoming message.
///
/// The first argument is the reservation identifier [`ReservationId`] obtained
/// by calling the corresponding API. The second argument is the address of the
/// target account ([`ActorId`]). The third argument is the encodable payload.
/// Finally, the last argument is the value to be transferred from the current
/// program account to the message target account.
///
/// # Examples
///
/// Send a message to the sender's address:
///
/// ```
/// use gstd::{exec, msg, prelude::*, ReservationId};
///
/// #[derive(Encode)]
/// #[codec(crate = gstd::codec)]
/// struct Output {
///     a: i32,
///     b: Option<bool>,
/// }
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     let payload = Output {
///         a: 42,
///         b: Some(true),
///     };
///     // Reserve 5 million of gas for 100 blocks
///     let reservation_id = ReservationId::reserve(5_000_000, 100).expect("Unable to reserve");
///     // Receiver id is the message source
///     let actor_id = msg::source();
///
///     msg::send_from_reservation(reservation_id, actor_id, payload, 0).expect("Unable to send");
/// }
/// ```
///
/// # See also
///
/// - [`reply_from_reservation`] function sends a reply to the program or user
///   by using gas from a reservation.
/// - [`MessageHandle::init`](super::MessageHandle::init),
///   [`MessageHandle::push`](super::MessageHandle::init), and
///   [`MessageHandle::commit`](super::MessageHandle::commit) functions allow
///   forming a message to send in parts.
#[wait_for_reply]
pub fn send_from_reservation<E: Encode>(
    id: ReservationId,
    program: ActorId,
    payload: E,
    value: u128,
) -> Result<MessageId> {
    super::send_bytes_from_reservation(id, program, payload.encode(), value)
}

/// Same as [`send_from_reservation`], but sends the message after the `delay`
/// expressed in block count.
pub fn send_delayed_from_reservation<E: Encode>(
    id: ReservationId,
    program: ActorId,
    payload: E,
    value: u128,
    delay: u32,
) -> Result<MessageId> {
    super::send_bytes_delayed_from_reservation(id, program, payload.encode(), value, delay)
}
