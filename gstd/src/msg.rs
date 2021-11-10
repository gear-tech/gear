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

use crate::prelude::{convert::AsRef, Vec};
use crate::{MessageId, ProgramId};
use codec::{Decode, Encode, Output};

use galloc::prelude::*;
pub use gcore::msg::{exit_code, id, reply_to, source, value};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageHandle(gcore::MessageHandle);

impl MessageHandle {
    pub fn init() -> Self {
        send_init()
    }

    pub fn push<T: AsRef<[u8]>>(&self, payload: T) {
        gcore::msg::send_push(&self.0, payload.as_ref());
    }

    pub fn commit(self, program: ProgramId, gas_limit: u64, value: u128) -> MessageId {
        gcore::msg::send_commit(self.0, program, gas_limit, value)
    }
}

impl Output for MessageHandle {
    fn write(&mut self, bytes: &[u8]) {
        gcore::msg::send_push(&self.0, bytes);
    }
}

pub fn load<D: Decode>() -> Result<D, codec::Error> {
    D::decode(&mut load_bytes().as_ref())
}

pub fn load_bytes() -> Vec<u8> {
    let mut result = vec![0u8; gcore::msg::size()];
    gcore::msg::load(&mut result[..]);
    result
}

pub fn reply<E: Encode>(payload: E, gas_limit: u64, value: u128) -> MessageId {
    reply_bytes(&payload.encode(), gas_limit, value)
}

pub fn reply_bytes<T: AsRef<[u8]>>(payload: T, gas_limit: u64, value: u128) -> MessageId {
    gcore::msg::reply(payload.as_ref(), gas_limit, value)
}

pub fn reply_commit(gas_limit: u64, value: u128) -> MessageId {
    gcore::msg::reply_commit(gas_limit, value)
}

pub fn reply_push<T: AsRef<[u8]>>(payload: T) {
    gcore::msg::reply_push(payload.as_ref());
}

pub fn send_init() -> MessageHandle {
    MessageHandle(gcore::msg::send_init())
}

pub fn send<E: Encode>(program: ProgramId, payload: E, gas_limit: u64, value: u128) -> MessageId {
    send_bytes(program, &payload.encode(), gas_limit, value)
}

pub fn send_bytes<T: AsRef<[u8]>>(
    program: ProgramId,
    payload: T,
    gas_limit: u64,
    value: u128,
) -> MessageId {
    gcore::msg::send(program, payload.as_ref(), gas_limit, value)
}
