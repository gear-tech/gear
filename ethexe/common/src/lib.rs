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

//! ethexe common types and traits.

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "std")]
extern crate std;

extern crate alloc;

pub mod crypto;
pub mod db;
pub mod events;
pub mod gear;
pub mod tx_pool;
pub mod utils;

pub use crypto::*;
pub use gear_core;
pub use gprimitives;
pub use k256;
pub use sha3;
pub use utils::*;

use alloc::vec::Vec;
use db::BlockHeader;
use events::BlockEvent;
use gprimitives::H256;
use parity_scale_codec::{Decode, Encode};

// TODO #4547: move to submodule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockData {
    pub hash: H256,
    pub header: BlockHeader,
    pub events: Vec<BlockEvent>,
}

impl BlockData {
    pub fn to_simple(&self) -> SimpleBlockData {
        SimpleBlockData {
            hash: self.hash,
            header: self.header.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimpleBlockData {
    pub hash: H256,
    pub header: BlockHeader,
}

#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq)]
pub struct ProducerBlock {
    pub block_hash: H256,
    pub gas_allowance: Option<u64>,
    pub off_chain_transactions: Vec<H256>,
}
