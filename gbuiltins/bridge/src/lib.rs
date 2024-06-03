// This file is part of Gear.

// Copyright (C) 2024 Gear Technologies Inc.
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

#![no_std]

use codec::{Decode, Encode};
use gear_core::message::Payload;
use gprimitives::{H160, H256, U256};

#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug)]
#[codec(crate = codec)]
pub enum Request {
    #[codec(index = 0)]
    SendMessage { dest: H160, payload: Payload },
}

#[derive(Encode, Decode, Clone, Debug, PartialEq, Eq)]
#[codec(crate = codec)]
pub enum Response {
    MessageSent { nonce: U256, hash: H256 },
}
