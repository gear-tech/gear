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
    errors::{ContractError, Result},
    msg::r#async::{CodecMessageFuture, MessageFuture},
    prelude::convert::AsRef,
    ActorId, MessageId,
};
use codec::{Decode, Encode};
use gstd_codegen::wait_for_reply;

/// `load` returns Result, where Ok case contains a message payload decoded into
/// the struct of specified type, or as a generic argument. In case of Err,
/// contains a decoding error ContractError::Decode. For decode-related errors (<https://docs.rs/parity-scale-codec/2.3.1/parity_scale_codec/struct.Error.html>),
/// Gear returns the native one after decode.
///
/// Example:
/// ```ignore
/// use gstd::msg;
/// ...
/// let x: String = msg::load().expect("Unable to decode `String`");
/// ```
pub fn load<D: Decode>() -> Result<D> {
    D::decode(&mut super::load_bytes()?.as_ref()).map_err(ContractError::Decode)
}

/// Send a new message as a reply to the message currently being processed.
#[wait_for_reply]
pub fn reply<E: Encode>(payload: E, value: u128) -> Result<MessageId> {
    super::reply_bytes(payload.encode(), value)
}

/// Same as [`reply`], but sends delayed.
pub fn reply_delayed<E: Encode>(payload: E, value: u128, delay: u32) -> Result<MessageId> {
    super::reply_bytes_delayed(payload.encode(), value, delay)
}

/// Same as [`reply`](crate::msg::reply), but with explicit gas limit.
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
/// Second argument is `gas_limit`. It means the maximum amount of gas that you
/// want to spend on message sending.
/// Third argument `value` is the value to be transferred from the current
/// program account to the reply message target account.
///
/// Reply message transactions will be posted only after processing is finished,
/// similar to the standard message [`send`](crate::msg::send).
///
/// # Examples
///
/// ```
/// use gstd::{exec, msg};
///
/// unsafe extern "C" fn handle() {
///     // ...
///     msg::reply_with_gas(b"PING", 0, 0).unwrap();
/// }
/// ```
///
/// # See also
///
/// [`reply_push`](crate::msg::reply_push) function allows to form a reply
/// message in parts.
#[wait_for_reply]
pub fn reply_with_gas<E: Encode>(payload: E, gas_limit: u64, value: u128) -> Result<MessageId> {
    super::reply_bytes_with_gas(payload.encode(), gas_limit, value)
}

/// Same as [`reply_with_gas`], but sends delayed.
pub fn reply_with_gas_delayed<E: Encode>(
    payload: E,
    gas_limit: u64,
    value: u128,
    delay: u32,
) -> Result<MessageId> {
    super::reply_bytes_with_gas_delayed(payload.encode(), gas_limit, value, delay)
}

/// Send a new message to the program or user.
#[wait_for_reply]
pub fn send<E: Encode>(program: ActorId, payload: E, value: u128) -> Result<MessageId> {
    super::send_bytes(program, payload.encode(), value)
}

/// Same as [`send`], but sends delayed.
pub fn send_delayed<E: Encode>(
    program: ActorId,
    payload: E,
    value: u128,
    delay: u32,
) -> Result<MessageId> {
    super::send_bytes_delayed(program, payload.encode(), value, delay)
}

/// Same as [`send`], but with explicit gas limit.
#[wait_for_reply]
pub fn send_with_gas<E: Encode>(
    program: ActorId,
    payload: E,
    gas_limit: u64,
    value: u128,
) -> Result<MessageId> {
    super::send_bytes_with_gas(program, payload.encode(), gas_limit, value)
}

/// Same as [`send_with_gas`], but sends delayed.
pub fn send_with_gas_delayed<E: Encode>(
    program: ActorId,
    payload: E,
    gas_limit: u64,
    value: u128,
    delay: u32,
) -> Result<MessageId> {
    super::send_bytes_with_gas_delayed(program, payload.encode(), gas_limit, value, delay)
}
