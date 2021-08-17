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

use crate::prelude::Vec;
use crate::{MessageId, ProgramId};
use codec::{Decode, Encode, Output};

pub use gcore::msg::{gas_available, id, reply_to, source, value, wait, wake};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageHandle(gcore::MessageHandle);

impl MessageHandle {
    pub fn init() -> Self {
        send_init()
    }

    pub fn push<E: Encode>(&self, payload: E) {
        gcore::msg::send_push(&self.0, &payload.encode())
    }

    pub fn commit(self, program: ProgramId, gas_limit: u64, value: u128) -> MessageId {
        gcore::msg::send_commit(self.0, program, gas_limit, value)
    }
}

impl Output for MessageHandle {
    fn write(&mut self, bytes: &[u8]) {
        gcore::msg::send_push(&self.0, bytes)
    }
}

pub fn load<D: Decode>() -> Result<D, codec::Error> {
    D::decode(&mut gcore::msg::load().as_ref())
}

pub fn load_bytes() -> Vec<u8> {
    gcore::msg::load()
}

pub fn reply<E: Encode>(payload: E, gas_limit: u64, value: u128) {
    let bytes = payload.encode();
    gcore::msg::reply(&bytes, gas_limit, value)
}

pub fn reply_bytes(payload: &[u8], gas_limit: u64, value: u128) {
    gcore::msg::reply(payload, gas_limit, value)
}

pub fn reply_push<E: Encode>(payload: E) {
    let bytes = payload.encode();
    gcore::msg::reply_push(&bytes)
}

pub fn reply_push_bytes(payload: &[u8]) {
    gcore::msg::reply_push(payload)
}

pub fn send<E: Encode>(program: ProgramId, payload: E, gas_limit: u64) -> MessageId {
    send_with_value(program, payload, gas_limit, 0u128)
}

pub fn send_bytes(program: ProgramId, payload: &[u8], gas_limit: u64) -> MessageId {
    gcore::msg::send(program, payload, gas_limit)
}

pub fn send_init() -> MessageHandle {
    MessageHandle(gcore::msg::send_init())
}

pub fn send_with_value<E: Encode>(
    program: ProgramId,
    payload: E,
    gas_limit: u64,
    value: u128,
) -> MessageId {
    let bytes = payload.encode();
    gcore::msg::send_with_value(program, &bytes, gas_limit, value)
}

pub fn send_bytes_with_value(
    program: ProgramId,
    payload: &[u8],
    gas_limit: u64,
    value: u128,
) -> MessageId {
    gcore::msg::send_with_value(program, payload, gas_limit, value)
}
