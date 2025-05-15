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

//! Queue storage manager.

use gear_core::message::StoredDispatch;
use std::{cell::RefCell, collections::VecDeque, thread::LocalKey};

thread_local! {
    /// Definition of the storage value storing dispatches queue.
    pub(super) static DISPATCHES_QUEUE: RefCell<VecDeque<StoredDispatch>> = const { RefCell::new(VecDeque::new()) };
}

fn storage() -> &'static LocalKey<RefCell<VecDeque<StoredDispatch>>> {
    if super::overlay_enabled() {
        &super::DISPATCHES_QUEUE_OVERLAY
    } else {
        &DISPATCHES_QUEUE
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct QueueManager;

impl QueueManager {
    /// Push dispatch to the queue back.
    pub(crate) fn push_back(&self, dispatch: StoredDispatch) {
        storage().with_borrow_mut(|queue| queue.push_back(dispatch));
    }

    /// Push dispatch to the queue head.
    pub(crate) fn push_front(&self, dispatch: StoredDispatch) {
        storage().with_borrow_mut(|queue| queue.push_front(dispatch));
    }

    /// Pop dispatch from the queue head.
    pub(crate) fn pop_front(&self) -> Option<StoredDispatch> {
        storage().with_borrow_mut(|queue| queue.pop_front())
    }

    /// Clears the queue.
    pub(crate) fn clear(&self) {
        storage().with_borrow_mut(|queue| queue.clear())
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        storage().with_borrow(|queue| queue.len())
    }
}
