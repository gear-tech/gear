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

use super::reply_hooks;

pub(crate) type HooksMap = HashMap<MessageId, Box<dyn FnOnce()>>;

/// Register hook to be executed when a reply for message_id is received.
pub(crate) fn register_reply_hook<F: FnOnce() + 'static>(mid: MessageId, f: F) {
    reply_hooks().insert(mid, Box::new(f));
}

/// Execute hook for message_id (if registered)
pub(crate) fn execute_reply_hook(mid: MessageId) {
    if let Some(f) = reply_hooks().remove(&mid) {
        f();
    }
}

/// Clear hook for message_id without executing it.
pub(crate) fn clear_reply_hook(mid: MessageId) {
    reply_hooks().remove(&mid);
}
