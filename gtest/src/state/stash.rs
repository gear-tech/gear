// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
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

//! Stash manager.

use gear_common::{
    auxiliary::{overlay::WithOverlay, BlockNumber},
    storage::Interval,
};
use gear_core::{ids::MessageId, message::StoredDelayedDispatch};
use std::{collections::HashMap, thread::LocalKey};

pub(super) type DispatchStashType =
    WithOverlay<HashMap<MessageId, (StoredDelayedDispatch, Interval<BlockNumber>)>>;
thread_local! {
    /// Definition of the storage value storing stash.
    pub(super) static DISPATCHES_STASH: DispatchStashType = Default::default();
}

fn storage() -> &'static LocalKey<DispatchStashType> {
    &DISPATCHES_STASH
}

#[derive(Debug, Clone, Default)]
pub(crate) struct DispatchStashManager;

impl DispatchStashManager {
    pub(crate) fn contains_key(&self, key: &MessageId) -> bool {
        storage().with(|stash| stash.data().contains_key(key))
    }

    pub(crate) fn insert(
        &self,
        key: MessageId,
        value: (StoredDelayedDispatch, Interval<BlockNumber>),
    ) {
        storage().with(|stash| {
            stash.data_mut().insert(key, value);
        });
    }

    pub(crate) fn remove(
        &self,
        key: &MessageId,
    ) -> Option<(StoredDelayedDispatch, Interval<BlockNumber>)> {
        storage().with(|stash| stash.data_mut().remove(key))
    }
}
