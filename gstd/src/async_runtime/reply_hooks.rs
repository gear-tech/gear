// This file is part of Gear.

// Copyright (C) 2023-2024 Gear Technologies Inc.
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

use crate::MessageId;
use alloc::boxed::Box;
use hashbrown::HashMap;

pub(crate) struct HooksMap(HashMap<MessageId, Box<dyn FnOnce()>>);

impl HooksMap {
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    /// Register hook to be executed when a reply for message_id is received.
    pub(crate) fn register<F: FnOnce() + 'static>(&mut self, mid: MessageId, f: F) {
        if self.0.contains_key(&mid) {
            panic!("handle_reply: reply hook for this message_id is already registered");
        }
        self.0.insert(mid, Box::new(f));
    }

    /// Execute hook for message_id (if registered)
    pub(crate) fn execute_and_remove(&mut self, message_id: MessageId) {
        if let Some(f) = self.0.remove(&message_id) {
            f();
        }
    }

    /// Clear hook for message_id without executing it.
    pub(crate) fn remove(&mut self, message_id: MessageId) {
        self.0.remove(&message_id);
    }
}
