// This file is part of Gear.

// Copyright (C) 2022-2023 Gear Technologies Inc.
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

//! This program demonstrates the use of [`gstd::msg`] syscalls from an async environment.
//!
//! `Handle` is async and gets a [`Kind`] in the payload, executing a syscall based on the `Kind`,
//! and sending a message back to the source with the payload "PONG".
//!
//! [`Send`] uses the [`send_for_reply()`] syscall to send a message back to the source, containing
//! the `kind` in the payload.
//!
//! [`SendWithGas(gas)`] uses the [`send_with_gas_for_reply()`] syscall to send a message back to
//! the source, containing the `kind` in the payload, and `gas` as the gas limit.
//!
//! [`SendBytes`] uses the [`send_bytes_for_reply()`] syscall to send a message back to the source,
//! containing the `kind` encoded as bytes in the payload.
//!
//! [`SendBytesWithGas(gas)`] uses the [`send_bytes_with_gas_for_reply()`] syscall to send a message
//! back to the source, containing the `kind` encoded as bytes in the payload and `gas` as the gas
//! limit.
//!
//! [`SendCommit`] uses the [`MessageHandle`], pushing the `kind` encoded as bytes and using
//! [`commit_for_reply()`] to send the message back to the source.
//!
//! [`SendCommitWithGas(gas)`] uses the [`MessageHandle`], pushing the `kind` encoded as bytes and
//! using [`commit_with_gas_for_reply()`] to send the message back to the source with `gas` as the
//! gas limit.
//!
//! [`Send`]: Kind::Send
//! [`SendWithGas(gas)`]: Kind::SendWithGas
//! [`SendBytes`]: Kind::SendBytes
//! [`SendBytesWithGas(gas)`]: Kind::SendBytesWithGas
//! [`SendCommit`]: Kind::SendCommit
//! [`SendCommitWithGas(gas)`]: Kind::SendCommitWithGas
//! [`send_for_reply()`]: msg::send_for_reply
//! [`send_with_gas_for_reply()`]: msg::send_with_gas_for_reply
//! [`send_bytes_for_reply()`]: msg::send_bytes_for_reply
//! [`send_bytes_with_gas_for_reply()`]: msg::send_bytes_with_gas_for_reply
//! [`commit_for_reply()`]: MessageHandle::commit_for_reply
//! [`commit_with_gas_for_reply()`]: MessageHandle::commit_with_gas_for_reply

use crate::Kind;
use gstd::{
    msg::{self, MessageHandle},
    prelude::*,
};

#[no_mangle]
extern "C" fn init() {}

#[gstd::async_main]
async fn main() {
    let kind: Kind = msg::load().expect("invalid arguments");
    let encoded_kind = kind.encode();

    match kind {
        Kind::Send => {
            msg::send_for_reply(msg::source(), kind, 0, 0)
                .expect("send message failed")
                .await
        }
        Kind::SendWithGas(gas) => {
            msg::send_with_gas_for_reply(msg::source(), kind, gas, 0, 0)
                .expect("send message failed")
                .await
        }
        Kind::SendBytes => {
            msg::send_bytes_for_reply(msg::source(), &encoded_kind, 0, 0)
                .expect("send message failed")
                .await
        }
        Kind::SendBytesWithGas(gas) => {
            msg::send_bytes_with_gas_for_reply(msg::source(), &encoded_kind, gas, 0, 0)
                .expect("send message failed")
                .await
        }
        Kind::SendCommit => {
            let handle = MessageHandle::init().expect("init message failed");
            handle.push(&encoded_kind).expect("push payload failed");
            handle
                .commit_for_reply(msg::source(), 0, 0)
                .expect("send message failed")
                .await
        }
        Kind::SendCommitWithGas(gas) => {
            let handle = MessageHandle::init().expect("init message failed");
            handle.push(&encoded_kind).expect("push payload failed");
            handle
                .commit_with_gas_for_reply(msg::source(), gas, 0, 0)
                .expect("send message failed")
                .await
        }
    }
    .expect("ran into error-reply");

    msg::send(msg::source(), b"PONG", 0).expect("send message failed");
}
