// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

mod futures;
mod locks;
mod signals;

#[cfg(not(feature = "ethexe"))]
mod reply_hooks;

pub use self::futures::message_loop;
pub(crate) use locks::Lock;
pub(crate) use signals::ReplyPoll;

#[cfg(not(feature = "ethexe"))]
pub(crate) use reply_hooks::HooksMap;

use self::futures::FuturesMap;
use hashbrown::HashMap;
use locks::LocksMap;
use signals::WakeSignals;

#[cfg(not(feature = "ethexe"))]
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

#[cfg(not(feature = "ethexe"))]
static mut REPLY_HOOKS: Option<HooksMap> = None;

#[cfg(not(feature = "ethexe"))]
pub(crate) fn reply_hooks() -> &'static mut HooksMap {
    unsafe { crate::static_mut!(REPLY_HOOKS).get_or_insert_with(HooksMap::new) }
}

/// Default reply handler.
pub fn handle_reply_with_hook() {
    signals().record_reply();

    // Execute reply hook (if it was registered)
    let replied_to =
        crate::msg::reply_to().expect("`gstd::handle_reply_with_hook()` called in wrong context");

    #[cfg(not(feature = "ethexe"))]
    reply_hooks().execute_and_remove(replied_to);

    #[cfg(feature = "ethexe")]
    let _ = replied_to;
}

/// Default signal handler.
#[cfg(not(feature = "ethexe"))]
pub fn handle_signal() {
    let msg_id = crate::msg::signal_from().expect(
        "`gstd::async_runtime::handle_signal()` must be called only in `handle_signal` entrypoint",
    );

    critical::take_and_execute();

    futures().remove(&msg_id);
    locks().remove_message_entry(msg_id);
    reply_hooks().remove(msg_id)
}
