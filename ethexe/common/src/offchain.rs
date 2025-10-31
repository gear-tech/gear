// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use gear_core::message::Value;
use gprimitives::{ActorId, H256};
use parity_scale_codec::{Decode, Encode};

#[derive(Debug, Clone, Encode, Decode, Eq, PartialEq)]
pub struct OffchainInjectedMessage {
    pub reference_block: H256,
    pub destination: ActorId,
    pub payload: Vec<u8>,
    pub value: Value,
}
