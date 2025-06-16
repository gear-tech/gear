// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::{string::String, vec::Vec};
use parity_scale_codec::{Decode, Encode};

#[cfg(feature = "wasm-wrapper")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "wasm-wrapper")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

type MessageId = [u8; 32];
type ActorId = [u8; 32];
type Value = u128;
type Gas = u64;

// Instead of proper gstd primitives we use their raw versions to make this program
// compilable as a dependency for the build of the `gear` with `runtime-benchmarking` feature.
#[derive(Debug, Encode, Decode)]
pub enum Kind {
    // Params(salt, gas), Expected(message id, actor id)
    CreateProgram(u64, Option<u64>, (MessageId, ActorId)),
    // Params(value), Expected(error message)
    Error(u128, String),
    // Params(gas), Expected(message id)
    Send(Option<u64>, MessageId),
    // Params(payload, gas), Expected(message id)
    SendRaw(Vec<u8>, Option<u64>, MessageId),
    // Params(gas), Expected(message id)
    SendInput(Option<u64>, MessageId),
    // Expected(message id)
    SendPushInput(MessageId),
    // Expected(payload size)
    Size(u32),
    // Expected(message id)
    MessageId(MessageId),
    // Expected(program id)
    ActorId(ActorId),
    // Expected(message sender)
    Source(ActorId),
    // Expected(message value)
    Value(u128),
    // Expected(this program's balance)
    ValueAvailable(u128),
    // Params(gas), Expected(message id)
    Reply(Option<u64>, MessageId),
    // Params(payload, gas), Expected(message id)
    ReplyRaw(Vec<u8>, Option<u64>, MessageId),
    // Params(gas), Expected(message id)
    ReplyInput(Option<u64>, MessageId),
    // Expected(message id)
    ReplyPushInput(MessageId),
    // Expected(reply to id, ReplyCode.to_bytes repr)
    ReplyDetails(MessageId, [u8; 4]),
    SignalDetails,
    SignalDetailsWake,
    // Expected values
    EnvVars {
        performance_multiplier: u32,
        existential_deposit: Value,
        mailbox_threshold: Gas,
        gas_to_value_multiplier: Value,
    },
    // Expected(block height)
    BlockHeight(u32),
    // Expected(block timestamp)
    BlockTimestamp(u64),
    // Expected(id)
    Reserve(Vec<u8>),
    // Expected(amount)
    Unreserve(u64),
    // Param(salt), Expected(hash, block number)
    Random([u8; 32], ([u8; 32], u32)),
    // Expected(lower bound, upper bound )-> estimated gas level
    GasAvailable(u64, u64),
    // Expected(message id)
    ReservationSend(MessageId),
    // Param(payload), Expected(message id)
    ReservationSendRaw(Vec<u8>, MessageId),
    // Expected(message id)
    ReservationReply(MessageId),
    // Param(payload), Expected(message id)
    ReservationReplyCommit(Vec<u8>, MessageId),
    // Param(reserve amount)
    SystemReserveGas(u64),
    // Param(deposit amount)
    ReplyDeposit(u64),
}

#[cfg(not(feature = "wasm-wrapper"))]
mod wasm;
