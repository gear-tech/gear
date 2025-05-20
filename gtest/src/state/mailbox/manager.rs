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

use crate::state::blocks::GetBlockNumberImpl;
use gear_common::{
    auxiliary::{mailbox::*, BlockNumber},
    storage::{Interval, IterableByKeyMap, Mailbox, MailboxCallbacks},
};
use gear_core::ids::{ActorId, MessageId};

/// Mailbox manager which operates under the hood over
/// [`gear_common::auxiliary::mailbox::AuxiliaryMailbox`].
#[derive(Debug, Default)]
pub(crate) struct MailboxManager;

impl MailboxManager {
    /// Insert user message into mailbox.
    pub(crate) fn insert(
        &self,
        message: MailboxedMessage,
        expected: BlockNumber,
    ) -> Result<(), MailboxErrorImpl> {
        <AuxiliaryMailbox<MailboxCallbacksImpl> as Mailbox>::insert(message, expected)
    }

    /// Remove user message from mailbox.
    pub(crate) fn remove(
        &self,
        user: ActorId,
        reply_to: MessageId,
    ) -> Result<(MailboxedMessage, Interval<BlockNumber>), MailboxErrorImpl> {
        <AuxiliaryMailbox<MailboxCallbacksImpl> as Mailbox>::remove(user, reply_to)
    }

    /// Returns an iterator over all `to` user messages and their deadlines
    /// inside mailbox.
    pub(crate) fn iter_key(
        &self,
        to: ActorId,
    ) -> impl Iterator<Item = (MailboxedMessage, Interval<BlockNumber>)> + use<> {
        <AuxiliaryMailbox<MailboxCallbacksImpl> as IterableByKeyMap<_>>::iter_key(to)
    }

    /// Fully reset mailbox.
    ///
    /// # Note:
    /// Must be called by `MailboxManager` owner to reset mailbox
    /// when the owner is dropped.
    pub(crate) fn reset(&self) {
        <AuxiliaryMailbox<MailboxCallbacksImpl> as Mailbox>::clear();
    }
}

/// Mailbox callbacks implementor.
pub(crate) struct MailboxCallbacksImpl;

impl MailboxCallbacks<MailboxErrorImpl> for MailboxCallbacksImpl {
    type Value = MailboxedMessage;
    type BlockNumber = BlockNumber;

    type GetBlockNumber = GetBlockNumberImpl;

    type OnInsert = ();
    type OnRemove = ();
}
