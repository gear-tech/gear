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

#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode)]
pub enum Event {
    Transfer {
        from: ActorId,
        to: ActorId,
        value: u128,
    },
    Approval {
        owner: ActorId,
        spender: ActorId,
        value: U256,
    },
}

impl Event {
    pub fn to_request(self) -> Option<RequestEvent> {
        Some(match self {
            Self::Transfer { from, to, value } => RequestEvent::Transfer { from, to, value },
            Self::Approval { .. } => return None,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub enum RequestEvent {
    Transfer {
        /// Never router, wvara or zero address.
        from: ActorId,
        /// Never router, wvara or zero address.
        to: ActorId,
        value: u128,
    },
}

impl RequestEvent {
    pub fn involves_address(&self, address: &ActorId) -> bool {
        match self {
            Self::Transfer { from, to, .. } => from == address || to == address,
        }
    }

    pub fn involves_addresses(&self, addresses: &[ActorId]) -> bool {
        match self {
            Self::Transfer { from, to, .. } => addresses.contains(from) || addresses.contains(to),
        }
    }
}
