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

use alloc::{
    collections::{BTreeMap, VecDeque},
    vec::Vec,
};
use anyhow::{anyhow, Result};
use core::num::NonZero;
use gear_core::{
    code::InstrumentedCode,
    ids::{prelude::MessageIdExt as _, ProgramId},
    memory::PageBuf,
    message::{
        ContextStore, DispatchKind, GasLimit, MessageDetails, Payload, ReplyDetails,
        StoredDispatch, Value, MAX_PAYLOAD_SIZE,
    },
    pages::{numerated::tree::IntervalsTree, GearPage, WasmPage},
    program::MemoryInfix,
    reservation::GasReservationMap,
};
use gear_core_errors::ReplyCode;
use gprimitives::{ActorId, CodeId, MessageId, H256};
use gsys::BlockNumber;
use parity_scale_codec::{Decode, Encode};

pub use gear_core::program::ProgramState as InitStatus;

/// 3h validity in mailbox for 12s blocks.
pub const MAILBOX_VALIDITY: u32 = 54_000;

#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq)]
pub struct HashAndLen {
    pub hash: H256,
    pub len: NonZero<u32>,
}

// TODO: temporary solution to avoid lengths taking in account
impl From<H256> for HashAndLen {
    fn from(value: H256) -> Self {
        Self {
            hash: value,
            len: NonZero::<u32>::new(1).expect("impossible"),
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
    pub fn is_empty(&self) -> bool {
        matches!(self, MaybeHash::Empty)
    }

    pub fn with_hash<T>(&self, f: impl FnOnce(H256) -> T) -> Option<T> {
        let Self::Hash(HashAndLen { hash, .. }) = self else {
            return None;
        };

        Some(f(*hash))
    }

    pub fn with_hash_or_default<T: Default>(&self, f: impl FnOnce(H256) -> T) -> T {
        self.with_hash(f).unwrap_or_default()
    }

    pub fn with_hash_or_default_result<T: Default>(
        &self,
        f: impl FnOnce(H256) -> Result<T>,
    ) -> Result<T> {
        self.with_hash(f).unwrap_or_else(|| Ok(Default::default()))
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

impl Program {
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active(_))
    }

    pub fn is_initialized(&self) -> bool {
        matches!(
            self,
            Self::Active(ActiveProgram {
                initialized: true,
                ..
            })
        )
    }
}

/// ethexe program state.
#[derive(Clone, Debug, Decode, Encode, PartialEq, Eq)]
pub struct ProgramState {
    /// Active, exited or terminated program state.
    pub program: Program,
    /// Hash of incoming message queue, see [`MessageQueue`].
    pub queue_hash: MaybeHash,
    /// Hash of waiting messages list, see [`Waitlist`].
    pub waitlist_hash: MaybeHash,
    /// Hash of mailboxed messages, see [`Mailbox`].
    pub mailbox_hash: MaybeHash,
    /// Reducible balance.
    pub balance: Value,
    /// Executable balance.
    pub executable_balance: Value,
}

impl ProgramState {
    pub const fn zero() -> Self {
        Self {
            program: Program::Active(ActiveProgram {
                allocations_hash: MaybeHash::Empty,
                pages_hash: MaybeHash::Empty,
                memory_infix: MemoryInfix::new(0),
                initialized: false,
            }),
            queue_hash: MaybeHash::Empty,
            waitlist_hash: MaybeHash::Empty,
            mailbox_hash: MaybeHash::Empty,
            balance: 0,
            executable_balance: 0,
        }
    }

    pub fn is_zero(&self) -> bool {
        *self == Self::zero()
    }

    pub fn requires_init_message(&self) -> bool {
        if !matches!(
            self.program,
            Program::Active(ActiveProgram {
                initialized: false,
                ..
            })
        ) {
            return false;
        }

        self.queue_hash.is_empty() && self.waitlist_hash.is_empty()
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

impl Dispatch {
    pub fn reply(
        reply_to: MessageId,
        source: ActorId,
        payload_hash: MaybeHash,
        value: u128,
        reply_code: impl Into<ReplyCode>,
    ) -> Self {
        Self {
            id: MessageId::generate_reply(reply_to),
            kind: DispatchKind::Reply,
            source,
            payload_hash,
            value,
            details: Some(ReplyDetails::new(reply_to, reply_code.into()).into()),
            context: None,
        }
    }

    pub fn from_stored<S: Storage>(storage: &S, value: StoredDispatch) -> Self {
        let (kind, message, context) = value.into_parts();
        let (id, source, destination, payload, value, details) = message.into_parts();

        let payload_hash = storage
            .store_payload(payload.into_vec())
            .expect("infallible due to recasts (only panics on len)");

        Self {
            id,
            kind,
            source,
            payload_hash,
            value,
            details,
            context,
        }
    }
}

pub type ValueWithExpiry<T> = (T, u32);

pub type MessageQueue = VecDeque<Dispatch>;

pub type Waitlist = BTreeMap<MessageId, ValueWithExpiry<Dispatch>>;

// TODO (breathx): consider here LocalMailbox for each user.
pub type Mailbox = BTreeMap<ActorId, BTreeMap<MessageId, ValueWithExpiry<Value>>>;

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

    /// Reads mailbox by mailbox hash.
    fn read_mailbox(&self, hash: H256) -> Option<Mailbox>;

    /// Writes mailbox and returns its hash.
    fn write_mailbox(&self, mailbox: Mailbox) -> H256;

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

pub trait ComplexStorage: Storage {
    fn store_payload(&self, payload: Vec<u8>) -> Result<MaybeHash> {
        let payload =
            Payload::try_from(payload).map_err(|_| anyhow!("failed to save payload: too large"))?;

        Ok(payload
            .inner()
            .is_empty()
            .then_some(MaybeHash::Empty)
            .unwrap_or_else(|| self.write_payload(payload).into()))
    }

    fn store_pages(&self, pages: BTreeMap<GearPage, PageBuf>) -> BTreeMap<GearPage, H256> {
        pages
            .into_iter()
            .map(|(k, v)| (k, self.write_page_data(v)))
            .collect()
    }

    fn modify_memory_pages(
        &self,
        pages_hash: MaybeHash,
        f: impl FnOnce(&mut MemoryPages),
    ) -> Result<MaybeHash> {
        let mut pages = pages_hash.with_hash_or_default_result(|pages_hash| {
            self.read_pages(pages_hash)
                .ok_or_else(|| anyhow!("failed to read pages by their hash ({pages_hash})"))
        })?;

        f(&mut pages);

        let pages_hash = pages
            .is_empty()
            .then_some(MaybeHash::Empty)
            .unwrap_or_else(|| self.write_pages(pages).into());

        Ok(pages_hash)
    }

    fn modify_allocations(
        &self,
        allocations_hash: MaybeHash,
        f: impl FnOnce(&mut Allocations),
    ) -> Result<MaybeHash> {
        let mut allocations = allocations_hash.with_hash_or_default_result(|allocations_hash| {
            self.read_allocations(allocations_hash).ok_or_else(|| {
                anyhow!("failed to read allocations by their hash ({allocations_hash})")
            })
        })?;

        f(&mut allocations);

        let allocations_hash = allocations
            .intervals_amount()
            .eq(&0)
            .then_some(MaybeHash::Empty)
            .unwrap_or_else(|| self.write_allocations(allocations).into());

        Ok(allocations_hash)
    }

    /// Usage: for optimized performance, please remove entries if empty.
    /// Always updates storage.
    fn modify_waitlist(
        &self,
        waitlist_hash: MaybeHash,
        f: impl FnOnce(&mut Waitlist),
    ) -> Result<MaybeHash> {
        self.modify_waitlist_if_changed(waitlist_hash, |waitlist| {
            f(waitlist);
            Some(())
        })
        .map(|v| v.expect("`Some` passed above; infallible").1)
    }

    /// Usage: for optimized performance, please remove entries if empty.
    /// Waitlist is treated changed if f() returns Some.
    fn modify_waitlist_if_changed<T>(
        &self,
        waitlist_hash: MaybeHash,
        f: impl FnOnce(&mut Waitlist) -> Option<T>,
    ) -> Result<Option<(T, MaybeHash)>> {
        let mut waitlist = waitlist_hash.with_hash_or_default_result(|waitlist_hash| {
            self.read_waitlist(waitlist_hash)
                .ok_or_else(|| anyhow!("failed to read waitlist by its hash ({waitlist_hash})"))
        })?;

        let res = if let Some(v) = f(&mut waitlist) {
            let maybe_hash = waitlist
                .is_empty()
                .then_some(MaybeHash::Empty)
                .unwrap_or_else(|| self.write_waitlist(waitlist).into());

            Some((v, maybe_hash))
        } else {
            None
        };

        Ok(res)
    }

    fn modify_queue(
        &self,
        queue_hash: MaybeHash,
        f: impl FnOnce(&mut MessageQueue),
    ) -> Result<MaybeHash> {
        self.modify_queue_returning(queue_hash, f)
            .map(|((), queue_hash)| queue_hash)
    }

    fn modify_queue_returning<T>(
        &self,
        queue_hash: MaybeHash,
        f: impl FnOnce(&mut MessageQueue) -> T,
    ) -> Result<(T, MaybeHash)> {
        let mut queue = queue_hash.with_hash_or_default_result(|queue_hash| {
            self.read_queue(queue_hash)
                .ok_or_else(|| anyhow!("failed to read queue by its hash ({queue_hash})"))
        })?;

        let res = f(&mut queue);

        let queue_hash = queue
            .is_empty()
            .then_some(MaybeHash::Empty)
            .unwrap_or_else(|| self.write_queue(queue).into());

        Ok((res, queue_hash))
    }

    /// Usage: for optimized performance, please remove map entries if empty.
    /// Always updates storage.
    fn modify_mailbox(
        &self,
        mailbox_hash: MaybeHash,
        f: impl FnOnce(&mut Mailbox),
    ) -> Result<MaybeHash> {
        self.modify_mailbox_if_changed(mailbox_hash, |mailbox| {
            f(mailbox);
            Some(())
        })
        .map(|v| v.expect("`Some` passed above; infallible").1)
    }

    /// Usage: for optimized performance, please remove map entries if empty.
    /// Mailbox is treated changed if f() returns Some.
    fn modify_mailbox_if_changed<T>(
        &self,
        mailbox_hash: MaybeHash,
        f: impl FnOnce(&mut Mailbox) -> Option<T>,
    ) -> Result<Option<(T, MaybeHash)>> {
        let mut mailbox = mailbox_hash.with_hash_or_default_result(|mailbox_hash| {
            self.read_mailbox(mailbox_hash)
                .ok_or_else(|| anyhow!("failed to read mailbox by its hash ({mailbox_hash})"))
        })?;

        let res = if let Some(v) = f(&mut mailbox) {
            let maybe_hash = mailbox
                .values()
                .all(|v| v.is_empty())
                .then_some(MaybeHash::Empty)
                .unwrap_or_else(|| self.write_mailbox(mailbox).into());

            Some((v, maybe_hash))
        } else {
            None
        };

        Ok(res)
    }

    fn mutate_state(
        &self,
        state_hash: H256,
        f: impl FnOnce(&Self, &mut ProgramState) -> Result<()>,
    ) -> Result<H256> {
        self.mutate_state_returning(state_hash, f)
            .map(|((), hash)| hash)
    }

    fn mutate_state_returning<T>(
        &self,
        state_hash: H256,
        f: impl FnOnce(&Self, &mut ProgramState) -> Result<T>,
    ) -> Result<(T, H256)> {
        let mut state = self
            .read_state(state_hash)
            .ok_or_else(|| anyhow!("failed to read state by its hash ({state_hash})"))?;

        let res = f(self, &mut state)?;

        Ok((res, self.write_state(state)))
    }
}

impl<T: Storage> ComplexStorage for T {}
