// This file is part of Gear.

// Copyright (C) 2024 Gear Technologies Inc.
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

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::{vec, vec::Vec};
use codec::{Decode, Encode};
use ssz_rs::{prelude::SimpleSerialize, Deserialize, DeserializeError, Sized};

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

pub mod primitives;
use primitives::*;

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

#[cfg(not(feature = "std"))]
mod wasm;

pub type Address = ByteVector<20>;
pub type Bytes32 = ByteVector<32>;
pub type LogsBloom = ByteVector<256>;
pub type BLSPubKey = ByteVector<48>;
pub type SignatureBytes = ByteVector<96>;
pub type Transaction = ByteList<1_073_741_824>;

#[derive(Debug, Clone, Default, SimpleSerialize)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
pub struct Header {
    pub slot: U64,
    pub proposer_index: U64,
    pub parent_root: Bytes32,
    pub state_root: Bytes32,
    pub body_root: Bytes32,
}

#[derive(Debug, Clone, Default, SimpleSerialize)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
pub struct SyncCommittee {
    pub pubkeys: ssz_rs::Vector<BLSPubKey, 512>,
    pub aggregate_pubkey: BLSPubKey,
}

#[derive(Debug, Clone, Default, Decode, Encode)]
#[codec(crate = codec)]
pub struct Init {
    pub last_checkpoint: [u8; 32],
    // all next fields are ssz_rs serialized
    pub finalized_header: Vec<u8>,
    pub optimistic_header: Vec<u8>,
    pub current_sync_committee: Vec<u8>,
    pub current_sync_committee_branch: Vec<[u8; 32]>,
}

#[derive(Debug, Clone, Default, SimpleSerialize)]
pub struct Update {
    pub attested_header: Header,
    pub sync_committee_bits: ssz_rs::Bitvector<512>,
    pub next_sync_committee: Option<SyncCommittee>,
    pub finalized_header: Option<Header>,
}

#[derive(Debug, Clone, Decode, Encode)]
#[codec(crate = codec)]
pub enum Handle {
    Update {
        // ssz_rs serialized Update struct
        update: Vec<u8>,
        signature_slot: u64,
        // serialized without compression
        sync_committee_signature: Vec<u8>,
        next_sync_committee_branch: Option<Vec<[u8; 32]>>,
        finality_branch: Option<Vec<[u8; 32]>>,
    },
}

pub fn calc_sync_period(slot: u64) -> u64 {
    // 32 slots per epoch
    let epoch = slot / 32;

    // 256 epochs per sync committee
    epoch / 256
}
