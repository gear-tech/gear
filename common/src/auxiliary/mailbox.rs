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

//! Auxiliary implementation of the mailbox.

use crate::{
    auxiliary::{BlockNumber, DoubleBTreeMap},
    storage::{
        CountedByKey, DoubleMapStorage, GetSecondPos, Interval, IterableByKeyMap, IteratorWrap,
        MailboxError, MailboxImpl, MailboxKeyGen,
    },
};
use alloc::collections::btree_map::IntoIter;
use core::cell::RefCell;
use gear_core::{
    ids::{MessageId, ProgramId},
    message::UserStoredMessage,
};

/// Mailbox implementation that can be used in a native, non-wasm runtimes.
pub type AuxiliaryMailbox<MailboxCallbacks> = MailboxImpl<
    MailboxStorageWrap,
    MailboxedMessage,
    BlockNumber,
    MailboxErrorImpl,
    MailboxErrorImpl,
    MailboxCallbacks,
    MailboxKeyGen<ProgramId>,
>;

/// Type represents message stored in the mailbox.
pub type MailboxedMessage = UserStoredMessage;

std::thread_local! {
    // Definition of the mailbox (`StorageDoubleMap`) global storage, accessed by the `Mailbox` trait implementor.
    pub(crate) static MAILBOX_STORAGE: RefCell<DoubleBTreeMap<ProgramId, MessageId, (MailboxedMessage, Interval<BlockNumber>)>> = const { RefCell::new(DoubleBTreeMap::new()) };
}

/// `Mailbox` double storage map manager.
pub struct MailboxStorageWrap;

impl DoubleMapStorage for MailboxStorageWrap {
    type Key1 = ProgramId;
    type Key2 = MessageId;
    type Value = (MailboxedMessage, Interval<BlockNumber>);

    fn contains_keys(key1: &Self::Key1, key2: &Self::Key2) -> bool {
        MAILBOX_STORAGE.with_borrow(|map| map.contains_keys(key1, key2))
    }

    fn get(key1: &Self::Key1, key2: &Self::Key2) -> Option<Self::Value> {
        MAILBOX_STORAGE.with_borrow(|map| map.get(key1, key2).cloned())
    }

    fn insert(key1: Self::Key1, key2: Self::Key2, value: Self::Value) {
        MAILBOX_STORAGE.with_borrow_mut(|map| map.insert(key1, key2, value));
    }

    fn mutate<R, F: FnOnce(&mut Option<Self::Value>) -> R>(
        _key1: Self::Key1,
        _key2: Self::Key2,
        _f: F,
    ) -> R {
        unimplemented!()
    }

    fn mutate_values<F: FnMut(Self::Value) -> Self::Value>(_f: F) {
        unimplemented!()
    }

    fn remove(key1: Self::Key1, key2: Self::Key2) {
        Self::take(key1, key2);
    }

    fn clear() {
        MAILBOX_STORAGE.with_borrow_mut(|map| map.clear())
    }

    fn take(key1: Self::Key1, key2: Self::Key2) -> Option<Self::Value> {
        MAILBOX_STORAGE.with_borrow_mut(|map| map.remove(key1, key2))
    }

    fn clear_prefix(_first_key: Self::Key1) {
        unimplemented!()
    }
}

impl CountedByKey for MailboxStorageWrap {
    type Key = ProgramId;
    type Length = usize;

    fn len(key: &Self::Key) -> Self::Length {
        MAILBOX_STORAGE.with_borrow(|map| map.count_key(key))
    }
}

impl IterableByKeyMap<(MailboxedMessage, Interval<BlockNumber>)> for MailboxStorageWrap {
    type Key = ProgramId;

    type DrainIter = IteratorWrap<
        IntoIter<MessageId, (MailboxedMessage, Interval<BlockNumber>)>,
        (MailboxedMessage, Interval<BlockNumber>),
        GetSecondPos,
    >;

    type Iter = IteratorWrap<
        IntoIter<MessageId, (MailboxedMessage, Interval<BlockNumber>)>,
        (MailboxedMessage, Interval<BlockNumber>),
        GetSecondPos,
    >;

    fn drain_key(key: Self::Key) -> Self::DrainIter {
        MAILBOX_STORAGE
            .with_borrow_mut(|map| map.drain_key(&key))
            .into()
    }

    fn iter_key(key: Self::Key) -> Self::Iter {
        MAILBOX_STORAGE.with_borrow(|map| map.iter_key(&key)).into()
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
