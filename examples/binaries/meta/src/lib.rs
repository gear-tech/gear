// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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
#![allow(deprecated)]

use codec::{Decode, Encode};
use gstd::{String, ToString, Vec};
use scale_info::TypeInfo;

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

// TODO: delete once moved gcli on new reading state approach.
#[cfg(feature = "std")]
pub use code::WASM_BINARY as WASM_BINARY_META;
#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

#[cfg(not(feature = "std"))]
mod wasm {
    include! {"./code.rs"}
}

// Metatypes for input and output
#[derive(TypeInfo, Decode, Encode)]
pub struct MessageInitIn {
    pub amount: u8,
    pub currency: String,
}

#[derive(TypeInfo, Encode)]
pub struct MessageInitOut {
    pub exchange_rate: Result<u8, u8>,
    pub sum: u8,
}

impl From<MessageInitIn> for MessageInitOut {
    fn from(other: MessageInitIn) -> Self {
        let rate = match other.currency.as_ref() {
            "USD" => Ok(2),
            "EUR" => Ok(3),
            _ => Err(1),
        };

        Self {
            exchange_rate: rate,
            sum: rate.unwrap_or(0) * other.amount,
        }
    }
}

#[derive(TypeInfo, Encode, Decode)]
pub struct MessageIn {
    pub id: Id,
}

#[derive(TypeInfo, Encode)]
pub struct MessageOut {
    pub res: Option<Wallet>,
}

impl From<MessageIn> for MessageOut {
    fn from(other: MessageIn) -> Self {
        unsafe {
            let res = WALLETS
                .iter()
                .find(|w| w.id.decimal == other.id.decimal)
                .map(Clone::clone);

            Self { res }
        }
    }
}

// Additional to primary types
#[derive(TypeInfo, Decode, Encode, Debug, PartialEq, Eq, Clone)]
pub struct Id {
    pub decimal: u64,
    pub hex: Vec<u8>,
}

#[derive(TypeInfo, Encode, Decode, Clone, Debug)]
pub struct Person {
    pub surname: String,
    pub name: String,
}

#[derive(TypeInfo, Encode, Decode, Clone, Debug)]
pub struct Wallet {
    pub id: Id,
    pub person: Person,
}

#[derive(TypeInfo, Decode, Clone)]
pub struct MessageInitAsyncIn {
    pub empty: (),
}

#[derive(TypeInfo, Encode, Clone)]
pub struct MessageInitAsyncOut {
    pub empty: (),
}

#[derive(TypeInfo, Decode, Clone)]
pub struct MessageHandleAsyncIn {
    pub empty: (),
}

#[derive(TypeInfo, Encode, Clone)]
pub struct MessageHandleAsyncOut {
    pub empty: (),
}

// NOTE: this macro has been deprecated, see
// https://github.com/gear-tech/gear/tree/master/examples/binaries/new-meta
gstd::metadata! {
    title: "Example program with metadata",
    init:
        input: MessageInitIn,
        output: MessageInitOut,
        awaiting:
            input: MessageInitAsyncIn,
            output: MessageInitAsyncOut,
    handle:
        input: MessageIn,
        output: MessageOut,
        awaiting:
            input: MessageHandleAsyncIn,
            output: MessageHandleAsyncOut,
    state:
        input: Option<Id>,
        output: Vec<Wallet>,
}

static mut WALLETS: Vec<Wallet> = Vec::new();
