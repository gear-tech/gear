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

mod futures;
mod locks;
mod signals;

#[cfg(not(feature = "gearexe"))]
mod reply_hooks;

pub use self::futures::message_loop;
pub(crate) use locks::Lock;
pub(crate) use signals::ReplyPoll;

#[cfg(not(feature = "gearexe"))]
pub(crate) use reply_hooks::HooksMap;

use self::futures::FuturesMap;
use hashbrown::HashMap;
use locks::LocksMap;
use signals::WakeSignals;

#[cfg(not(feature = "gearexe"))]
use crate::critical;

static mut FUTURES: Option<FuturesMap> = None;

pub(crate) fn futures() -> &'static mut FuturesMap {
    unsafe { crate::static_mut!(FUTURES).get_or_insert_with(HashMap::new) }
}

static mut SIGNALS: Option<WakeSignals> = None;

pub(crate) fn signals() -> &'static mut WakeSignals {
    unsafe { crate::static_mut!(SIGNALS).get_or_insert_with(WakeSignals::new) }
}

static mut LOCKS: Option<LocksMap> = None;

pub(crate) fn locks() -> &'static mut LocksMap {
    unsafe { crate::static_mut!(LOCKS).get_or_insert_with(LocksMap::default) }
}

#[cfg(not(feature = "gearexe"))]
static mut REPLY_HOOKS: Option<HooksMap> = None;

#[cfg(not(feature = "gearexe"))]
pub(crate) fn reply_hooks() -> &'static mut HooksMap {
    unsafe { crate::static_mut!(REPLY_HOOKS).get_or_insert_with(HooksMap::new) }
}

/// Default reply handler.
pub fn handle_reply_with_hook() {
    signals().record_reply();

    // Execute reply hook (if it was registered)
    let replied_to =
        crate::msg::reply_to().expect("`gstd::handle_reply_with_hook()` called in wrong context");

    #[cfg(not(feature = "gearexe"))]
    reply_hooks().execute_and_remove(replied_to);

    #[cfg(feature = "gearexe")]
    let _ = replied_to;
}

/// Default signal handler.
#[cfg(not(feature = "gearexe"))]
pub fn handle_signal() {
    let msg_id = crate::msg::signal_from().expect(
        "`gstd::async_runtime::handle_signal()` must be called only in `handle_signal` entrypoint",
    );

    critical::take_and_execute();

    futures().remove(&msg_id);
    locks().remove_message_entry(msg_id);
    reply_hooks().remove(msg_id)
}
