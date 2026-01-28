// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
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

use gprimitives::{ActorId, U256};
use parity_scale_codec::{Decode, Encode};

#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, Hash)]
pub struct TransferEvent {
    pub from: ActorId,
    pub to: ActorId,
    pub value: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, Hash)]
pub struct ApprovalEvent {
    pub owner: ActorId,
    pub spender: ActorId,
    pub value: U256,
}

#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, Hash)]
pub enum Event {
    Transfer(TransferEvent),
    Approval(ApprovalEvent),
}
