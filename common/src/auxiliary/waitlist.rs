// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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

//! Auxiliary implementation of the waitlist.

use super::{AuxiliaryDoubleStorageWrap, BlockNumber, DoubleBTreeMap};
use crate::{auxiliary::overlay::WithOverlay, storage::{Interval, WaitlistError, WaitlistImpl, WaitlistKeyGen}};
use gear_core::{
    ids::{ActorId, MessageId},
    message::StoredDispatch,
};
use std::thread::LocalKey;

/// Waitlist implementation that can be used in a native, non-wasm runtimes.
pub type AuxiliaryWaitlist<WaitListCallbacks> = WaitlistImpl<
    WaitlistStorageWrap,
    WaitlistedMessage,
    BlockNumber,
    WaitlistErrorImpl,
    WaitlistErrorImpl,
    WaitListCallbacks,
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
