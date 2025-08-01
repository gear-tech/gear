// This file is part of Gear.

// Copyright (C) 2024-2025 Gear Technologies Inc.
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

//! Mailbox manager.

use crate::{
    constants::BlockNumber,
    state::{WithOverlay, blocks::GetBlockNumberImpl},
};
use gear_common::storage::{
    AuxiliaryDoubleStorageWrap, DoubleBTreeMap, Interval, IterableByKeyMap, Mailbox,
    MailboxCallbacks, MailboxError, MailboxImpl, MailboxKeyGen,
};
use gear_core::{
    ids::{ActorId, MessageId},
    message::UserStoredMessage,
};
use std::thread::LocalKey;

/// Mailbox implementation that can be used in a native, non-wasm runtimes.
type AuxiliaryMailbox = MailboxImpl<
    MailboxStorageWrap,
    MailboxedMessage,
    BlockNumber,
    MailboxErrorImpl,
    MailboxErrorImpl,
    MailboxCallbacksImpl,
    MailboxKeyGen<ActorId>,
>;
/// Type represents message stored in the mailbox.
pub(crate) type MailboxedMessage = UserStoredMessage;
type MailboxStorage =
    WithOverlay<DoubleBTreeMap<ActorId, MessageId, (MailboxedMessage, Interval<BlockNumber>)>>;
std::thread_local! {
    // Definition of the mailbox (`StorageDoubleMap`) global storage, accessed by the `Mailbox` trait implementor.
    pub(super) static MAILBOX_STORAGE: MailboxStorage = Default::default();
}

fn storage() -> &'static LocalKey<MailboxStorage> {
    &MAILBOX_STORAGE
}

#[derive(Debug, Default)]
pub(crate) struct MailboxManager;

impl MailboxManager {
    /// Insert user message into mailbox.
    pub(crate) fn insert(
        &self,
        message: MailboxedMessage,
        expected: BlockNumber,
    ) -> Result<(), MailboxErrorImpl> {
        <AuxiliaryMailbox as Mailbox>::insert(message, expected)
    }

    /// Remove user message from mailbox.
    pub(crate) fn remove(
        &self,
        user: ActorId,
        reply_to: MessageId,
    ) -> Result<(MailboxedMessage, Interval<BlockNumber>), MailboxErrorImpl> {
        <AuxiliaryMailbox as Mailbox>::remove(user, reply_to)
    }

    /// Returns an iterator over all `to` user messages and their deadlines
    /// inside mailbox.
    pub(crate) fn iter_key(
        &self,
        to: ActorId,
    ) -> impl Iterator<Item = (MailboxedMessage, Interval<BlockNumber>)> + use<> {
        <AuxiliaryMailbox as IterableByKeyMap<_>>::iter_key(to)
    }

    /// Fully reset mailbox.
    ///
    /// # Note:
    /// Must be called by `MailboxManager` owner to reset mailbox
    /// when the owner is dropped.
    pub(crate) fn clear(&self) {
        <AuxiliaryMailbox as Mailbox>::clear();
    }
}

/// Mailbox callbacks implementor.
struct MailboxCallbacksImpl;

impl MailboxCallbacks<MailboxErrorImpl> for MailboxCallbacksImpl {
    type Value = MailboxedMessage;
    type BlockNumber = BlockNumber;

    type GetBlockNumber = GetBlockNumberImpl;

    type OnInsert = ();
    type OnRemove = ();
}

/// `Mailbox` double storage map manager.
pub struct MailboxStorageWrap;

impl AuxiliaryDoubleStorageWrap for MailboxStorageWrap {
    type Key1 = ActorId;
    type Key2 = MessageId;
    type Value = (MailboxedMessage, Interval<BlockNumber>);

    fn with_storage<F, R>(f: F) -> R
    where
        F: FnOnce(&DoubleBTreeMap<Self::Key1, Self::Key2, Self::Value>) -> R,
    {
        storage().with(|ms| f(&ms.data()))
    }

    fn with_storage_mut<F, R>(f: F) -> R
    where
        F: FnOnce(&mut DoubleBTreeMap<Self::Key1, Self::Key2, Self::Value>) -> R,
    {
        storage().with(|ms| f(&mut ms.data_mut()))
    }
}

/// An implementor of the error returned from calling `Mailbox` trait functions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MailboxErrorImpl {
    DuplicateKey,
    ElementNotFound,
}

impl MailboxError for MailboxErrorImpl {
    fn duplicate_key() -> Self {
        Self::DuplicateKey
    }

    fn element_not_found() -> Self {
        Self::ElementNotFound
    }
}
