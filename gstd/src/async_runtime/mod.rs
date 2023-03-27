// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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
mod waker;

pub use self::futures::message_loop;

use self::futures::FuturesMap;
use hashbrown::HashMap;
pub(crate) use locks::Lock;
use locks::LocksMap;
pub(crate) use signals::ReplyPoll;
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

/// Default reply handler.
pub fn record_reply() {
    signals().record_reply();
}

/// Default signal handler.
pub fn handle_signal() {
    let msg_id = crate::msg::signal_from().expect(
        "`gstd::async_runtime::handle_signal()` must be called only in `handle_signal` entrypoint",
    );
    futures().remove(&msg_id);
    locks().remove_message_entry(msg_id);
}
