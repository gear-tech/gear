// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

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
