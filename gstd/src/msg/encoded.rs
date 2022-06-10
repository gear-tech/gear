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
use core::marker::PhantomData;
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
    D::decode(&mut super::load_bytes().as_ref()).map_err(ContractError::Decode)
}

#[wait_for_reply]
pub fn reply<E: Encode>(payload: E, value: u128) -> Result<MessageId> {
    super::reply_bytes(payload.encode(), value)
}

#[wait_for_reply]
pub fn send<E: Encode>(program: ActorId, payload: E, value: u128) -> Result<MessageId> {
    super::send_bytes(program, payload.encode(), value)
}

#[wait_for_reply]
pub fn send_with_gas<E: Encode>(
    program: ActorId,
    payload: E,
    gas_limit: u64,
    value: u128,
) -> Result<MessageId> {
    super::send_bytes_with_gas(program, payload.encode(), gas_limit, value)
}
