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

//! State-related data structures.

#[cfg(not(feature = "std"))]
use alloc::string::{String, ToString};
use alloc::{
    collections::{BTreeMap, VecDeque},
    vec::Vec,
};
use anyhow::{Result, anyhow};
use core::{
    any::Any,
    cell::RefCell,
    cmp::Ordering,
    hash::{Hash, Hasher},
    marker::PhantomData,
    mem,
    ops::{Index, IndexMut},
};
use ethexe_common::gear::{Message, Origin};
pub use gear_core::program::ProgramState as InitStatus;
use gear_core::{
    buffer::Payload,
    ids::prelude::MessageIdExt as _,
    memory::PageBuf,
    message::{ContextStore, DispatchKind, MessageDetails, ReplyDetails, StoredDispatch, Value},
    pages::{GearPage, WasmPage, numerated::tree::IntervalsTree},
    program::MemoryInfix,
};
use gear_core_errors::{ReplyCode, SuccessReplyReason};
use gprimitives::{ActorId, H256, MessageId};
use parity_scale_codec::{Decode, Encode};
use private::Sealed;

/// 3h validity in mailbox for 12s blocks.
// TODO (breathx): WITHIN THE PR
pub const MAILBOX_VALIDITY: u32 = 54_000;

mod private {
    use super::*;

    pub trait Sealed {}

    impl Sealed for Allocations {}
    impl Sealed for DispatchStash {}
    impl Sealed for Mailbox {}
    impl Sealed for UserMailbox {}
    impl Sealed for MemoryPages {}
    impl Sealed for MemoryPagesRegion {}
    impl Sealed for MessageQueue {}
    impl Sealed for Payload {}
    impl Sealed for PageBuf {}
    // TODO (breathx): consider using HashOf<ProgramState> everywhere.
    // impl Sealed for ProgramState {}
    impl Sealed for Waitlist {}
}

#[allow(unused)]
fn shortname<S: Any>() -> &'static str {
    core::any::type_name::<S>()
        .split("::")
        .last()
        .expect("name is empty")
}

#[allow(unused)]
fn option_string<S: ToString>(value: &Option<S>) -> String {
    value
        .as_ref()
        .map(|v| v.to_string())
        .unwrap_or_else(|| "<none>".to_string())
}

/// Represents payload provider (lookup).
///
/// Directly keeps payload inside of itself, or keeps hash of payload stored in database.
///
/// Motivation for usage: it's more optimized to held small payloads in place.
/// Zero payload should always be stored directly.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Encode, Decode)]
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
                .payload(hash)
                .ok_or_else(|| anyhow!("failed to read ['Payload'] from storage by hash")),
        }
    }
}

#[derive(Encode, Decode, derive_more::Into, derive_more::Debug, derive_more::Display)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
#[debug("HashOf<{}>({hash:?})", shortname::<S>())]
#[display("{hash}")]
pub struct HashOf<S: Sealed + 'static> {
    hash: H256,
    #[into(ignore)]
    #[codec(skip)]
    #[cfg_attr(feature = "std", serde(skip))]
    _phantom: PhantomData<S>,
}

impl<S: Sealed> PartialEq for HashOf<S> {
    fn eq(&self, other: &Self) -> bool {
        self.hash.eq(&other.hash)
    }
}

impl<S: Sealed> Eq for HashOf<S> {}

impl<S: Sealed> PartialOrd for HashOf<S> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<S: Sealed> Ord for HashOf<S> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.hash.cmp(&other.hash)
    }
}

impl<S: Sealed> Clone for HashOf<S> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<S: Sealed> Copy for HashOf<S> {}

impl<S: Sealed> Hash for HashOf<S> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.hash.hash(state)
    }
}

impl<S: Sealed> HashOf<S> {
    /// # Safety
    /// Use it only for low-level storage implementations or tests.
    pub unsafe fn new(hash: H256) -> Self {
        Self {
            hash,
            _phantom: PhantomData,
        }
    }

    pub fn hash(self) -> H256 {
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
    derive_more::Debug,
    derive_more::Display,
)]
#[cfg_attr(
    feature = "std",
    derive(serde::Serialize, serde::Deserialize),
    serde(bound = "")
)]
#[debug("MaybeHashOf<{}>({})", shortname::<S>(), option_string(&Self::hash(*self)))]
#[display("{}", option_string(_0))]
pub struct MaybeHashOf<S: Sealed + 'static>(Option<HashOf<S>>);

impl<S: Sealed> Clone for MaybeHashOf<S> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<S: Sealed> Copy for MaybeHashOf<S> {}

impl<S: Sealed> Hash for MaybeHashOf<S> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state)
    }
}

impl<S: Sealed> MaybeHashOf<S> {
    pub const fn empty() -> Self {
        Self(None)
    }

    pub const fn is_empty(&self) -> bool {
        self.0.is_none()
    }

    pub fn hash(self) -> Option<H256> {
        self.to_inner().map(HashOf::hash)
    }

    pub fn to_inner(self) -> Option<HashOf<S>> {
        self.0
    }

    pub fn map<T>(&self, f: impl FnOnce(HashOf<S>) -> T) -> Option<T> {
        self.to_inner().map(f)
    }

    pub fn map_or_default<T: Default>(&self, f: impl FnOnce(HashOf<S>) -> T) -> T {
        self.map(f).unwrap_or_default()
    }

    pub fn try_map_or_default<T: Default>(
        &self,
        f: impl FnOnce(HashOf<S>) -> Result<T>,
    ) -> Result<T> {
        self.map(f).unwrap_or_else(|| Ok(Default::default()))
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
        self.try_map_or_default(|hash| {
            storage.allocations(hash).ok_or(anyhow!(
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
        self.try_map_or_default(|hash| {
            storage.dispatch_stash(hash).ok_or(anyhow!(
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
        self.try_map_or_default(|hash| {
            storage
                .mailbox(hash)
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

impl MaybeHashOf<UserMailbox> {
    pub fn query<S: Storage>(&self, storage: &S) -> Result<UserMailbox> {
        self.try_map_or_default(|hash| {
            storage.user_mailbox(hash).ok_or(anyhow!(
                "failed to read ['UserMailbox'] from storage by hash"
            ))
        })
    }
}

impl MaybeHashOf<MemoryPages> {
    pub fn query<S: Storage>(&self, storage: &S) -> Result<MemoryPages> {
        self.try_map_or_default(|hash| {
            storage.memory_pages(hash).ok_or(anyhow!(
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

// TODO(romanm): consider to make it into general primitive: `HashOf`, `SizedHashOf`, `MaybeHashOf`, `SizedMaybeHashOf`
#[derive(Clone, Copy, Debug, Decode, Encode, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct MessageQueueHashWithSize {
    pub hash: MaybeHashOf<MessageQueue>,
    // NOTE: only here to propagate queue size to the parent state (`StateHashWithQueueSize`).
    pub cached_queue_size: u8,
}

impl MessageQueueHashWithSize {
    pub fn query<S: Storage>(&self, storage: &S) -> Result<MessageQueue> {
        self.hash.try_map_or_default(|hash| {
            storage.message_queue(hash).ok_or(anyhow!(
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

        // Emulate saturating behavior for queue size.
        self.cached_queue_size = queue.len().min(u8::MAX as usize) as u8;
        self.hash = queue.store(storage);

        r
    }

    pub fn is_empty(&self) -> bool {
        self.hash.is_empty()
    }
}

impl MaybeHashOf<Payload> {
    pub fn query<S: Storage>(&self, storage: &S) -> Result<Payload> {
        self.try_map_or_default(|hash| {
            storage
                .payload(hash)
                .ok_or_else(|| anyhow!("failed to read ['Payload'] from storage by hash"))
        })
    }

    // TODO (breathx): enum for caught value
}

impl MaybeHashOf<Waitlist> {
    pub fn query<S: Storage>(&self, storage: &S) -> Result<Waitlist> {
        self.try_map_or_default(|hash| {
            storage
                .waitlist(hash)
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

#[derive(Copy, Clone, Debug, Decode, Encode, PartialEq, Eq, Hash)]
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

#[derive(Copy, Clone, Debug, Decode, Encode, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub enum Program {
    Active(ActiveProgram),
    Exited(ActorId),
    Terminated(ActorId),
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
#[derive(Copy, Clone, Debug, Decode, Encode, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct ProgramState {
    /// Active, exited or terminated program state.
    pub program: Program,
    /// Hash of incoming message queue with its cached size, see [`MessageQueueHashWithSize`].
    pub queue: MessageQueueHashWithSize,
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
            queue: MessageQueueHashWithSize {
                hash: MaybeHashOf::empty(),
                cached_queue_size: 0,
            },
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

        self.queue.hash.is_empty() && self.waitlist_hash.is_empty()
    }
}

#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct Dispatch {
    /// Message id.
    pub id: MessageId,
    /// Dispatch kind.
    pub kind: DispatchKind,
    /// Message source.
    pub source: ActorId,
    /// Message payload.
    pub payload: PayloadLookup,
    /// Message value.
    pub value: Value,
    /// Message details like reply message ID, status code, etc.
    pub details: Option<MessageDetails>,
    /// Message previous executions context.
    pub context: Option<ContextStore>,
    /// Origin of the message.
    pub origin: Origin,
    /// If to call on eth.
    /// Currently only used for replies: assert_eq!(message.call, replyToThisMessage.call);
    pub call: bool,
}

impl Dispatch {
    #[allow(clippy::too_many_arguments)]
    pub fn new<S: Storage>(
        storage: &S,
        id: MessageId,
        source: ActorId,
        payload: Vec<u8>,
        value: u128,
        is_init: bool,
        origin: Origin,
        call: bool,
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
            origin,
            call,
        })
    }

    pub fn new_reply<S: Storage>(
        storage: &S,
        replied_to: MessageId,
        source: ActorId,
        payload: Vec<u8>,
        value: u128,
        origin: Origin,
        call: bool,
    ) -> Result<Self> {
        let payload_hash = storage.write_payload_raw(payload)?;

        Ok(Self::reply(
            replied_to,
            source,
            payload_hash,
            value,
            SuccessReplyReason::Manual,
            origin,
            call,
        ))
    }

    pub fn reply(
        reply_to: MessageId,
        source: ActorId,
        payload: PayloadLookup,
        value: u128,
        reply_code: impl Into<ReplyCode>,
        origin: Origin,
        call: bool,
    ) -> Self {
        Self {
            id: MessageId::generate_reply(reply_to),
            kind: DispatchKind::Reply,
            source,
            payload,
            value,
            details: Some(ReplyDetails::new(reply_to, reply_code.into()).into()),
            context: None,
            origin,
            call,
        }
    }

    pub fn from_core_stored<S: Storage>(
        storage: &S,
        value: StoredDispatch,
        origin: Origin,
        call_reply: bool,
    ) -> Self {
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
            origin,
            call: call_reply,
        }
    }

    pub fn into_message<S: Storage>(self, storage: &S, destination: ActorId) -> Message {
        let Self {
            id,
            payload,
            value,
            details,
            call,
            ..
        } = self;

        let payload = payload.query(storage).expect("must be found").into_vec();

        Message {
            id,
            destination,
            payload,
            value,
            reply_details: details.and_then(|d| d.to_reply_details()),
            call,
        }
    }
}

#[derive(Clone, Default, Debug, Encode, Decode, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct Expiring<T> {
    pub value: T,
    pub expiry: u32,
}

impl<T> From<(T, u32)> for Expiring<T> {
    fn from((value, expiry): (T, u32)) -> Self {
        Self { value, expiry }
    }
}

#[derive(
    Clone,
    Default,
    Debug,
    Encode,
    Decode,
    PartialEq,
    Eq,
    Hash,
    derive_more::Into,
    derive_more::AsRef,
    derive_more::IntoIterator,
)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct MessageQueue(VecDeque<Dispatch>);

impl MessageQueue {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn queue(&mut self, dispatch: Dispatch) {
        self.0.push_back(dispatch);
    }

    pub fn dequeue(&mut self) -> Option<Dispatch> {
        self.0.pop_front()
    }

    pub fn peek(&self) -> Option<&Dispatch> {
        self.0.front()
    }

    pub fn store<S: Storage>(self, storage: &S) -> MaybeHashOf<Self> {
        MaybeHashOf((!self.0.is_empty()).then(|| storage.write_message_queue(self)))
    }
}

#[derive(
    Clone,
    Default,
    Debug,
    Encode,
    Decode,
    PartialEq,
    Eq,
    Hash,
    derive_more::Into,
    derive_more::AsRef,
)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct Waitlist {
    #[as_ref]
    inner: BTreeMap<MessageId, Expiring<Dispatch>>,
    #[into(ignore)]
    #[codec(skip)]
    changed: bool,
}

impl Waitlist {
    pub fn wait(&mut self, dispatch: Dispatch, expiry: u32) {
        self.changed = true;

        let r = self.inner.insert(
            dispatch.id,
            Expiring {
                value: dispatch,
                expiry,
            },
        );
        debug_assert!(r.is_none())
    }

    pub fn wake(&mut self, message_id: &MessageId) -> Option<Expiring<Dispatch>> {
        self.inner
            .remove(message_id)
            .inspect(|_| self.changed = true)
    }

    pub fn store<S: Storage>(self, storage: &S) -> Option<MaybeHashOf<Self>> {
        self.changed
            .then(|| MaybeHashOf((!self.inner.is_empty()).then(|| storage.write_waitlist(self))))
    }

    pub fn into_inner(self) -> BTreeMap<MessageId, Expiring<Dispatch>> {
        self.into()
    }
}

#[derive(
    Clone,
    Default,
    Debug,
    Encode,
    Decode,
    PartialEq,
    Eq,
    Hash,
    derive_more::Into,
    derive_more::AsRef,
)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct DispatchStash(BTreeMap<MessageId, Expiring<(Dispatch, Option<ActorId>)>>);

impl DispatchStash {
    pub fn add_to_program(&mut self, dispatch: Dispatch, expiry: u32) {
        let r = self.0.insert(
            dispatch.id,
            Expiring {
                value: (dispatch, None),
                expiry,
            },
        );
        debug_assert!(r.is_none());
    }

    pub fn add_to_user(&mut self, dispatch: Dispatch, expiry: u32, user_id: ActorId) {
        let r = self.0.insert(
            dispatch.id,
            Expiring {
                value: (dispatch, Some(user_id)),
                expiry,
            },
        );
        debug_assert!(r.is_none());
    }

    pub fn remove_to_program(&mut self, message_id: &MessageId) -> Dispatch {
        let Expiring {
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
        let Expiring {
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
        MaybeHashOf((!self.0.is_empty()).then(|| storage.write_dispatch_stash(self)))
    }
}

#[derive(Clone, Default, Debug, Encode, Decode, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct MailboxMessage {
    pub payload: PayloadLookup,
    pub value: Value,
    pub origin: Origin,
}

impl MailboxMessage {
    pub fn new(payload: PayloadLookup, value: Value, origin: Origin) -> Self {
        Self {
            payload,
            value,
            origin,
        }
    }
}

impl From<Dispatch> for MailboxMessage {
    fn from(dispatch: Dispatch) -> Self {
        Self {
            payload: dispatch.payload,
            value: dispatch.value,
            origin: dispatch.origin,
        }
    }
}

#[derive(Clone, Default, Debug, Encode, Decode, PartialEq, Eq, Hash, derive_more::Into)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct UserMailbox(BTreeMap<MessageId, Expiring<MailboxMessage>>);

impl UserMailbox {
    fn add(&mut self, message_id: MessageId, message: MailboxMessage, expiry: u32) {
        let r = self.0.insert(message_id, (message, expiry).into());
        debug_assert!(r.is_none())
    }

    fn remove(&mut self, message_id: MessageId) -> Option<Expiring<MailboxMessage>> {
        self.0.remove(&message_id)
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn store<S: Storage>(self, storage: &S) -> MaybeHashOf<Self> {
        MaybeHashOf((!self.0.is_empty()).then(|| storage.write_user_mailbox(self)))
    }
}

impl AsRef<BTreeMap<MessageId, Expiring<MailboxMessage>>> for UserMailbox {
    fn as_ref(&self) -> &BTreeMap<MessageId, Expiring<MailboxMessage>> {
        &self.0
    }
}

#[derive(
    Clone,
    Default,
    Debug,
    Encode,
    Decode,
    PartialEq,
    Eq,
    Hash,
    derive_more::Into,
    derive_more::AsRef,
)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct Mailbox {
    #[as_ref]
    inner: BTreeMap<ActorId, HashOf<UserMailbox>>,
    #[into(ignore)]
    #[codec(skip)]
    changed: bool,
}

impl Mailbox {
    pub fn add_and_store_user_mailbox<S: Storage>(
        &mut self,
        storage: &S,
        user_id: ActorId,
        message_id: MessageId,
        message: MailboxMessage,
        expiry: u32,
    ) {
        self.changed = true;

        let maybe_hash: MaybeHashOf<UserMailbox> = self.inner.get(&user_id).cloned().into();

        let mut mailbox = maybe_hash
            .query(storage)
            .expect("failed to query user mailbox");

        mailbox.add(message_id, message, expiry);

        let hash = storage.write_user_mailbox(mailbox);

        let _ = self.inner.insert(user_id, hash);
    }

    pub fn remove_and_store_user_mailbox<S: Storage>(
        &mut self,
        storage: &S,
        user_id: ActorId,
        message_id: MessageId,
    ) -> Option<Expiring<MailboxMessage>> {
        let maybe_hash: MaybeHashOf<UserMailbox> = self.inner.get(&user_id).cloned().into();

        let mut mailbox = maybe_hash
            .query(storage)
            .expect("failed to query user mailbox");

        let value = mailbox.remove(message_id);

        if value.is_some() {
            self.changed = true;

            if mailbox.is_empty() {
                self.inner.remove(&user_id);
            } else {
                let hash = mailbox
                    .store(storage)
                    .to_inner()
                    .expect("failed to store user mailbox");

                self.inner.insert(user_id, hash);
            }
        }

        value
    }

    pub fn store<S: Storage>(self, storage: &S) -> Option<MaybeHashOf<Self>> {
        self.changed
            .then(|| MaybeHashOf((!self.inner.is_empty()).then(|| storage.write_mailbox(self))))
    }

    pub fn into_values<S: Storage>(
        self,
        storage: &S,
    ) -> BTreeMap<ActorId, BTreeMap<MessageId, Expiring<MailboxMessage>>> {
        self.inner
            .into_iter()
            .map(|(k, v)| {
                (
                    k,
                    storage
                        .user_mailbox(v)
                        .expect("failed to read user mailbox from store")
                        .0
                        .into_iter()
                        .collect(),
                )
            })
            .collect()
    }
}

#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq, Hash, derive_more::Into)]
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
    /// Instead of a single huge map of `GearPage` to `HashOf<PageBuf>`, memory is
    /// stored in page regions. Each region represents the same map,
    /// but with a specific range of `GearPage` as keys.
    ///
    /// # Safety
    /// Be careful adjusting this value, as it affects the storage invariants.
    /// In case of a change, not only should the database be migrated, but
    /// necessary changes should also be applied in the ethexe lazy pages
    /// host implementation: see the `ThreadParams` struct.
    pub const REGIONS_AMOUNT: usize = 16;

    /// Pages amount per each region.
    pub const PAGES_PER_REGION: usize = Self::MAX_PAGES / Self::REGIONS_AMOUNT;
    const _DIVISIBILITY_ASSERT: () = assert!(Self::MAX_PAGES.is_multiple_of(Self::REGIONS_AMOUNT));

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
                                .memory_pages_region(region_hash)
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
                .to_inner()
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
                                .memory_pages_region(region_hash)
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
            if let Some(region_hash) = region.store(storage).to_inner() {
                self[region_idx] = region_hash.into();
            }
        }
    }

    pub fn store<S: Storage>(self, storage: &S) -> MaybeHashOf<Self> {
        MaybeHashOf((!self.0.is_empty()).then(|| storage.write_memory_pages(self)))
    }

    pub fn to_inner(&self) -> MemoryPagesInner {
        self.0
    }
}

#[derive(Clone, Default, Debug, Encode, Decode, PartialEq, Eq, Hash, derive_more::Into)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct MemoryPagesRegion(MemoryPagesRegionInner);

pub type MemoryPagesRegionInner = BTreeMap<GearPage, HashOf<PageBuf>>;

impl MemoryPagesRegion {
    pub fn store<S: Storage>(self, storage: &S) -> MaybeHashOf<Self> {
        MaybeHashOf((!self.0.is_empty()).then(|| storage.write_memory_pages_region(self)))
    }

    pub fn as_inner(&self) -> &MemoryPagesRegionInner {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Encode, Decode, PartialEq, Eq, PartialOrd, Ord, derive_more::Into)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct RegionIdx(u8);

#[derive(Clone, Default, Debug, Encode, Decode, PartialEq, Eq, Hash, derive_more::Into)]
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

#[auto_impl::auto_impl(&, Box)]
pub trait Storage {
    /// Reads program state by state hash.
    fn program_state(&self, hash: H256) -> Option<ProgramState>;

    /// Writes program state and returns its hash.
    fn write_program_state(&self, state: ProgramState) -> H256;

    /// Reads message queue by queue hash.
    fn message_queue(&self, hash: HashOf<MessageQueue>) -> Option<MessageQueue>;

    /// Writes message queue and returns its hash.
    fn write_message_queue(&self, queue: MessageQueue) -> HashOf<MessageQueue>;

    /// Reads waitlist by waitlist hash.
    fn waitlist(&self, hash: HashOf<Waitlist>) -> Option<Waitlist>;

    /// Writes waitlist and returns its hash.
    fn write_waitlist(&self, waitlist: Waitlist) -> HashOf<Waitlist>;

    /// Reads dispatch stash by its hash.
    fn dispatch_stash(&self, hash: HashOf<DispatchStash>) -> Option<DispatchStash>;

    /// Writes dispatch stash and returns its hash.
    fn write_dispatch_stash(&self, stash: DispatchStash) -> HashOf<DispatchStash>;

    /// Reads mailbox by mailbox hash.
    fn mailbox(&self, hash: HashOf<Mailbox>) -> Option<Mailbox>;

    /// Writes mailbox and returns its hash.
    fn write_mailbox(&self, mailbox: Mailbox) -> HashOf<Mailbox>;

    /// Reads user mailbox and returns its hash.
    fn user_mailbox(&self, hash: HashOf<UserMailbox>) -> Option<UserMailbox>;

    /// Writes user mailbox and returns its hash.
    fn write_user_mailbox(&self, user_mailbox: UserMailbox) -> HashOf<UserMailbox>;

    /// Reads memory pages by pages hash.
    fn memory_pages(&self, hash: HashOf<MemoryPages>) -> Option<MemoryPages>;

    /// Writes memory pages region and returns its hash.
    fn memory_pages_region(&self, hash: HashOf<MemoryPagesRegion>) -> Option<MemoryPagesRegion>;

    /// Writes memory pages and returns its hash.
    fn write_memory_pages(&self, pages: MemoryPages) -> HashOf<MemoryPages>;

    /// Writes memory pages region and returns its hash.
    fn write_memory_pages_region(
        &self,
        pages_region: MemoryPagesRegion,
    ) -> HashOf<MemoryPagesRegion>;

    /// Reads allocations by allocations hash.
    fn allocations(&self, hash: HashOf<Allocations>) -> Option<Allocations>;

    /// Writes allocations and returns its hash.
    fn write_allocations(&self, allocations: Allocations) -> HashOf<Allocations>;

    /// Reads payload by payload hash.
    fn payload(&self, hash: HashOf<Payload>) -> Option<Payload>;

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
    fn page_data(&self, hash: HashOf<PageBuf>) -> Option<PageBuf>;

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

/// In-memory storage for testing purposes.
#[derive(Debug, Default)]
pub struct MemStorage {
    inner: RefCell<BTreeMap<H256, Vec<u8>>>,
}

impl MemStorage {
    fn read<T: Decode>(&self, hash: H256) -> Option<T> {
        self.inner
            .borrow()
            .get(&hash)
            .map(|vec| Decode::decode(&mut &vec[..]).unwrap())
    }

    fn write<T: Encode>(&self, value: T) -> H256 {
        let value = value.encode();
        let hash = gear_core::utils::hash(&value);
        let hash = H256(hash);
        self.inner.borrow_mut().insert(hash, value);
        hash
    }
}

impl Storage for MemStorage {
    fn program_state(&self, hash: H256) -> Option<ProgramState> {
        self.read(hash)
    }

    fn write_program_state(&self, state: ProgramState) -> H256 {
        self.write(state)
    }

    fn message_queue(&self, hash: HashOf<MessageQueue>) -> Option<MessageQueue> {
        self.read(hash.hash())
    }

    fn write_message_queue(&self, queue: MessageQueue) -> HashOf<MessageQueue> {
        unsafe { HashOf::new(self.write(queue)) }
    }

    fn waitlist(&self, hash: HashOf<Waitlist>) -> Option<Waitlist> {
        self.read(hash.hash())
    }

    fn write_waitlist(&self, waitlist: Waitlist) -> HashOf<Waitlist> {
        unsafe { HashOf::new(self.write(waitlist)) }
    }

    fn dispatch_stash(&self, hash: HashOf<DispatchStash>) -> Option<DispatchStash> {
        self.read(hash.hash())
    }

    fn write_dispatch_stash(&self, stash: DispatchStash) -> HashOf<DispatchStash> {
        unsafe { HashOf::new(self.write(stash)) }
    }

    fn mailbox(&self, hash: HashOf<Mailbox>) -> Option<Mailbox> {
        self.read(hash.hash())
    }

    fn write_mailbox(&self, mailbox: Mailbox) -> HashOf<Mailbox> {
        unsafe { HashOf::new(self.write(mailbox)) }
    }

    fn user_mailbox(&self, hash: HashOf<UserMailbox>) -> Option<UserMailbox> {
        self.read(hash.hash())
    }

    fn write_user_mailbox(&self, user_mailbox: UserMailbox) -> HashOf<UserMailbox> {
        unsafe { HashOf::new(self.write(user_mailbox)) }
    }

    fn memory_pages(&self, hash: HashOf<MemoryPages>) -> Option<MemoryPages> {
        self.read(hash.hash())
    }

    fn memory_pages_region(&self, hash: HashOf<MemoryPagesRegion>) -> Option<MemoryPagesRegion> {
        self.read(hash.hash())
    }

    fn write_memory_pages(&self, pages: MemoryPages) -> HashOf<MemoryPages> {
        unsafe { HashOf::new(self.write(pages)) }
    }

    fn write_memory_pages_region(
        &self,
        pages_region: MemoryPagesRegion,
    ) -> HashOf<MemoryPagesRegion> {
        unsafe { HashOf::new(self.write(pages_region)) }
    }

    fn allocations(&self, hash: HashOf<Allocations>) -> Option<Allocations> {
        self.read(hash.hash())
    }

    fn write_allocations(&self, allocations: Allocations) -> HashOf<Allocations> {
        unsafe { HashOf::new(self.write(allocations)) }
    }

    fn payload(&self, hash: HashOf<Payload>) -> Option<Payload> {
        self.read(hash.hash())
    }

    fn write_payload(&self, payload: Payload) -> HashOf<Payload> {
        unsafe { HashOf::new(self.write(payload)) }
    }

    fn page_data(&self, hash: HashOf<PageBuf>) -> Option<PageBuf> {
        self.read(hash.hash())
    }

    fn write_page_data(&self, data: PageBuf) -> HashOf<PageBuf> {
        unsafe { HashOf::new(self.write(data)) }
    }
}
