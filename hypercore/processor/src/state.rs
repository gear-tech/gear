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

//! State-related data structures.

use gear_core::{
    ids::ProgramId,
    message::{ContextStore, DispatchKind, GasLimit, MessageDetails, Value},
    pages::GearPage,
    program::MemoryInfix,
};
use gprimitives::{MessageId, H256};
use hypercore_db::CASDatabase;
use parity_scale_codec::{Decode, Encode};
use std::{collections::BTreeMap, num::NonZeroU32};

#[derive(Clone, Debug, Encode, Decode)]
pub struct HashAndLen {
    pub hash: H256,
    pub len: NonZeroU32,
}

// TODO: temporary solution to avoid lengths taking in account
impl From<H256> for HashAndLen {
    fn from(value: H256) -> Self {
        Self {
            hash: value,
            len: NonZeroU32::new(1).expect("impossible"),
        }
    }
}

impl HashAndLen {
    pub fn read<T: Decode>(&self, db: Box<dyn CASDatabase>) -> T {
        let data = db
            .read(&self.hash)
            .expect("`db` does not contain data by hash {hash}");
        T::decode(&mut &data[..]).expect("Failed to decode data into `T`")
    }
}

#[derive(Clone, Debug, Encode, Decode)]
pub enum MaybeHash {
    Hash(HashAndLen),
    Empty,
}

// TODO: temporary solution to avoid lengths taking in account
impl From<H256> for MaybeHash {
    fn from(value: H256) -> Self {
        MaybeHash::Hash(HashAndLen::from(value))
    }
}

impl MaybeHash {
    pub fn read<T: Decode>(&self, db: Box<dyn CASDatabase>) -> Option<T> {
        match self {
            MaybeHash::Hash(hash_and_len) => Some(hash_and_len.read(db)),
            MaybeHash::Empty => None,
        }
    }
}

/// Hypercore program state.
#[derive(Clone, Debug, Decode, Encode)]
pub struct ProgramState {
    /// Hash of incoming message queue, see [`MessageQueue`].
    pub queue_hash: MaybeHash,
    /// Wasm memory pages allocations.
    pub allocations_hash: MaybeHash,
    /// Hash of memory pages table, see [`MemoryPages`].
    pub pages_hash: MaybeHash,
    /// Hash of the original code bytes.
    pub original_code_hash: HashAndLen,
    /// Hash of the instrumented code, see [`InstrumentedCode`].
    pub instrumented_code_hash: HashAndLen,
    /// Gas reservations map.
    pub gas_reservation_map_hash: MaybeHash,
    /// Program memory infix.
    pub memory_infix: MemoryInfix,
    /// Balance
    pub balance: Value,
}

#[derive(Clone, Debug, Encode, Decode)]
pub struct Dispatch {
    /// Message id.
    pub id: MessageId,
    /// Dispatch kind.
    pub kind: DispatchKind,
    /// Message source.
    pub source: ProgramId,
    /// Message payload.
    pub payload_hash: MaybeHash,
    /// Message gas limit. Required here.
    pub gas_limit: GasLimit,
    /// Message value.
    pub value: Value,
    /// Message details like reply message ID, status code, etc.
    pub details: Option<MessageDetails>,
    /// Message previous executions context.
    pub context: Option<ContextStore>,
}

#[derive(Clone, Debug, Encode, Decode, Default)]
pub struct MessageQueue(pub Vec<Dispatch>);

/// Memory pages table, mapping gear page number to page data bytes hash.
#[derive(Clone, Debug, Encode, Decode, Default)]
pub struct MemoryPages(pub BTreeMap<GearPage, H256>);
