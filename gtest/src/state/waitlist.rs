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

use crate::{
    constants::BlockNumber,
    state::{blocks::GetBlockNumberImpl, WithOverlay},
};
use gear_common::storage::{
    AuxiliaryDoubleStorageWrap, DoubleBTreeMap, Interval, IterableByKeyMap, Waitlist,
    WaitlistCallbacks, WaitlistError, WaitlistImpl, WaitlistKeyGen,
};
use gear_core::{
    ids::{ActorId, MessageId},
    message::StoredDispatch,
};
use std::thread::LocalKey;

/// Waitlist implementation that can be used in a native, non-wasm runtimes.
pub type AuxiliaryWaitlist = WaitlistImpl<
    WaitlistStorageWrap,
    WaitlistedMessage,
    BlockNumber,
    WaitlistErrorImpl,
    WaitlistErrorImpl,
    WaitlistCallbacksImpl,
    WaitlistKeyGen,
>;
/// Type represents message stored in the waitlist.
pub type WaitlistedMessage = StoredDispatch;

pub(crate) type WaitlistStorage =
    WithOverlay<DoubleBTreeMap<ActorId, MessageId, (WaitlistedMessage, Interval<BlockNumber>)>>;
std::thread_local! {
    // Definition of the waitlist (`StorageDoubleMap`) global storage, accessed by the `Waitlist` trait implementor.
    pub(crate) static WAITLIST_STORAGE: WaitlistStorage = Default::default();
}

fn storage() -> &'static LocalKey<WaitlistStorage> {
    &WAITLIST_STORAGE
}

#[derive(Debug, Default)]
pub(crate) struct WaitlistManager;

impl WaitlistManager {
    /// Check if message with `message_id` to a program with `program_id` is in
    /// the waitlist.
    pub(crate) fn contains(&self, program_id: ActorId, message_id: MessageId) -> bool {
        <AuxiliaryWaitlist as Waitlist>::contains(&program_id, &message_id)
    }

    /// Insert message into waitlist.
    pub(crate) fn insert(
        &self,
        message: WaitlistedMessage,
        expected: BlockNumber,
    ) -> Result<(), WaitlistErrorImpl> {
        <AuxiliaryWaitlist as Waitlist>::insert(message, expected)
    }

    /// Remove message from the waitlist.
    pub(crate) fn remove(
        &self,
        program_id: ActorId,
        message_id: MessageId,
    ) -> Result<(WaitlistedMessage, Interval<BlockNumber>), WaitlistErrorImpl> {
        <AuxiliaryWaitlist as Waitlist>::remove(program_id, message_id)
    }

    /// Fully reset waitlist.
    ///
    /// # Note:
    /// Must be called by `WaitlistManager` owner to reset waitlist
    /// when the owner is dropped.
    pub(crate) fn clear(&self) {
        <AuxiliaryWaitlist as Waitlist>::clear();
    }

    pub(crate) fn drain_key(
        &self,
        program_id: ActorId,
    ) -> impl Iterator<Item = (StoredDispatch, Interval<BlockNumber>)> + Send + 'static {
        <AuxiliaryWaitlist as IterableByKeyMap<(StoredDispatch, Interval<BlockNumber>)>>::drain_key(
            program_id,
        )
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

/// `Waitlist` double storage map manager.
pub struct WaitlistStorageWrap;

impl AuxiliaryDoubleStorageWrap for WaitlistStorageWrap {
    type Key1 = ActorId;
    type Key2 = MessageId;
    type Value = (WaitlistedMessage, Interval<BlockNumber>);

    fn with_storage<F, R>(f: F) -> R
    where
        F: FnOnce(&DoubleBTreeMap<Self::Key1, Self::Key2, Self::Value>) -> R,
    {
        storage().with(|wls| f(&wls.data()))
    }

    fn with_storage_mut<F, R>(f: F) -> R
    where
        F: FnOnce(&mut DoubleBTreeMap<Self::Key1, Self::Key2, Self::Value>) -> R,
    {
        storage().with(|wls| f(&mut wls.data_mut()))
    }
}

/// An implementor of the error returned from calling `Waitlist` trait functions
#[derive(Debug)]
pub enum WaitlistErrorImpl {
    DuplicateKey,
    ElementNotFound,
}

impl WaitlistError for WaitlistErrorImpl {
    fn duplicate_key() -> Self {
        Self::DuplicateKey
    }

    fn element_not_found() -> Self {
        Self::ElementNotFound
    }
}
