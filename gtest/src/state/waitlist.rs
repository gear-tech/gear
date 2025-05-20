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

//! Waitlist manager.

#![allow(unused)]

use crate::state::blocks::GetBlockNumberImpl;
use gear_common::{
    auxiliary::{waitlist::*, BlockNumber},
    storage::{Interval, IterableByKeyMap, Waitlist, WaitlistCallbacks},
};
use gear_core::{
    ids::{ActorId, MessageId},
    message::StoredDispatch,
};

/// Waitlist manager which operates under the hood over
/// [`gear_common::auxiliary::waitlist::AuxiliaryWaitlist`].
#[derive(Debug, Default)]
pub(crate) struct WaitlistManager;

impl WaitlistManager {
    /// Check if message with `message_id` to a program with `program_id` is in
    /// the waitlist.
    pub(crate) fn contains(&self, program_id: ActorId, message_id: MessageId) -> bool {
        <AuxiliaryWaitlist<WaitlistCallbacksImpl> as Waitlist>::contains(&program_id, &message_id)
    }

    /// Insert message into waitlist.
    pub(crate) fn insert(
        &self,
        message: WaitlistedMessage,
        expected: BlockNumber,
    ) -> Result<(), WaitlistErrorImpl> {
        <AuxiliaryWaitlist<WaitlistCallbacksImpl> as Waitlist>::insert(message, expected)
    }

    /// Remove message from the waitlist.
    pub(crate) fn remove(
        &self,
        program_id: ActorId,
        message_id: MessageId,
    ) -> Result<(WaitlistedMessage, Interval<BlockNumber>), WaitlistErrorImpl> {
        <AuxiliaryWaitlist<WaitlistCallbacksImpl> as Waitlist>::remove(program_id, message_id)
    }

    /// Fully reset waitlist.
    ///
    /// # Note:
    /// Must be called by `WaitlistManager` owner to reset waitlist
    /// when the owner is dropped.
    pub(crate) fn reset(&self) {
        <AuxiliaryWaitlist<WaitlistCallbacksImpl> as Waitlist>::clear();
    }

    pub(crate) fn drain_key(
        &self,
        program_id: ActorId,
    ) -> impl Iterator<Item = (StoredDispatch, Interval<BlockNumber>)> + use<> {
        <AuxiliaryWaitlist<WaitlistCallbacksImpl> as IterableByKeyMap<(
            StoredDispatch,
            Interval<BlockNumber>,
        )>>::drain_key(program_id)
    }
}

/// Waitlist callbacks implementor.
pub(crate) struct WaitlistCallbacksImpl;

impl WaitlistCallbacks for WaitlistCallbacksImpl {
    type Value = WaitlistedMessage;
    type BlockNumber = BlockNumber;

    type GetBlockNumber = GetBlockNumberImpl;

    type OnInsert = ();
    type OnRemove = ();
}
