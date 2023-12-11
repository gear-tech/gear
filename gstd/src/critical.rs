// This file is part of Gear.

// Copyright (C) 2023 Gear Technologies Inc.
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

// TODO: describe denied sys-calls in entrypoint (#3580)
//! Critical hook that guarantees code section execution.
//!
//! Code is executed in `handle_signal` entry point in case of failure
//! only across [`exec::wait()`] calls because hook has to be saved.
//!
//! ```rust,no_run
//! use gstd::{critical, msg};
//!
//! # async fn _dummy() {
//! // get source outside of critical hook
//! // because `gr_source` sys-call is forbidden inside `handle_signal` entry point
//! let source = msg::source();
//!
//! critical::set_hook(move || {
//!     msg::send(source, "sends failed", 0).expect("Failed to send emergency message");
//! });
//!
//! let msg = msg::send_for_reply(source, "send_for_reply", 0, 0)
//!     .expect("Failed to send message")
//!     // await on `MessageFuture` which calls `exec::wait()` inside
//!     // so program state will be saved and thus hook will too
//!     .await
//!     .expect("Received error reply");
//!
//! // if some code fails (panic, out of gas, etc) after `exec::wait()` and friends
//! // then saved hook will be executed in `handle_signal`
//!
//! // your code
//! // ...
//!
//! # }
//! ```
//!
//! [`exec::wait()`]: crate::exec::wait

use crate::{msg, MessageId};
use alloc::boxed::Box;
use hashbrown::HashMap;

type HooksMap = HashMap<MessageId, Box<dyn FnMut()>>;

static mut HOOKS: Option<HooksMap> = None;

fn hooks() -> &'static mut HooksMap {
    unsafe { HOOKS.get_or_insert_with(HashMap::new) }
}

/// Sets critical hook.
pub fn set_hook<F: FnMut() + 'static>(f: F) {
    if msg::reply_code().is_ok() {
        panic!("`gstd::critical::set_hook()` must not be called in `handle_reply` entrypoint")
    }

    if msg::signal_code().is_ok() {
        panic!("`gstd::critical::set_hook()` must not be called in `handle_signal` entrypoint")
    }

    hooks().insert(msg::id(), Box::new(f));
}

/// Removes current hook and returns it.
///
/// Must be called inside `handle_signal`:
///
/// ```rust,no_run
/// use gstd::critical;
///
/// #[no_mangle]
/// extern "C" fn handle_signal() {
///     if let Some(f) = critical::take_hook() {
///         f();
///     }
/// }
/// ```
///
/// or __don't__ be used at all if you use
/// [`#[gstd::async_init]`](crate::async_init) or
/// [`#[gstd::async_main]`](crate::async_main).
pub fn take_hook() -> Option<Box<dyn FnMut()>> {
    let msg_id = msg::signal_from().expect(
        "`gstd::critical::execute_hook_once()` must be called only in `handle_signal` entrypoint",
    );

    hooks().remove(&msg_id)
}
