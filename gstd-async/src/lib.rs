// This file is part of Gear.

// Copyright (C) 2021 Gear Technologies Inc.
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

#![no_std]
#![cfg_attr(feature = "strict", deny(warnings))]
#![doc(html_logo_url = "https://gear-tech.io/images/logo-black.svg")]

extern crate alloc;

use alloc::{boxed::Box, collections::BTreeMap};
use core::{future::Future, pin::Pin, task::Context};
use futures::FutureExt;
use gcore::MessageId;

pub use gstd_async_macro::main;

pub mod msg;
pub mod mutex;
pub mod rwlock;
mod waker;

type LocalBoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;

static mut MAIN_FUTURES: Option<BTreeMap<MessageId, LocalBoxFuture<'static, ()>>> = None;

fn main_futures_static() -> &'static mut BTreeMap<MessageId, LocalBoxFuture<'static, ()>> {
    unsafe {
        if MAIN_FUTURES.is_none() {
            MAIN_FUTURES = Some(BTreeMap::new())
        }

        MAIN_FUTURES
            .as_mut()
            .expect("Set if none above; cannot fail")
    }
}

/// Asynchronous message handling main loop.
pub fn main_loop<F>(future: F)
where
    F: Future<Output = ()> + 'static,
{
    let mut actual_future = main_futures_static()
        .remove(&gcore::msg::id())
        .unwrap_or_else(|| future.boxed_local());

    // Create context based on an empty waker
    let waker = waker::empty();
    let mut cx = Context::from_waker(&waker);

    let pinned = Pin::new(&mut actual_future);

    if pinned.poll(&mut cx).is_ready() {
        // Done!
    } else {
        main_futures_static().insert(gcore::msg::id(), actual_future);
        gcore::exec::wait()
    }
}

#[no_mangle]
unsafe extern "C" fn handle_reply() {
    let original_message_id = gcore::msg::reply_to();
    crate::msg::signals_static().record_reply(
        original_message_id,
        gstd::msg::load_bytes(),
        gstd::msg::exit_code(),
    );
}
