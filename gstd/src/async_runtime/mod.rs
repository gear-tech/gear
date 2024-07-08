// This file is part of Gear.

// Copyright (C) 2021-2024 Gear Technologies Inc.
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

mod futures;
mod locks;
mod reply_hooks;
mod signals;
mod waker;

pub use self::futures::message_loop;
pub(crate) use locks::Lock;
pub(crate) use reply_hooks::{clear_reply_hook, execute_and_clear_reply_hook, register_reply_hook};
pub(crate) use signals::ReplyPoll;

use self::futures::FuturesMap;
use crate::critical;
use hashbrown::HashMap;
use locks::LocksMap;
use reply_hooks::HooksMap;
use signals::WakeSignals;

static mut FUTURES: Option<FuturesMap> = None;

pub(crate) fn futures() -> &'static mut FuturesMap {
    unsafe { FUTURES.get_or_insert_with(HashMap::new) }
}

static mut SIGNALS: Option<WakeSignals> = None;

pub(crate) fn signals() -> &'static mut WakeSignals {
    unsafe { SIGNALS.get_or_insert_with(WakeSignals::new) }
}

static mut LOCKS: Option<LocksMap> = None;

pub(crate) fn locks() -> &'static mut LocksMap {
    unsafe { LOCKS.get_or_insert_with(LocksMap::default) }
}

static mut REPLY_HOOKS: Option<HooksMap> = None;

fn reply_hooks() -> &'static mut HooksMap {
    unsafe { REPLY_HOOKS.get_or_insert_with(HashMap::new) }
}

/// Default reply handler.
pub fn record_reply() {
    signals().record_reply();

    let replied_to = crate::msg::reply_to().expect("record_reply called in wrong context");

    // Execute reply hook (if it was registered)
    execute_and_clear_reply_hook(replied_to);
}

/// Default signal handler.
pub fn handle_signal() {
    let msg_id = crate::msg::signal_from().expect(
        "`gstd::async_runtime::handle_signal()` must be called only in `handle_signal` entrypoint",
    );

    critical::take_and_execute();

    futures().remove(&msg_id);
    locks().remove_message_entry(msg_id);

    clear_reply_hook(msg_id)
}
