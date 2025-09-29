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

use crate::abi::{IWrappedVara, utils::*};
use gearexe_common::events::WVaraEvent;
use gprimitives::U256;

impl From<IWrappedVara::Approval> for WVaraEvent {
    fn from(value: IWrappedVara::Approval) -> Self {
        Self::Approval {
            owner: address_to_actor_id(value.owner),
            spender: address_to_actor_id(value.spender),
            value: U256(value.value.into_limbs()),
        }
    }
}

impl From<IWrappedVara::Transfer> for WVaraEvent {
    fn from(value: IWrappedVara::Transfer) -> Self {
        Self::Transfer {
            from: address_to_actor_id(value.from),
            to: address_to_actor_id(value.to),
            value: uint256_to_u128_lossy(value.value),
        }
    }
}
