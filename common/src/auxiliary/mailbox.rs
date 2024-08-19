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
    auxiliary::{AuxiliaryDoubleStorageWrap, BlockNumber, DoubleBTreeMap},
    storage::{Interval, MailboxError, MailboxImpl, MailboxKeyGen},
};
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

impl AuxiliaryDoubleStorageWrap for MailboxStorageWrap {
    type Key1 = ProgramId;
    type Key2 = MessageId;
    type Value = (MailboxedMessage, Interval<BlockNumber>);

    fn with_storage<F, R>(f: F) -> R
    where
        F: FnOnce(&DoubleBTreeMap<Self::Key1, Self::Key2, Self::Value>) -> R,
    {
        MAILBOX_STORAGE.with_borrow(f)
    }

    fn with_storage_mut<F, R>(f: F) -> R
    where
        F: FnOnce(&mut DoubleBTreeMap<Self::Key1, Self::Key2, Self::Value>) -> R,
    {
        MAILBOX_STORAGE.with_borrow_mut(f)
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
