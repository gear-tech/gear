// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
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

#[cfg(not(feature = "std"))]
use alloc::string::{String, ToString};
use alloc::{
    collections::{BTreeMap, VecDeque},
    vec::Vec,
};
use anyhow::{anyhow, Result};
use core::{
    any::Any,
    marker::PhantomData,
    mem,
    ops::{Index, IndexMut},
};
use ethexe_common::gear::Message;
pub use gear_core::program::ProgramState as InitStatus;
use gear_core::{
    ids::{prelude::MessageIdExt as _, ProgramId},
    memory::PageBuf,
    message::{
        ContextStore, DispatchKind, MessageDetails, Payload, ReplyDetails, StoredDispatch, Value,
    },
    pages::{numerated::tree::IntervalsTree, GearPage, WasmPage},
    program::MemoryInfix,
};
use gear_core_errors::{ReplyCode, SuccessReplyReason};
use gprimitives::{ActorId, MessageId, H256};
use parity_scale_codec::{Decode, Encode};
use private::Sealed;

/// 3h validity in mailbox for 12s blocks.
pub const MAILBOX_VALIDITY: u32 = 54_000;

mod private {
    use super::*;

    pub trait Sealed {}

    impl Sealed for Allocations {}
    impl Sealed for DispatchStash {}
    impl Sealed for Mailbox {}
    impl Sealed for MemoryPages {}
    impl Sealed for MemoryPagesRegion {}
    impl Sealed for MessageQueue {}
    impl Sealed for Payload {}
    impl Sealed for PageBuf {}
    // TODO (breathx): consider using HashOf<ProgramState> everywhere.
    // impl Sealed for ProgramState {}
    impl Sealed for Waitlist {}

    pub fn shortname<S: Any>() -> &'static str {
        core::any::type_name::<S>()
            .split("::")
            .last()
            .expect("name is empty")
    }
}

/// Represents payload provider (lookup).
///
/// Directly keeps payload inside of itself, or keeps hash of payload stored in database.
///
/// Motivation for usage: it's more optimized to held small payloads in place.
/// Zero payload should always be stored directly.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub enum PayloadLookup {
    Direct(Payload),
    Stored(HashOf<Payload>),
}

impl Default for PayloadLookup {
    fn default() -> Self {
        Self::empty()
    }
}

impl From<HashOf<Payload>> for PayloadLookup {
    fn from(value: HashOf<Payload>) -> Self {
        Self::Stored(value)
    }
}

impl PayloadLookup {
    /// Lower len to be stored in storage instead of holding value itself; 1 KB.
    pub const STORING_THRESHOLD: usize = 1024;

    pub const fn empty() -> Self {
        Self::Direct(Payload::new())
    }

    pub fn is_empty(&self) -> bool {
        if let Self::Direct(payload) = self {
            payload.inner().is_empty()
        } else {
            false
        }
    }

    pub fn force_stored<S: Storage>(&mut self, storage: &S) -> HashOf<Payload> {
        let hash = match self {
            Self::Direct(payload) => {
                let payload = mem::replace(payload, Payload::new());
                storage.write_payload(payload)
            }
            Self::Stored(hash) => *hash,
        };

        *self = hash.into();

        hash
    }

    pub fn query<S: Storage>(self, storage: &S) -> Result<Payload> {
        match self {
            Self::Direct(payload) => Ok(payload),
            Self::Stored(hash) => storage
                .read_payload(hash)
                .ok_or_else(|| anyhow!("failed to read ['Payload'] from storage by hash")),
        }
    }
}

#[derive(
    Encode, Decode, PartialEq, Eq, derive_more::Into, derive_more::DebugCustom, derive_more::Display,
)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
#[debug(fmt = "HashOf<{}>({hash:?})", "private::shortname::<S>()")]
#[display(fmt = "{hash}")]
pub struct HashOf<S: Sealed + 'static> {
    hash: H256,
    #[into(ignore)]
    #[codec(skip)]
    #[cfg_attr(feature = "std", serde(skip))]
    _phantom: PhantomData<S>,
}

impl<S: Sealed> Clone for HashOf<S> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<S: Sealed> Copy for HashOf<S> {}

impl<S: Sealed> HashOf<S> {
    /// # Safety
    /// Use it only for low-level storage implementations or tests.
    pub unsafe fn new(hash: H256) -> Self {
        Self {
            hash,
            _phantom: PhantomData,
        }
    }

    pub fn hash(&self) -> H256 {
        self.hash
    }
}

#[derive(
    Encode,
    Decode,
    PartialEq,
    Eq,
    derive_more::Into,
    derive_more::From,
    derive_more::DebugCustom,
    derive_more::Display,
)]
#[cfg_attr(
    feature = "std",
    derive(serde::Serialize, serde::Deserialize),
    serde(bound = "")
)]
#[debug(
    fmt = "MaybeHashOf<{}>({})",
    "private::shortname::<S>()",
    "self.hash().map(|v| v.to_string()).unwrap_or_else(|| String::from(\"<none>\"))"
)]
#[display(
    fmt = "{}",
    "_0.map(|v| v.to_string()).unwrap_or_else(|| String::from(\"<none>\"))"
)]
pub struct MaybeHashOf<S: Sealed + 'static>(Option<HashOf<S>>);

impl<S: Sealed> Clone for MaybeHashOf<S> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<S: Sealed> Copy for MaybeHashOf<S> {}

impl<S: Sealed> MaybeHashOf<S> {
    pub const fn empty() -> Self {
        Self(None)
    }

    pub const fn is_empty(&self) -> bool {
        self.0.is_none()
    }

    pub fn hash(&self) -> Option<HashOf<S>> {
        self.0
    }

    pub fn with_hash<T>(&self, f: impl FnOnce(HashOf<S>) -> T) -> Option<T> {
        self.hash().map(f)
    }

    pub fn with_hash_or_default<T: Default>(&self, f: impl FnOnce(HashOf<S>) -> T) -> T {
        self.with_hash(f).unwrap_or_default()
    }

    pub fn with_hash_or_default_fallible<T: Default>(
        &self,
        f: impl FnOnce(HashOf<S>) -> Result<T>,
    ) -> Result<T> {
        self.with_hash(f).unwrap_or_else(|| Ok(Default::default()))
    }

    pub fn replace(&mut self, other: Option<Self>) {
        if let Some(other) = other {
            *self = other;
        }
    }
}

impl<S: Sealed + 'static> From<HashOf<S>> for MaybeHashOf<S> {
    fn from(value: HashOf<S>) -> Self {
        Self(Some(value))
    }
}

impl MaybeHashOf<Allocations> {
    pub fn query<S: Storage>(&self, storage: &S) -> Result<Allocations> {
        self.with_hash_or_default_fallible(|hash| {
            storage.read_allocations(hash).ok_or(anyhow!(
                "failed to read ['Allocations'] from storage by hash"
            ))
        })
    }

    pub fn modify_allocations<S: Storage, T>(
        &mut self,
        storage: &S,
        f: impl FnOnce(&mut Allocations) -> T,
    ) -> T {
        let mut allocations = self.query(storage).expect("failed to modify allocations");

        let r = f(&mut allocations);

        self.replace(allocations.store(storage));

        r
    }
}

impl MaybeHashOf<DispatchStash> {
    pub fn query<S: Storage>(&self, storage: &S) -> Result<DispatchStash> {
        self.with_hash_or_default_fallible(|hash| {
            storage.read_stash(hash).ok_or(anyhow!(
                "failed to read ['DispatchStash'] from storage by hash"
            ))
        })
    }

    pub fn modify_stash<S: Storage, T>(
        &mut self,
        storage: &S,
        f: impl FnOnce(&mut DispatchStash) -> T,
    ) -> T {
        let mut stash = self.query(storage).expect("failed to modify stash");

        let r = f(&mut stash);

        *self = stash.store(storage);

        r
    }
}

impl MaybeHashOf<Mailbox> {
    pub fn query<S: Storage>(&self, storage: &S) -> Result<Mailbox> {
        self.with_hash_or_default_fallible(|hash| {
            storage
                .read_mailbox(hash)
                .ok_or(anyhow!("failed to read ['Mailbox'] from storage by hash"))
        })
    }

    pub fn modify_mailbox<S: Storage, T>(
        &mut self,
        storage: &S,
        f: impl FnOnce(&mut Mailbox) -> T,
    ) -> T {
        let mut mailbox = self.query(storage).expect("failed to modify mailbox");

        let r = f(&mut mailbox);

        self.replace(mailbox.store(storage));

        r
    }
}

impl MaybeHashOf<MemoryPages> {
    pub fn query<S: Storage>(&self, storage: &S) -> Result<MemoryPages> {
        self.with_hash_or_default_fallible(|hash| {
            storage.read_pages(hash).ok_or(anyhow!(
                "failed to read ['MemoryPages'] from storage by hash"
            ))
        })
    }

    pub fn modify_pages<S: Storage, T>(
        &mut self,
        storage: &S,
        f: impl FnOnce(&mut MemoryPages) -> T,
    ) -> T {
        let mut pages = self.query(storage).expect("failed to modify memory pages");

        let r = f(&mut pages);

        *self = pages.store(storage);

        r
    }
}

impl MaybeHashOf<MessageQueue> {
    pub fn query<S: Storage>(&self, storage: &S) -> Result<MessageQueue> {
        self.with_hash_or_default_fallible(|hash| {
            storage.read_queue(hash).ok_or(anyhow!(
                "failed to read ['MessageQueue'] from storage by hash"
            ))
        })
    }

    pub fn modify_queue<S: Storage, T>(
        &mut self,
        storage: &S,
        f: impl FnOnce(&mut MessageQueue) -> T,
    ) -> T {
        let mut queue = self.query(storage).expect("failed to modify queue");

        let r = f(&mut queue);

        *self = queue.store(storage);

        r
    }
}

impl MaybeHashOf<Payload> {
    pub fn query<S: Storage>(&self, storage: &S) -> Result<Payload> {
        self.with_hash_or_default_fallible(|hash| {
            storage
                .read_payload(hash)
                .ok_or_else(|| anyhow!("failed to read ['Payload'] from storage by hash"))
        })
    }

    // TODO (breathx): enum for caught value
}

impl MaybeHashOf<Waitlist> {
    pub fn query<S: Storage>(&self, storage: &S) -> Result<Waitlist> {
        self.with_hash_or_default_fallible(|hash| {
            storage
                .read_waitlist(hash)
                .ok_or(anyhow!("failed to read ['Waitlist'] from storage by hash"))
        })
    }

    pub fn modify_waitlist<S: Storage, T>(
        &mut self,
        storage: &S,
        f: impl FnOnce(&mut Waitlist) -> T,
    ) -> T {
        let mut waitlist = self.query(storage).expect("failed to modify waitlist");

        let r = f(&mut waitlist);

        self.replace(waitlist.store(storage));

        r
    }
}

#[derive(Clone, Debug, Decode, Encode, PartialEq, Eq)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct ActiveProgram {
    /// Hash of wasm memory pages allocations, see [`Allocations`].
    pub allocations_hash: MaybeHashOf<Allocations>,
    /// Hash of memory pages table, see [`MemoryPages`].
    pub pages_hash: MaybeHashOf<MemoryPages>,
    /// Program memory infix.
    pub memory_infix: MemoryInfix,
    /// Program initialization status.
    pub initialized: bool,
}

#[derive(Clone, Debug, Decode, Encode, PartialEq, Eq)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
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
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct ProgramState {
    /// Active, exited or terminated program state.
    pub program: Program,
    /// Hash of incoming message queue, see [`MessageQueue`].
    pub queue_hash: MaybeHashOf<MessageQueue>,
    /// Hash of waiting messages list, see [`Waitlist`].
    pub waitlist_hash: MaybeHashOf<Waitlist>,
    /// Hash of dispatch stash, see [`DispatchStash`].
    pub stash_hash: MaybeHashOf<DispatchStash>,
    /// Hash of mailboxed messages, see [`Mailbox`].
    pub mailbox_hash: MaybeHashOf<Mailbox>,
    /// Reducible balance.
    pub balance: Value,
    /// Executable balance.
    pub executable_balance: Value,
}

impl ProgramState {
    pub const fn zero() -> Self {
        Self {
            program: Program::Active(ActiveProgram {
                allocations_hash: MaybeHashOf::empty(),
                pages_hash: MaybeHashOf::empty(),
                memory_infix: MemoryInfix::new(0),
                initialized: false,
            }),
            queue_hash: MaybeHashOf::empty(),
            waitlist_hash: MaybeHashOf::empty(),
            stash_hash: MaybeHashOf::empty(),
            mailbox_hash: MaybeHashOf::empty(),
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

#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct Dispatch {
    /// Message id.
    pub id: MessageId,
    /// Dispatch kind.
    pub kind: DispatchKind,
    /// Message source.
    pub source: ProgramId,
    /// Message payload.
    pub payload: PayloadLookup,
    /// Message value.
    pub value: Value,
    /// Message details like reply message ID, status code, etc.
    pub details: Option<MessageDetails>,
    /// Message previous executions context.
    pub context: Option<ContextStore>,
}

impl Dispatch {
    pub fn new<S: Storage>(
        storage: &S,
        id: MessageId,
        source: ActorId,
        payload: Vec<u8>,
        value: u128,
        is_init: bool,
    ) -> Result<Self> {
        let payload = storage.write_payload_raw(payload)?;

        let kind = if is_init {
            DispatchKind::Init
        } else {
            DispatchKind::Handle
        };

        Ok(Self {
            id,
            kind,
            source,
            payload,
            value,
            details: None,
            context: None,
        })
    }

    pub fn new_reply<S: Storage>(
        storage: &S,
        replied_to: MessageId,
        source: ActorId,
        payload: Vec<u8>,
        value: u128,
    ) -> Result<Self> {
        let payload_hash = storage.write_payload_raw(payload)?;

        Ok(Self::reply(
            replied_to,
            source,
            payload_hash,
            value,
            SuccessReplyReason::Manual,
        ))
    }

    pub fn reply(
        reply_to: MessageId,
        source: ActorId,
        payload: PayloadLookup,
        value: u128,
        reply_code: impl Into<ReplyCode>,
    ) -> Self {
        Self {
            id: MessageId::generate_reply(reply_to),
            kind: DispatchKind::Reply,
            source,
            payload,
            value,
            details: Some(ReplyDetails::new(reply_to, reply_code.into()).into()),
            context: None,
        }
    }

    pub fn from_stored<S: Storage>(storage: &S, value: StoredDispatch) -> Self {
        let (kind, message, context) = value.into_parts();
        let (id, source, _destination, payload, value, details) = message.into_parts();

        let payload = storage
            .write_payload_raw(payload.into_vec())
            .expect("infallible due to recasts (only panics on len)");

        Self {
            id,
            kind,
            source,
            payload,
            value,
            details,
            context,
        }
    }

    pub fn into_message<S: Storage>(self, storage: &S, destination: ActorId) -> Message {
        let Self {
            id,
            payload,
            value,
            details,
            ..
        } = self;

        let payload = payload.query(storage).expect("must be found").into_vec();

        Message {
            id,
            destination,
            payload,
            value,
            reply_details: details.and_then(|d| d.to_reply_details()),
        }
    }
}

#[derive(Clone, Default, Debug, Encode, Decode, PartialEq, Eq)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct ValueWithExpiry<T> {
    pub value: T,
    pub expiry: u32,
}

impl<T> From<(T, u32)> for ValueWithExpiry<T> {
    fn from((value, expiry): (T, u32)) -> Self {
        Self { value, expiry }
    }
}

#[derive(Clone, Default, Debug, Encode, Decode, PartialEq, Eq, derive_more::Into)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct MessageQueue(VecDeque<Dispatch>);

impl MessageQueue {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn queue(&mut self, dispatch: Dispatch) {
        self.0.push_back(dispatch);
    }

    pub fn dequeue(&mut self) -> Option<Dispatch> {
        self.0.pop_front()
    }

    pub fn store<S: Storage>(self, storage: &S) -> MaybeHashOf<Self> {
        MaybeHashOf((!self.0.is_empty()).then(|| storage.write_queue(self)))
    }
}

#[derive(Clone, Default, Debug, Encode, Decode, PartialEq, Eq, derive_more::Into)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct Waitlist {
    inner: BTreeMap<MessageId, ValueWithExpiry<Dispatch>>,
    #[into(ignore)]
    #[codec(skip)]
    changed: bool,
}

impl Waitlist {
    pub fn wait(&mut self, message_id: MessageId, dispatch: Dispatch, expiry: u32) {
        self.changed = true;

        let r = self.inner.insert(
            message_id,
            ValueWithExpiry {
                value: dispatch,
                expiry,
            },
        );
        debug_assert!(r.is_none())
    }

    pub fn wake(&mut self, message_id: &MessageId) -> Option<ValueWithExpiry<Dispatch>> {
        self.inner
            .remove(message_id)
            .inspect(|_| self.changed = true)
    }

    pub fn store<S: Storage>(self, storage: &S) -> Option<MaybeHashOf<Self>> {
        self.changed
            .then(|| MaybeHashOf((!self.inner.is_empty()).then(|| storage.write_waitlist(self))))
    }

    pub fn into_inner(self) -> BTreeMap<MessageId, ValueWithExpiry<Dispatch>> {
        self.into()
    }
}

#[derive(Clone, Default, Debug, Encode, Decode, PartialEq, Eq, derive_more::Into)]
pub struct DispatchStash(BTreeMap<MessageId, ValueWithExpiry<(Dispatch, Option<ActorId>)>>);

impl DispatchStash {
    pub fn add_to_program(&mut self, message_id: MessageId, dispatch: Dispatch, expiry: u32) {
        let r = self.0.insert(
            message_id,
            ValueWithExpiry {
                value: (dispatch, None),
                expiry,
            },
        );
        debug_assert!(r.is_none());
    }

    pub fn add_to_user(
        &mut self,
        message_id: MessageId,
        dispatch: Dispatch,
        expiry: u32,
        user_id: ActorId,
    ) {
        let r = self.0.insert(
            message_id,
            ValueWithExpiry {
                value: (dispatch, Some(user_id)),
                expiry,
            },
        );
        debug_assert!(r.is_none());
    }

    pub fn remove_to_program(&mut self, message_id: &MessageId) -> Dispatch {
        let ValueWithExpiry {
            value: (dispatch, user_id),
            ..
        } = self
            .0
            .remove(message_id)
            .expect("unknown mid queried from stash");

        if user_id.is_some() {
            panic!("stashed message was intended to be sent to program, but keeps data for user");
        }

        dispatch
    }

    pub fn remove_to_user(&mut self, message_id: &MessageId) -> (Dispatch, ActorId) {
        let ValueWithExpiry {
            value: (dispatch, user_id),
            ..
        } = self
            .0
            .remove(message_id)
            .expect("unknown mid queried from stash");

        let user_id = user_id
            .expect("stashed mid was intended to be sent to user, but keeps no data for user");

        (dispatch, user_id)
    }

    pub fn store<S: Storage>(self, storage: &S) -> MaybeHashOf<Self> {
        MaybeHashOf((!self.0.is_empty()).then(|| storage.write_stash(self)))
    }
}

// TODO (breathx): consider here LocalMailbox for each user.
#[derive(Clone, Default, Debug, Encode, Decode, PartialEq, Eq, derive_more::Into)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct Mailbox {
    inner: BTreeMap<ActorId, BTreeMap<MessageId, ValueWithExpiry<Value>>>,
    #[into(ignore)]
    #[codec(skip)]
    changed: bool,
}

impl Mailbox {
    pub fn add(&mut self, user_id: ActorId, message_id: MessageId, value: Value, expiry: u32) {
        self.changed = true;

        let r = self
            .inner
            .entry(user_id)
            .or_default()
            .insert(message_id, ValueWithExpiry { value, expiry });
        debug_assert!(r.is_none())
    }

    pub fn remove(
        &mut self,
        user_id: ActorId,
        message_id: MessageId,
    ) -> Option<ValueWithExpiry<u128>> {
        let local_mailbox = self.inner.get_mut(&user_id)?;
        let claimed_value = local_mailbox.remove(&message_id)?;

        self.changed = true;

        if local_mailbox.is_empty() {
            self.inner.remove(&user_id);
        }

        Some(claimed_value)
    }

    pub fn store<S: Storage>(self, storage: &S) -> Option<MaybeHashOf<Self>> {
        self.changed
            .then(|| MaybeHashOf((!self.inner.is_empty()).then(|| storage.write_mailbox(self))))
    }

    pub fn into_inner(self) -> BTreeMap<ActorId, BTreeMap<MessageId, ValueWithExpiry<Value>>> {
        self.into()
    }
}

#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq, derive_more::Into)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct MemoryPages(MemoryPagesInner);

impl Default for MemoryPages {
    fn default() -> Self {
        Self([MaybeHashOf::empty(); MemoryPages::REGIONS_AMOUNT])
    }
}

impl Index<RegionIdx> for MemoryPages {
    type Output = MaybeHashOf<MemoryPagesRegion>;

    fn index(&self, idx: RegionIdx) -> &Self::Output {
        &self.0[idx.0 as usize]
    }
}

impl IndexMut<RegionIdx> for MemoryPages {
    fn index_mut(&mut self, index: RegionIdx) -> &mut Self::Output {
        &mut self.0[index.0 as usize]
    }
}

/// An inner structure for [`MemoryPages`]. Has at [`REGIONS_AMOUNT`](MemoryPages::REGIONS_AMOUNT)
/// entries.
pub type MemoryPagesInner = [MaybeHashOf<MemoryPagesRegion>; MemoryPages::REGIONS_AMOUNT];

impl MemoryPages {
    /// Copy of the gear_core constant defining max pages amount per program.
    pub const MAX_PAGES: usize = gear_core::code::MAX_WASM_PAGES_AMOUNT as usize;

    /// Granularity parameter of how memory pages hashes are stored.
    ///
    /// Instead of a single huge map of GearPage to HashOf<PageBuf>, memory is
    /// stored in page regions. Each region represents the same map,
    /// but with a specific range of GearPage as keys.
    ///
    /// # Safety
    /// Be careful adjusting this value, as it affects the storage invariants.
    /// In case of a change, not only should the database be migrated, but
    /// necessary changes should also be applied in the ethexe lazy pages
    /// host implementation: see the `ThreadParams` struct.
    pub const REGIONS_AMOUNT: usize = 16;

    /// Pages amount per each region.
    pub const PAGES_PER_REGION: usize = Self::MAX_PAGES / Self::REGIONS_AMOUNT;
    const _DIVISIBILITY_ASSERT: () = assert!(Self::MAX_PAGES % Self::REGIONS_AMOUNT == 0);

    pub fn page_region(page: GearPage) -> RegionIdx {
        RegionIdx((u32::from(page) as usize / Self::PAGES_PER_REGION) as u8)
    }

    pub fn update_and_store_regions<S: Storage>(
        &mut self,
        storage: &S,
        new_pages: BTreeMap<GearPage, HashOf<PageBuf>>,
    ) {
        let mut updated_regions = BTreeMap::new();

        let mut current_region_idx = None;
        let mut current_region_entry = None;

        for (page, data) in new_pages {
            let region_idx = Self::page_region(page);

            if current_region_idx != Some(region_idx) {
                let region_entry = updated_regions.entry(region_idx).or_insert_with(|| {
                    self[region_idx]
                        .0
                        .take()
                        .map(|region_hash| {
                            storage
                                .read_pages_region(region_hash)
                                .expect("failed to read region from storage")
                        })
                        .unwrap_or_default()
                });

                current_region_idx = Some(region_idx);
                current_region_entry = Some(region_entry);
            }

            current_region_entry
                .as_mut()
                .expect("infallible; inserted above")
                .0
                .insert(page, data);
        }

        for (region_idx, region) in updated_regions {
            let region_hash = region
                .store(storage)
                .hash()
                .expect("infallible; pages are only appended here, none are removed");

            self[region_idx] = region_hash.into();
        }
    }

    pub fn remove_and_store_regions<S: Storage>(&mut self, storage: &S, pages: &Vec<GearPage>) {
        let mut updated_regions = BTreeMap::new();

        let mut current_region_idx = None;
        let mut current_region_entry = None;

        for page in pages {
            let region_idx = Self::page_region(*page);

            if current_region_idx != Some(region_idx) {
                let region_entry = updated_regions.entry(region_idx).or_insert_with(|| {
                    self[region_idx]
                        .0
                        .take()
                        .map(|region_hash| {
                            storage
                                .read_pages_region(region_hash)
                                .expect("failed to read region from storage")
                        })
                        .unwrap_or_default()
                });

                current_region_idx = Some(region_idx);
                current_region_entry = Some(region_entry);
            }

            current_region_entry
                .as_mut()
                .expect("infallible; inserted above")
                .0
                .remove(page);
        }

        for (region_idx, region) in updated_regions {
            if let Some(region_hash) = region.store(storage).hash() {
                self[region_idx] = region_hash.into();
            }
        }
    }

    pub fn store<S: Storage>(self, storage: &S) -> MaybeHashOf<Self> {
        MaybeHashOf((!self.0.is_empty()).then(|| storage.write_pages(self)))
    }
}

#[derive(Clone, Default, Debug, Encode, Decode, PartialEq, Eq, derive_more::Into)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct MemoryPagesRegion(MemoryPagesRegionInner);

pub type MemoryPagesRegionInner = BTreeMap<GearPage, HashOf<PageBuf>>;

impl MemoryPagesRegion {
    pub fn store<S: Storage>(self, storage: &S) -> MaybeHashOf<Self> {
        MaybeHashOf((!self.0.is_empty()).then(|| storage.write_pages_region(self)))
    }
}

#[derive(Clone, Copy, Debug, Encode, Decode, PartialEq, Eq, PartialOrd, Ord, derive_more::Into)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct RegionIdx(u8);

#[derive(Clone, Default, Debug, Encode, Decode, PartialEq, Eq, derive_more::Into)]
pub struct Allocations {
    inner: IntervalsTree<WasmPage>,
    #[into(ignore)]
    #[codec(skip)]
    changed: bool,
}

impl Allocations {
    pub fn tree_len(&self) -> u32 {
        self.inner.intervals_amount() as u32
    }

    pub fn update(&mut self, allocations: IntervalsTree<WasmPage>) -> Vec<GearPage> {
        let removed_pages: Vec<_> = self
            .inner
            .difference(&allocations)
            .flat_map(|i| i.iter())
            .flat_map(|i| i.to_iter())
            .collect();

        if !removed_pages.is_empty() || allocations.difference(&self.inner).next().is_some() {
            self.changed = true;
            self.inner = allocations;
        }

        removed_pages
    }

    pub fn store<S: Storage>(self, storage: &S) -> Option<MaybeHashOf<Self>> {
        self.changed.then(|| {
            MaybeHashOf(
                (self.inner.intervals_amount() != 0).then(|| storage.write_allocations(self)),
            )
        })
    }
}

pub trait Storage {
    /// Reads program state by state hash.
    fn read_state(&self, hash: H256) -> Option<ProgramState>;

    /// Writes program state and returns its hash.
    fn write_state(&self, state: ProgramState) -> H256;

    /// Reads message queue by queue hash.
    fn read_queue(&self, hash: HashOf<MessageQueue>) -> Option<MessageQueue>;

    /// Writes message queue and returns its hash.
    fn write_queue(&self, queue: MessageQueue) -> HashOf<MessageQueue>;

    /// Reads waitlist by waitlist hash.
    fn read_waitlist(&self, hash: HashOf<Waitlist>) -> Option<Waitlist>;

    /// Writes waitlist and returns its hash.
    fn write_waitlist(&self, waitlist: Waitlist) -> HashOf<Waitlist>;

    /// Reads dispatch stash by its hash.
    fn read_stash(&self, hash: HashOf<DispatchStash>) -> Option<DispatchStash>;

    /// Writes dispatch stash and returns its hash.
    fn write_stash(&self, stash: DispatchStash) -> HashOf<DispatchStash>;

    /// Reads mailbox by mailbox hash.
    fn read_mailbox(&self, hash: HashOf<Mailbox>) -> Option<Mailbox>;

    /// Writes mailbox and returns its hash.
    fn write_mailbox(&self, mailbox: Mailbox) -> HashOf<Mailbox>;

    /// Reads memory pages by pages hash.
    fn read_pages(&self, hash: HashOf<MemoryPages>) -> Option<MemoryPages>;

    /// Writes memory pages region and returns its hash.
    fn read_pages_region(&self, hash: HashOf<MemoryPagesRegion>) -> Option<MemoryPagesRegion>;

    /// Writes memory pages and returns its hash.
    fn write_pages(&self, pages: MemoryPages) -> HashOf<MemoryPages>;

    /// Writes memory pages region and returns its hash.
    fn write_pages_region(&self, pages_region: MemoryPagesRegion) -> HashOf<MemoryPagesRegion>;

    /// Reads allocations by allocations hash.
    fn read_allocations(&self, hash: HashOf<Allocations>) -> Option<Allocations>;

    /// Writes allocations and returns its hash.
    fn write_allocations(&self, allocations: Allocations) -> HashOf<Allocations>;

    /// Reads payload by payload hash.
    fn read_payload(&self, hash: HashOf<Payload>) -> Option<Payload>;

    /// Writes payload and returns its hash.
    fn write_payload(&self, payload: Payload) -> HashOf<Payload>;

    /// Writes payload if it doesnt exceed limits, returning lookup.
    fn write_payload_raw(&self, payload: Vec<u8>) -> Result<PayloadLookup> {
        let payload =
            Payload::try_from(payload).map_err(|_| anyhow!("payload exceeds size limit"))?;

        let res = if payload.inner().len() < PayloadLookup::STORING_THRESHOLD {
            PayloadLookup::Direct(payload)
        } else {
            PayloadLookup::Stored(self.write_payload(payload))
        };

        Ok(res)
    }

    /// Reads page data by page data hash.
    fn read_page_data(&self, hash: HashOf<PageBuf>) -> Option<PageBuf>;

    /// Writes page data and returns its hash.
    fn write_page_data(&self, data: PageBuf) -> HashOf<PageBuf>;

    /// Writes multiple pages data and returns their hashes.
    fn write_pages_data(
        &self,
        pages: BTreeMap<GearPage, PageBuf>,
    ) -> BTreeMap<GearPage, HashOf<PageBuf>> {
        pages
            .into_iter()
            .map(|(k, v)| (k, self.write_page_data(v)))
            .collect()
    }
}
