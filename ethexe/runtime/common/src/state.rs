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

use core::num::NonZeroU32;

use alloc::{
    collections::{BTreeMap, VecDeque},
    vec::Vec,
};
use gear_core::{
    code::InstrumentedCode,
    ids::ProgramId,
    memory::PageBuf,
    message::{ContextStore, DispatchKind, GasLimit, MessageDetails, Payload, Value},
    pages::{numerated::tree::IntervalsTree, GearPage, WasmPage},
    program::MemoryInfix,
    reservation::GasReservationMap,
};
use gprimitives::{CodeId, MessageId, H256};
use gsys::BlockNumber;
use parity_scale_codec::{Decode, Encode};

pub use gear_core::program::ProgramState as InitStatus;

#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq)]
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

#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq)]
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
    pub fn with_hash_or_default<T: Default>(&self, f: impl FnOnce(H256) -> T) -> T {
        match &self {
            Self::Hash(HashAndLen { hash, .. }) => f(*hash),
            Self::Empty => Default::default(),
        }
    }
}

#[derive(Clone, Debug, Decode, Encode, PartialEq, Eq)]
pub struct ActiveProgram {
    /// Hash of wasm memory pages allocations, see [`Allocations`].
    pub allocations_hash: MaybeHash,
    /// Hash of memory pages table, see [`MemoryPages`].
    pub pages_hash: MaybeHash,
    /// Program memory infix.
    pub memory_infix: MemoryInfix,
    /// Program initialization status.
    pub initialized: bool,
}

#[derive(Clone, Debug, Decode, Encode, PartialEq, Eq)]
pub enum Program {
    Active(ActiveProgram),
    Exited(ProgramId),
    Terminated(ProgramId),
}

/// ethexe program state.
#[derive(Clone, Debug, Decode, Encode, PartialEq, Eq)]
pub struct ProgramState {
    /// Active, exited or terminated program state.
    pub state: Program,
    /// Hash of incoming message queue, see [`MessageQueue`].
    pub queue_hash: MaybeHash,
    /// Hash of waiting messages list, see [`Waitlist`].
    pub waitlist_hash: MaybeHash,
    /// Reducible balance.
    pub balance: Value,
    /// Executable balance.
    pub executable_balance: Value,
}

impl ProgramState {
    pub const fn zero() -> Self {
        Self {
            state: Program::Active(ActiveProgram {
                allocations_hash: MaybeHash::Empty,
                pages_hash: MaybeHash::Empty,
                memory_infix: MemoryInfix::new(0),
                initialized: false,
            }),
            queue_hash: MaybeHash::Empty,
            waitlist_hash: MaybeHash::Empty,
            balance: 0,
            executable_balance: 0,
        }
    }

    pub fn is_zero(&self) -> bool {
        *self == Self::zero()
    }
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
    /// Message value.
    pub value: Value,
    /// Message details like reply message ID, status code, etc.
    pub details: Option<MessageDetails>,
    /// Message previous executions context.
    pub context: Option<ContextStore>,
}

pub type MessageQueue = VecDeque<Dispatch>;

pub type Waitlist = BTreeMap<BlockNumber, Vec<Dispatch>>;

pub type MemoryPages = BTreeMap<GearPage, H256>;

pub type Allocations = IntervalsTree<WasmPage>;

pub trait Storage {
    /// Reads program state by state hash.
    fn read_state(&self, hash: H256) -> Option<ProgramState>;

    /// Writes program state and returns its hash.
    fn write_state(&self, state: ProgramState) -> H256;

    /// Reads message queue by queue hash.
    fn read_queue(&self, hash: H256) -> Option<MessageQueue>;

    /// Writes message queue and returns its hash.
    fn write_queue(&self, queue: MessageQueue) -> H256;

    /// Reads waitlist by waitlist hash.
    fn read_waitlist(&self, hash: H256) -> Option<Waitlist>;

    /// Writes waitlist and returns its hash.
    fn write_waitlist(&self, waitlist: Waitlist) -> H256;

    /// Reads memory pages by pages hash.
    fn read_pages(&self, hash: H256) -> Option<MemoryPages>;

    /// Writes memory pages and returns its hash.
    fn write_pages(&self, pages: MemoryPages) -> H256;

    /// Reads allocations by allocations hash.
    fn read_allocations(&self, hash: H256) -> Option<Allocations>;

    /// Writes allocations and returns its hash.
    fn write_allocations(&self, allocations: Allocations) -> H256;

    /// Reads payload by payload hash.
    fn read_payload(&self, hash: H256) -> Option<Payload>;

    /// Writes payload and returns its hash.
    fn write_payload(&self, payload: Payload) -> H256;

    /// Reads page data by page data hash.
    fn read_page_data(&self, hash: H256) -> Option<PageBuf>;

    /// Writes page data and returns its hash.
    fn write_page_data(&self, data: PageBuf) -> H256;
}
