// This file is part of Gear.
//
// Copyright (C) 2021-2025 Gear Technologies Inc.
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

#![cfg_attr(not(feature = "std"), no_std)]

use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

#[cfg(not(feature = "std"))]
pub const WASM_BINARY: &[u8] = &[];

#[cfg(not(feature = "std"))]
pub mod wasm;

use core::ops::Range;
use gstd::{ActorId, prelude::*};

#[derive(Debug, Decode, Encode, TypeInfo)]
pub struct InitConfig {
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub initial_capacity: Option<u32>,
}

#[derive(Debug, Decode, Encode, TypeInfo)]
pub enum FTAction {
    TestSet(Range<u64>, u128),
    Mint(u128),
    Burn(u128),
    Transfer {
        from: ActorId,
        to: ActorId,
        amount: u128,
    },
    Approve {
        to: ActorId,
        amount: u128,
    },
    TotalSupply,
    BalanceOf(ActorId),
}

#[derive(Debug, Encode, Decode, TypeInfo, MaxEncodedLen, Eq, PartialEq)]
pub enum FTEvent {
    Transfer {
        from: ActorId,
        to: ActorId,
        amount: u128,
    },
    Approve {
        from: ActorId,
        to: ActorId,
        amount: u128,
    },
    TotalSupply(u128),
    Balance(u128),
}

#[derive(Debug, Clone, Default, Encode, Decode, TypeInfo)]
pub struct IoFungibleToken {
    pub name: String,
    pub symbol: String,
    pub total_supply: u128,
    pub balances: Vec<(ActorId, u128)>,
    pub allowances: Vec<(ActorId, Vec<(ActorId, u128)>)>,
    pub decimals: u8,
}

impl InitConfig {
    pub fn test_sequence() -> Self {
        InitConfig {
            name: "MyToken".to_string(),
            symbol: "MTK".to_string(),
            decimals: 18,
            initial_capacity: None,
        }
    }
}

impl IoFungibleToken {
    pub fn test_sequence() -> Self {
        IoFungibleToken {
            name: "MyToken".to_string(),
            symbol: "MTK".to_string(),
            total_supply: 0,
            balances: vec![],
            allowances: vec![],
            decimals: 18,
        }
    }
}
