// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

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
    hash::Hash,
    mem,
    ops::{Index, IndexMut},
};
use ethexe_common::{
    HashOf, MaybeHashOf,
    gear::{Message, MessageType},
};
/// Re-export of `gear_core::program::ProgramState` as `InitStatus` for use in ethexe state.
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

/// 3h validity in mailbox for 12s blocks.
// TODO (breathx): WITHIN THE PR
pub const MAILBOX_VALIDITY: u32 = 54_000;

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
    /// Payload bytes held inline.
    Direct(Payload),
    /// Payload is stored in the database; this variant carries its content-addressed hash.
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

    /// Returns a [`PayloadLookup::Direct`] variant with an empty payload.
    pub const fn empty() -> Self {
        Self::Direct(Payload::new())
    }

    /// Returns `true` only when the payload is [`Direct`](PayloadLookup::Direct) and has zero bytes.
    pub fn is_empty(&self) -> bool {
        if let Self::Direct(payload) = self {
            payload.is_empty()
        } else {
            false
        }
    }

    /// Ensures the payload is persisted in storage, converting a `Direct` variant to `Stored`.
    ///
    /// Returns the content hash of the payload. If already `Stored`, returns the existing hash
    /// without writing again.
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

    /// Resolves the payload to its bytes, fetching from storage if necessary.
    pub fn query<S: Storage>(self, storage: &S) -> Result<Payload> {
        match self {
            Self::Direct(payload) => Ok(payload),
            Self::Stored(hash) => storage
                .payload(hash)
                .ok_or_else(|| anyhow!("failed to read ['Payload'] from storage by hash")),
        }
    }
}

impl<S: Storage> QueryableStorage<Allocations> for S {
    fn query(&self, hash: &MaybeHashOf<Allocations>) -> Result<Allocations> {
        hash.try_map_or_default(|hash| {
            self.allocations(hash).ok_or(anyhow!(
                "failed to read ['Allocations'] from storage by hash"
            ))
        })
    }
}

impl<S: Storage> ModifiableStorage<Allocations> for S {
    fn modify<U>(
        &self,
        hash: &mut MaybeHashOf<Allocations>,
        f: impl FnOnce(&mut Allocations) -> U,
    ) -> U {
        let mut allocations = self.query(hash).expect("failed to modify allocations");

        let r = f(&mut allocations);

        hash.replace(allocations.store(&self));

        r
    }
}

impl<S: Storage> QueryableStorage<DispatchStash> for S {
    fn query(&self, hash: &MaybeHashOf<DispatchStash>) -> Result<DispatchStash> {
        hash.try_map_or_default(|hash| {
            self.dispatch_stash(hash).ok_or(anyhow!(
                "failed to read ['DispatchStash'] from storage by hash"
            ))
        })
    }
}

impl<S: Storage> ModifiableStorage<DispatchStash> for S {
    fn modify<U>(
        &self,
        hash: &mut MaybeHashOf<DispatchStash>,
        f: impl FnOnce(&mut DispatchStash) -> U,
    ) -> U {
        let mut stash = self.query(hash).expect("failed to modify stash");

        let r = f(&mut stash);

        *hash = stash.store(&self);

        r
    }
}

impl<S: Storage> QueryableStorage<Mailbox> for S {
    fn query(&self, hash: &MaybeHashOf<Mailbox>) -> Result<Mailbox> {
        hash.try_map_or_default(|hash| {
            self.mailbox(hash)
                .ok_or(anyhow!("failed to read ['Mailbox'] from storage by hash"))
        })
    }
}

impl<S: Storage> ModifiableStorage<Mailbox> for S {
    fn modify<U>(&self, hash: &mut MaybeHashOf<Mailbox>, f: impl FnOnce(&mut Mailbox) -> U) -> U {
        let mut mailbox = self.query(hash).expect("failed to modify mailbox");

        let r = f(&mut mailbox);

        hash.replace(mailbox.store(&self));

        r
    }
}

impl<S: Storage> QueryableStorage<UserMailbox> for S {
    fn query(&self, hash: &MaybeHashOf<UserMailbox>) -> Result<UserMailbox> {
        hash.try_map_or_default(|hash| {
            self.user_mailbox(hash).ok_or(anyhow!(
                "failed to read ['UserMailbox'] from storage by hash"
            ))
        })
    }
}

impl<S: Storage> QueryableStorage<MemoryPages> for S {
    fn query(&self, hash: &MaybeHashOf<MemoryPages>) -> Result<MemoryPages> {
        hash.try_map_or_default(|hash| {
            self.memory_pages(hash).ok_or(anyhow!(
                "failed to read ['MemoryPages'] from storage by hash"
            ))
        })
    }
}

impl<S: Storage> ModifiableStorage<MemoryPages> for S {
    fn modify<U>(
        &self,
        hash: &mut MaybeHashOf<MemoryPages>,
        f: impl FnOnce(&mut MemoryPages) -> U,
    ) -> U {
        let mut pages = self.query(hash).expect("failed to modify memory pages");

        let r = f(&mut pages);

        *hash = pages.store(&self);

        r
    }
}

// TODO(romanm): consider to make it into general primitive: `HashOf`, `SizedHashOf`, `MaybeHashOf`, `SizedMaybeHashOf`
/// A [`MessageQueue`] hash paired with a cached count of pending messages.
///
/// The `cached_queue_size` is a saturating `u8` snapshot of the queue length, propagated to
/// the parent [`ProgramState`] so callers can gauge queue depth without fetching the full queue.
#[derive(Clone, Copy, Debug, Decode, Encode, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct MessageQueueHashWithSize {
    /// Content-addressed hash of the [`MessageQueue`], or empty if the queue is absent.
    pub hash: MaybeHashOf<MessageQueue>,
    // NOTE: only here to propagate queue size to the parent state (`StateHashWithQueueSize`).
    /// Saturating snapshot of the queue length; capped at `u8::MAX`.
    pub cached_queue_size: u8,
}

impl MessageQueueHashWithSize {
    /// Reads the [`MessageQueue`] from storage, returning an empty queue if the hash is absent.
    pub fn query<S: Storage + ?Sized>(&self, storage: &S) -> Result<MessageQueue> {
        self.hash.try_map_or_default(|hash| {
            storage.message_queue(hash).ok_or(anyhow!(
                "failed to read ['MessageQueue'] from storage by hash"
            ))
        })
    }

    /// Loads the queue, applies `f`, then re-serializes it, updating both `hash` and
    /// `cached_queue_size` atomically.
    pub fn modify_queue<S: Storage + ?Sized, T>(
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

    /// Returns `true` if the queue hash is absent (no messages queued).
    pub fn is_empty(&self) -> bool {
        self.hash.is_empty()
    }
}

impl<S: Storage> QueryableStorage<Payload> for S {
    fn query(&self, hash: &MaybeHashOf<Payload>) -> Result<Payload> {
        hash.try_map_or_default(|hash| {
            self.payload(hash)
                .ok_or_else(|| anyhow!("failed to read ['Payload'] from storage by hash"))
        })

        // TODO (breathx): enum for caught value
    }
}

impl<S: Storage> QueryableStorage<Waitlist> for S {
    fn query(&self, hash: &MaybeHashOf<Waitlist>) -> Result<Waitlist> {
        hash.try_map_or_default(|hash| {
            self.waitlist(hash)
                .ok_or(anyhow!("failed to read ['Waitlist'] from storage by hash"))
        })
    }
}

impl<S: Storage> ModifiableStorage<Waitlist> for S {
    fn modify<U>(&self, hash: &mut MaybeHashOf<Waitlist>, f: impl FnOnce(&mut Waitlist) -> U) -> U {
        let mut waitlist = self.query(hash).expect("failed to modify waitlist");

        let r = f(&mut waitlist);

        hash.replace(waitlist.store(&self));

        r
    }
}

/// State of a program that is currently active (not exited or terminated).
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

/// Lifecycle state of a Gear program.
#[derive(Copy, Clone, Debug, Decode, Encode, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub enum Program {
    /// Program is running; contains its runtime state.
    Active(ActiveProgram),
    /// Program has called `gr_exit`; the inheritor `ActorId` receives its remaining balance.
    Exited(ActorId),
    /// Program initialization failed; the inheritor `ActorId` receives its remaining balance.
    Terminated(ActorId),
}

impl Program {
    /// Returns `true` if the program is in the `Active` state.
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active(_))
    }

    /// Returns `true` if the program is active and its initialization has completed.
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
    /// Hash of the incoming Ethereum message queue with its cached size. See [`MessageQueueHashWithSize`].
    pub canonical_queue: MessageQueueHashWithSize,
    /// Hash of the injected message queue with its cached size. See [`MessageQueueHashWithSize`].
    pub injected_queue: MessageQueueHashWithSize,
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
    /// Returns the zero (freshly created, uninitialized) program state with no messages and no balance.
    pub const fn zero() -> Self {
        Self {
            program: Program::Active(ActiveProgram {
                allocations_hash: MaybeHashOf::empty(),
                pages_hash: MaybeHashOf::empty(),
                memory_infix: MemoryInfix::new(0),
                initialized: false,
            }),
            canonical_queue: MessageQueueHashWithSize {
                hash: MaybeHashOf::empty(),
                cached_queue_size: 0,
            },
            injected_queue: MessageQueueHashWithSize {
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

    /// Returns `true` if this state equals [`ProgramState::zero()`].
    pub fn is_zero(&self) -> bool {
        *self == Self::zero()
    }

    /// Returns `true` when the program is uninitialized and all queues are empty,
    /// meaning an `Init` message must be sent before the program can proceed.
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

        self.canonical_queue.hash.is_empty()
            && self.injected_queue.hash.is_empty()
            && self.waitlist_hash.is_empty()
    }

    /// Returns a mutable reference to either the canonical or injected queue
    /// based on the supplied [`MessageType`].
    pub fn queue_from_msg_type(
        &mut self,
        message_type: MessageType,
    ) -> &mut MessageQueueHashWithSize {
        match message_type {
            MessageType::Canonical => &mut self.canonical_queue,
            MessageType::Injected => &mut self.injected_queue,
        }
    }
}

/// An ethexe runtime dispatch: a message together with its routing metadata.
///
/// Wraps a Gear message with the additional context needed by the ethexe runtime
/// (dispatch kind, source, payload lookup, Ethereum `call` flag, and execution context).
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
    /// Type of the message.
    pub message_type: MessageType,
    /// If to call on eth.
    /// Currently only used for replies: assert_eq!(message.call, replyToThisMessage.call);
    pub call: bool,
}

impl Dispatch {
    /// Creates an `Init` or `Handle` dispatch, writing the payload to storage if it is large.
    #[expect(clippy::too_many_arguments)]
    pub fn new<S: Storage + ?Sized>(
        storage: &S,
        id: MessageId,
        source: ActorId,
        payload: Vec<u8>,
        value: u128,
        is_init: bool,
        message_type: MessageType,
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
            message_type,
            call,
        })
    }

    /// Creates a `Reply` dispatch for the given `replied_to` message, writing the payload to storage.
    pub fn new_reply<S: Storage + ?Sized>(
        storage: &S,
        replied_to: MessageId,
        source: ActorId,
        payload: Vec<u8>,
        value: u128,
        message_type: MessageType,
        call: bool,
    ) -> Result<Self> {
        let payload_hash = storage.write_payload_raw(payload)?;

        Ok(Self::reply(
            replied_to,
            source,
            payload_hash,
            value,
            SuccessReplyReason::Manual,
            message_type,
            call,
        ))
    }

    /// Constructs a `Reply` dispatch from an already-resolved [`PayloadLookup`].
    pub fn reply(
        reply_to: MessageId,
        source: ActorId,
        payload: PayloadLookup,
        value: u128,
        reply_code: impl Into<ReplyCode>,
        message_type: MessageType,
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
            message_type,
            call,
        }
    }

    /// Converts a `gear_core` [`StoredDispatch`] into an ethexe [`Dispatch`], persisting its
    /// payload via `storage`.
    pub fn from_core_stored<S: Storage + ?Sized>(
        storage: &S,
        value: StoredDispatch,
        message_type: MessageType,
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
            message_type,
            call: call_reply,
        }
    }

    /// Converts this dispatch into an `ethexe_common::gear::Message` bound for `destination`,
    /// resolving the payload from storage if needed.
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

/// Wraps a value with a block-number expiry.
#[derive(Clone, Default, Debug, Encode, Decode, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct Expiring<T> {
    /// The wrapped value.
    pub value: T,
    /// Block number at which this entry expires and should be removed.
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
/// FIFO queue of [`Dispatch`] entries for a single program.
pub struct MessageQueue(VecDeque<Dispatch>);

impl MessageQueue {
    /// Returns `true` if the queue contains no dispatches.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns the number of dispatches in the queue.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Appends a dispatch to the back of the queue.
    pub fn queue(&mut self, dispatch: Dispatch) {
        self.0.push_back(dispatch);
    }

    /// Removes and returns the dispatch at the front of the queue.
    pub fn dequeue(&mut self) -> Option<Dispatch> {
        self.0.pop_front()
    }

    /// Returns a reference to the dispatch at the front without removing it.
    pub fn peek(&self) -> Option<&Dispatch> {
        self.0.front()
    }

    /// Writes the queue to storage and returns its hash, or an empty hash if the queue is empty.
    pub fn store<S: Storage + ?Sized>(self, storage: &S) -> MaybeHashOf<Self> {
        MaybeHashOf::from_inner((!self.0.is_empty()).then(|| storage.write_message_queue(self)))
    }
}

/// Methods introduced due to solution to #4513.
/// Remove when becomes unnecessary.
impl MessageQueue {
    /// Removes all dispatches from the queue without returning them.
    pub fn clear(&mut self) {
        self.0.clear()
    }

    /// Removes and returns the dispatch at the back of the queue.
    pub fn pop_back(&mut self) -> Option<Dispatch> {
        self.0.pop_back()
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
/// Map of suspended dispatches keyed by [`MessageId`], each with a block-number expiry.
///
/// Programs are placed on the waitlist when they await a reply. They are woken and re-queued
/// when the matching reply arrives or the expiry block is reached.
pub struct Waitlist {
    #[as_ref]
    inner: BTreeMap<MessageId, Expiring<Dispatch>>,
    #[into(ignore)]
    #[codec(skip)]
    changed: bool,
}

impl Waitlist {
    /// Suspends `dispatch` until it is woken or expires at `expiry` block.
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

    /// Removes and returns the dispatch for `message_id`, or `None` if it is not waiting.
    pub fn wake(&mut self, message_id: &MessageId) -> Option<Expiring<Dispatch>> {
        self.inner
            .remove(message_id)
            .inspect(|_| self.changed = true)
    }

    /// Persists the waitlist to storage if it was modified, returning the new hash.
    ///
    /// Returns `None` if the waitlist was not modified since it was last loaded.
    pub fn store<S: Storage>(self, storage: &S) -> Option<MaybeHashOf<Self>> {
        self.changed.then(|| {
            MaybeHashOf::from_inner((!self.inner.is_empty()).then(|| storage.write_waitlist(self)))
        })
    }

    /// Consumes the waitlist and returns the underlying map.
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
/// Temporary storage for dispatches that are awaiting forwarding to either a program or a user.
///
/// Each entry carries an optional [`ActorId`] that, when `Some`, identifies the target user;
/// `None` means the dispatch is destined for a program.
pub struct DispatchStash(BTreeMap<MessageId, Expiring<(Dispatch, Option<ActorId>)>>);

impl DispatchStash {
    /// Stashes a dispatch intended to be forwarded to a program, expiring at `expiry` block.
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

    /// Stashes a dispatch intended to be forwarded to `user_id`, expiring at `expiry` block.
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

    /// Removes and returns the dispatch stashed for program delivery.
    ///
    /// Panics if the message is not found or was stashed for user delivery.
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

    /// Removes and returns the dispatch and its target user stashed for user delivery.
    ///
    /// Panics if the message is not found or was stashed for program delivery.
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

    /// Writes the stash to storage and returns its hash, or an empty hash if the stash is empty.
    pub fn store<S: Storage>(self, storage: &S) -> MaybeHashOf<Self> {
        MaybeHashOf::from_inner((!self.0.is_empty()).then(|| storage.write_dispatch_stash(self)))
    }
}

/// A message held in a user's mailbox, retaining its payload, value, and origin type.
#[derive(Clone, Default, Debug, Encode, Decode, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct MailboxMessage {
    /// Payload bytes or a reference to payload stored in the database.
    pub payload: PayloadLookup,
    /// Value (tokens) attached to the message.
    pub value: Value,
    /// Whether the message originated from the canonical or injected queue.
    pub message_type: MessageType,
}

impl MailboxMessage {
    /// Constructs a [`MailboxMessage`] from its components.
    pub fn new(payload: PayloadLookup, value: Value, message_type: MessageType) -> Self {
        Self {
            payload,
            value,
            message_type,
        }
    }
}

impl From<Dispatch> for MailboxMessage {
    fn from(dispatch: Dispatch) -> Self {
        Self {
            payload: dispatch.payload,
            value: dispatch.value,
            message_type: dispatch.message_type,
        }
    }
}

/// Per-user mailbox: a map from [`MessageId`] to an expiring [`MailboxMessage`].
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

    fn store<S: Storage + ?Sized>(self, storage: &S) -> MaybeHashOf<Self> {
        MaybeHashOf::from_inner((!self.0.is_empty()).then(|| storage.write_user_mailbox(self)))
    }

    /// Constructs a [`UserMailbox`] from an existing map; available in tests and mock builds.
    #[cfg(any(test, feature = "mock"))]
    pub fn from_inner(inner: BTreeMap<MessageId, Expiring<MailboxMessage>>) -> Self {
        Self(inner)
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
/// Top-level mailbox for a program, mapping each user [`ActorId`] to a hash of their
/// [`UserMailbox`].  The `changed` flag enables write-back only when mutations occurred.
pub struct Mailbox {
    #[as_ref]
    inner: BTreeMap<ActorId, HashOf<UserMailbox>>,
    #[into(ignore)]
    #[codec(skip)]
    changed: bool,
}

impl Mailbox {
    /// Adds `message` to `user_id`'s mailbox and immediately persists the updated user mailbox.
    pub fn add_and_store_user_mailbox<S: Storage + ?Sized>(
        &mut self,
        storage: &S,
        user_id: ActorId,
        message_id: MessageId,
        message: MailboxMessage,
        expiry: u32,
    ) {
        self.changed = true;

        let maybe_hash: MaybeHashOf<UserMailbox> = self.inner.get(&user_id).cloned().into();

        let mut mailbox = storage
            .query(&maybe_hash)
            .expect("failed to query user mailbox");

        mailbox.add(message_id, message, expiry);

        let hash = storage.write_user_mailbox(mailbox);

        let _ = self.inner.insert(user_id, hash);
    }

    /// Removes `message_id` from `user_id`'s mailbox and persists the change.
    ///
    /// Returns the removed [`Expiring<MailboxMessage>`] if it existed, or `None`.
    pub fn remove_and_store_user_mailbox<S: Storage + ?Sized>(
        &mut self,
        storage: &S,
        user_id: ActorId,
        message_id: MessageId,
    ) -> Option<Expiring<MailboxMessage>> {
        let maybe_hash: MaybeHashOf<UserMailbox> = self.inner.get(&user_id).cloned().into();

        let mut mailbox = storage
            .query(&maybe_hash)
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

    /// Persists the mailbox to storage if it was modified, returning the new hash.
    ///
    /// Returns `None` when no modifications were made.
    pub fn store<S: Storage>(self, storage: &S) -> Option<MaybeHashOf<Self>> {
        self.changed.then(|| {
            MaybeHashOf::from_inner((!self.inner.is_empty()).then(|| storage.write_mailbox(self)))
        })
    }

    /// Resolves all per-user mailbox hashes and returns a nested map of all mailboxed messages.
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

/// Content-addressed index of a program's WASM memory pages, split into fixed-size regions.
///
/// Each entry in the inner array is a `MaybeHashOf<MemoryPagesRegion>` covering
/// `PAGES_PER_REGION` consecutive [`GearPage`]s.  Only touched pages need to be
/// loaded, matching the lazy-page model used at the native layer.
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
    pub const MAX_PAGES: usize = 4 * gear_core::code::MAX_WASM_PAGES_AMOUNT as usize;

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

    /// Returns the [`RegionIdx`] that contains `page`.
    pub fn page_region(page: GearPage) -> RegionIdx {
        RegionIdx((u32::from(page) as usize / Self::PAGES_PER_REGION) as u8)
    }

    /// Inserts or updates the given pages in the relevant regions, persisting changed regions.
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
                        .to_inner()
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

    /// Removes the given pages from their regions and persists changed regions.
    pub fn remove_and_store_regions<S: Storage>(&mut self, storage: &S, pages: &Vec<GearPage>) {
        let mut updated_regions = BTreeMap::new();

        let mut current_region_idx = None;
        let mut current_region_entry = None;

        for page in pages {
            let region_idx = Self::page_region(*page);

            if current_region_idx != Some(region_idx) {
                let region_entry = updated_regions.entry(region_idx).or_insert_with(|| {
                    self[region_idx]
                        .to_inner()
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
            // TODO #5373
            if let Some(region_hash) = region.store(storage).to_inner() {
                self[region_idx] = region_hash.into();
            }
        }
    }

    /// Writes the pages index to storage and returns its content hash. Because the inner array
    /// always has `REGIONS_AMOUNT` entries, the hash is always non-empty.
    pub fn store<S: Storage>(self, storage: &S) -> MaybeHashOf<Self> {
        MaybeHashOf::from_inner((!self.0.is_empty()).then(|| storage.write_memory_pages(self)))
    }

    /// Returns a copy of the underlying [`MemoryPagesInner`] array.
    pub fn to_inner(&self) -> MemoryPagesInner {
        self.0
    }
}

/// One region of a program's memory pages: a map from [`GearPage`] to its content hash.
#[derive(Clone, Default, Debug, Encode, Decode, PartialEq, Eq, Hash, derive_more::Into)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct MemoryPagesRegion(MemoryPagesRegionInner);

/// Inner map type for [`MemoryPagesRegion`].
pub type MemoryPagesRegionInner = BTreeMap<GearPage, HashOf<PageBuf>>;

impl MemoryPagesRegion {
    /// Writes this region to storage, returning its hash (or empty if the region contains no pages).
    pub fn store<S: Storage>(self, storage: &S) -> MaybeHashOf<Self> {
        MaybeHashOf::from_inner(
            (!self.0.is_empty()).then(|| storage.write_memory_pages_region(self)),
        )
    }

    /// Returns a reference to the underlying [`MemoryPagesRegionInner`] map.
    pub fn as_inner(&self) -> &MemoryPagesRegionInner {
        &self.0
    }

    /// Constructs a [`MemoryPagesRegion`] from an existing map; available in tests and mock builds.
    #[cfg(any(test, feature = "mock"))]
    pub fn from_inner(inner: MemoryPagesRegionInner) -> Self {
        Self(inner)
    }
}

/// Zero-based index into the [`MemoryPages`] regions array.
#[derive(Clone, Copy, Debug, Encode, Decode, PartialEq, Eq, PartialOrd, Ord, derive_more::Into)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct RegionIdx(u8);

/// WASM page allocations for an active program, stored as an interval tree of [`WasmPage`]s.
///
/// The `changed` flag defers serialization: only call [`store`](Allocations::store) if an
/// update was actually performed.
#[derive(Clone, Default, Debug, Encode, Decode, PartialEq, Eq, Hash, derive_more::Into)]
pub struct Allocations {
    inner: IntervalsTree<WasmPage>,
    #[into(ignore)]
    #[codec(skip)]
    changed: bool,
}

impl Allocations {
    /// Returns the number of disjoint intervals in the allocation tree.
    pub fn tree_len(&self) -> u32 {
        self.inner.intervals_amount() as u32
    }

    /// Replaces the allocation tree with `allocations` and returns the [`GearPage`]s that were
    /// deallocated (present before but absent after the update).
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

    /// Persists the allocations to storage if they were modified, returning the new hash.
    ///
    /// Returns `None` when no modifications were made since last load.
    pub fn store<S: Storage>(self, storage: &S) -> Option<MaybeHashOf<Self>> {
        self.changed.then(|| {
            MaybeHashOf::from_inner(
                (self.inner.intervals_amount() != 0).then(|| storage.write_allocations(self)),
            )
        })
    }
}

/// Content-addressed backing store for all ethexe runtime state objects.
///
/// Implementors must be able to read and write every state primitive (program state, queues,
/// waitlist, stash, mailboxes, memory pages, allocations, payloads, and page buffers) using
/// their content hash as the key.  The trait is blanket-implemented for `&T` and `Box<T>` via
/// `auto_impl`.
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

        let res = if payload.len() < PayloadLookup::STORING_THRESHOLD {
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

/// [`QueryableStorage`] is a extension over [`Storage`] which provides methods to query
/// runtime primitives from it.
pub trait QueryableStorage<T>: Storage {
    /// Reads `T` from storage by `hash`, returning a default value when the hash is absent.
    fn query(&self, hash: &MaybeHashOf<T>) -> Result<T>;
}

/// [`ModifiableStorage`] is a extension over [`Storage`] which provides method to modify
/// runtime primitives by its hash.
pub trait ModifiableStorage<T>: QueryableStorage<T> {
    /// Loads `T` by `hash`, applies `f`, writes the result back, and updates `hash` in place.
    fn modify<U>(&self, hash: &mut MaybeHashOf<T>, f: impl FnOnce(&mut T) -> U) -> U;
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
        self.read(hash.inner())
    }

    fn write_message_queue(&self, queue: MessageQueue) -> HashOf<MessageQueue> {
        unsafe { HashOf::new(self.write(queue)) }
    }

    fn waitlist(&self, hash: HashOf<Waitlist>) -> Option<Waitlist> {
        self.read(hash.inner())
    }

    fn write_waitlist(&self, waitlist: Waitlist) -> HashOf<Waitlist> {
        unsafe { HashOf::new(self.write(waitlist)) }
    }

    fn dispatch_stash(&self, hash: HashOf<DispatchStash>) -> Option<DispatchStash> {
        self.read(hash.inner())
    }

    fn write_dispatch_stash(&self, stash: DispatchStash) -> HashOf<DispatchStash> {
        unsafe { HashOf::new(self.write(stash)) }
    }

    fn mailbox(&self, hash: HashOf<Mailbox>) -> Option<Mailbox> {
        self.read(hash.inner())
    }

    fn write_mailbox(&self, mailbox: Mailbox) -> HashOf<Mailbox> {
        unsafe { HashOf::new(self.write(mailbox)) }
    }

    fn user_mailbox(&self, hash: HashOf<UserMailbox>) -> Option<UserMailbox> {
        self.read(hash.inner())
    }

    fn write_user_mailbox(&self, user_mailbox: UserMailbox) -> HashOf<UserMailbox> {
        unsafe { HashOf::new(self.write(user_mailbox)) }
    }

    fn memory_pages(&self, hash: HashOf<MemoryPages>) -> Option<MemoryPages> {
        self.read(hash.inner())
    }

    fn memory_pages_region(&self, hash: HashOf<MemoryPagesRegion>) -> Option<MemoryPagesRegion> {
        self.read(hash.inner())
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
        self.read(hash.inner())
    }

    fn write_allocations(&self, allocations: Allocations) -> HashOf<Allocations> {
        unsafe { HashOf::new(self.write(allocations)) }
    }

    fn payload(&self, hash: HashOf<Payload>) -> Option<Payload> {
        self.read(hash.inner())
    }

    fn write_payload(&self, payload: Payload) -> HashOf<Payload> {
        unsafe { HashOf::new(self.write(payload)) }
    }

    fn page_data(&self, hash: HashOf<PageBuf>) -> Option<PageBuf> {
        self.read(hash.inner())
    }

    fn write_page_data(&self, data: PageBuf) -> HashOf<PageBuf> {
        unsafe { HashOf::new(self.write(data)) }
    }
}
