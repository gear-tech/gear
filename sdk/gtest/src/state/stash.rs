// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Stash manager.

use crate::{constants::BlockNumber, state::WithOverlay};
use gear_common::storage::Interval;
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

    pub(crate) fn clear(&self) {
        storage().with(|stash| {
            stash.data_mut().clear();
        });
    }
}
