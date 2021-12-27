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

//! Module with basic messaging functions.

use crate::prelude::{convert::AsRef, vec, Vec};
use crate::{ActorId, CodeHash, MessageId};
use codec::Output;

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

pub fn exit_code() -> i32 {
    gcore::msg::exit_code()
}

pub fn id() -> MessageId {
    gcore::msg::id().into()
}

pub fn load_bytes() -> Vec<u8> {
    let mut result = vec![0u8; size()];
    gcore::msg::load(&mut result[..]);
    result
}

pub fn reply_bytes<T: AsRef<[u8]>>(payload: T, gas_limit: u64, value: u128) -> MessageId {
    gcore::msg::reply(payload.as_ref(), gas_limit, value).into()
}

pub fn reply_commit(gas_limit: u64, value: u128) -> MessageId {
    gcore::msg::reply_commit(gas_limit, value).into()
}

pub fn reply_push<T: AsRef<[u8]>>(payload: T) {
    gcore::msg::reply_push(payload.as_ref());
}

pub fn reply_to() -> MessageId {
    gcore::msg::reply_to().into()
}

pub fn send_bytes<T: AsRef<[u8]>>(
    program: ActorId,
    payload: T,
    gas_limit: u64,
    value: u128,
) -> MessageId {
    gcore::msg::send(program.into(), payload.as_ref(), gas_limit, value).into()
}

pub fn send_commit(
    handle: MessageHandle,
    program: ActorId,
    gas_limit: u64,
    value: u128,
) -> MessageId {
    gcore::msg::send_commit(handle.into(), program.into(), gas_limit, value).into()
}

pub fn send_init() -> MessageHandle {
    gcore::msg::send_init().into()
}

pub fn send_push<T: AsRef<[u8]>>(handle: &MessageHandle, payload: T) {
    gcore::msg::send_push(handle.as_ref(), payload.as_ref())
}

pub fn size() -> usize {
    gcore::msg::size()
}

pub fn source() -> ActorId {
    gcore::msg::source().into()
}

pub fn value() -> u128 {
    gcore::msg::value()
}

pub fn create_program<T1: AsRef<[u8]>, T2: AsRef<[u8]>>(
    code_hash: CodeHash,
    salt: T1,
    payload: T2,
    gas_limit: u64,
    value: u128,
) -> ActorId {
    gcore::msg::create_program(
        code_hash.into(),
        salt.as_ref(),
        payload.as_ref(),
        gas_limit,
        value,
    )
    .into()
}
