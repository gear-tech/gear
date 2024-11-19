// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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

//! ethexe common types and traits.

#![no_std]

extern crate alloc;

pub mod db;
pub mod mirror;
pub mod router;
pub mod wvara;

pub use gear_core;
pub use gprimitives;

use gprimitives::ActorId;
use parity_scale_codec::{Decode, Encode};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Encode, Decode)]
pub enum BlockEvent {
    Router(router::Event),
    Mirror {
        address: ActorId,
        event: mirror::Event,
    },
    WVara(wvara::Event),
}

impl BlockEvent {
    pub fn mirror(address: ActorId, event: mirror::Event) -> Self {
        Self::Mirror { address, event }
    }
}

impl From<router::Event> for BlockEvent {
    fn from(value: router::Event) -> Self {
        Self::Router(value)
    }
}

impl From<wvara::Event> for BlockEvent {
    fn from(value: wvara::Event) -> Self {
        Self::WVara(value)
    }
}

#[derive(Clone, Debug, Encode, Decode, Serialize, Deserialize)]
pub enum BlockRequestEvent {
    Router(router::RequestEvent),
    Mirror {
        address: ActorId,
        event: mirror::RequestEvent,
    },
    WVara(wvara::RequestEvent),
}

impl BlockRequestEvent {
    pub fn mirror(address: ActorId, event: mirror::RequestEvent) -> Self {
        Self::Mirror { address, event }
    }
}

impl From<router::RequestEvent> for BlockRequestEvent {
    fn from(value: router::RequestEvent) -> Self {
        Self::Router(value)
    }
}

impl From<wvara::RequestEvent> for BlockRequestEvent {
    fn from(value: wvara::RequestEvent) -> Self {
        Self::WVara(value)
    }
}

pub const fn u64_into_uint48_be_bytes_lossy(val: u64) -> [u8; 6] {
    let [_, _, b1, b2, b3, b4, b5, b6] = val.to_be_bytes();

    [b1, b2, b3, b4, b5, b6]
}
