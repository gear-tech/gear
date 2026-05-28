// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Queue storage manager.

use crate::state::WithOverlay;
use gear_core::message::StoredDispatch;
use std::{collections::VecDeque, thread::LocalKey};

thread_local! {
    /// Definition of the storage value storing dispatches queue.
    pub(super) static DISPATCHES_QUEUE: WithOverlay<VecDeque<StoredDispatch>> = Default::default();
}

fn storage() -> &'static LocalKey<WithOverlay<VecDeque<StoredDispatch>>> {
    &DISPATCHES_QUEUE
}

#[derive(Debug, Clone, Default)]
pub(crate) struct QueueManager;

impl QueueManager {
    /// Push dispatch to the queue back.
    pub(crate) fn push_back(&self, dispatch: StoredDispatch) {
        storage().with(|queue| queue.data_mut().push_back(dispatch));
    }

    /// Push dispatch to the queue head.
    pub(crate) fn push_front(&self, dispatch: StoredDispatch) {
        storage().with(|queue| queue.data_mut().push_front(dispatch));
    }

    /// Pop dispatch from the queue head.
    pub(crate) fn pop_front(&self) -> Option<StoredDispatch> {
        storage().with(|queue| queue.data_mut().pop_front())
    }

    /// Clears the queue.
    pub(crate) fn clear(&self) {
        storage().with(|queue| queue.data_mut().clear())
    }

    /// Returns queue length.
    pub(crate) fn len(&self) -> usize {
        storage().with(|queue| queue.data().len())
    }
}
