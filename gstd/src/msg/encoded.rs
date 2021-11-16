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

//! Module with encoded via scale-codec messaging functions.

use crate::errors::{ContractError, Result};
use crate::prelude::convert::AsRef;
use crate::{ActorId, MessageId};
use codec::{Decode, Encode};

pub fn load<D: Decode>() -> Result<D> {
    D::decode(&mut super::load_bytes().as_ref()).map_err(ContractError::Decode)
}

pub fn reply<E: Encode>(payload: E, gas_limit: u64, value: u128) -> MessageId {
    super::reply_bytes(payload.encode(), gas_limit, value)
}

pub fn send<E: Encode>(program: ActorId, payload: E, gas_limit: u64, value: u128) -> MessageId {
    super::send_bytes(program, payload.encode(), gas_limit, value)
}
