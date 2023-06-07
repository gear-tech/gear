// This file is part of Gear.
//
// Copyright (C) 2021-2023 Gear Technologies Inc.
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

#![no_std]

use ft_io::*;
use gmeta::{metawasm, Metadata};
use gstd::{prelude::*, ActorId};

#[metawasm]
pub trait Metawasm {
    type State = <FungibleTokenMetadata as Metadata>::State;

    fn name(state: Self::State) -> String {
        state.name.clone()
    }
    fn symbol(state: Self::State) -> String {
        state.symbol.clone()
    }
    fn decimals(state: Self::State) -> u8 {
        state.decimals
    }
    fn total_supply(state: Self::State) -> u128 {
        state.total_supply
    }

    fn balances_of(account: ActorId, state: Self::State) -> u128 {
        match state.balances.iter().find(|(id, _balance)| account.eq(id)) {
            Some((_id, balance)) => *balance,
            None => panic!("Balance for account ID {account:?} not found",),
        }
    }
}
