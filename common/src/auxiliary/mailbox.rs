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
    auxiliary::DoubleBTreeMap,
    storage::{
        DoubleMapStorage, Interval, MailboxError as MailboxErrorTrait, MailboxImpl, MailboxKeyGen,
    },
};
use core::cell::RefCell;
use gear_core::{
    ids::{MessageId, ProgramId},
    message::UserStoredMessage,
};

pub type AuxiliaryMailbox<MailboxCallbacks> = MailboxImpl<
    MailboxWrap,
    MailboxedMessage,
    BlockNumber,
    MailboxError,
    MailboxError,
    MailboxCallbacks,
    MailboxKeyGen<ProgramId>,
>;
pub type BlockNumber = u32;
pub type MailboxedMessage = UserStoredMessage;

std::thread_local! {
    // Definition of the `GasNodes` (tree `StorageMap`) global storage, accessed by the tree.
    pub(crate) static MAILBOX: RefCell<DoubleBTreeMap<ProgramId, MessageId, (MailboxedMessage, Interval<BlockNumber>)>> = const { RefCell::new(DoubleBTreeMap::new()) };
}

pub struct MailboxWrap;

impl DoubleMapStorage for MailboxWrap {
    type Key1 = ProgramId;
    type Key2 = MessageId;
    type Value = (MailboxedMessage, Interval<BlockNumber>);

    fn contains_keys(key1: &Self::Key1, key2: &Self::Key2) -> bool {
        MAILBOX.with_borrow(|map| map.contains_keys(key1, key2))
    }

    fn get(key1: &Self::Key1, key2: &Self::Key2) -> Option<Self::Value> {
        MAILBOX.with_borrow(|map| map.get(key1, key2).cloned())
    }

    fn insert(key1: Self::Key1, key2: Self::Key2, value: Self::Value) {
        MAILBOX.with_borrow_mut(|map| map.insert(key1, key2, value));
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
        MAILBOX.with_borrow_mut(|map| map.clear())
    }

    fn take(key1: Self::Key1, key2: Self::Key2) -> Option<Self::Value> {
        MAILBOX.with_borrow_mut(|map| map.remove(key1, key2))
    }

    fn clear_prefix(_first_key: Self::Key1) {
        unimplemented!()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MailboxError {
    DuplicateKey,
    ElementNotFound,
}

impl MailboxErrorTrait for MailboxError {
    fn duplicate_key() -> Self {
        Self::DuplicateKey
    }

    fn element_not_found() -> Self {
        Self::ElementNotFound
    }
}
