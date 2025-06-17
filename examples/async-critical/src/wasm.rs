// This file is part of Gear.

// Copyright (C) 2023-2025 Gear Technologies Inc.
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

use crate::HandleAction;
use gstd::{critical, msg, prelude::*, ActorId};

static mut REPLY_SET_HOOK: bool = false;
static mut SIGNAL_SET_HOOK: bool = false;
static mut INITIATOR: ActorId = ActorId::zero();

#[gstd::async_main(handle_reply = my_handle_reply, handle_signal = my_handle_signal)]
async fn main() {
    unsafe { INITIATOR = msg::source() };

    let action: HandleAction = msg::load().expect("Failed to read handle action");

    match action {
        HandleAction::Simple => {
            // call `gr_source` outside because it is forbidden in `handle_signal`
            let source = msg::source();

            // should not send anything because execution will be completed
            critical::set_hook(move || {
                msg::send_bytes(source, b"critical", 0).unwrap();
            });

            // wait occurs inside so hook is saved
            gstd::msg::send_bytes_for_reply(source, b"for_reply", 0, 0)
                .expect("Failed to send message")
                .await
                .expect("Received error reply");
        }
        HandleAction::Panic => {
            // call `gr_source` outside because it is forbidden in `handle_signal`
            let source = msg::source();

            // should send message because panic occurs below
            critical::set_hook(move || {
                msg::send_bytes(source, b"critical", 0).unwrap();
            });

            // wait occurs inside so hook is saved
            gstd::msg::send_bytes_for_reply(msg::source(), b"for_reply", 0, 0)
                .expect("Failed to send message")
                .await
                .expect("Received error reply");

            // panic occurs so `handle_signal` will execute hook
            panic!();
        }
        HandleAction::InHandleReply => {
            unsafe {
                REPLY_SET_HOOK = true;
            }

            gstd::msg::send_bytes_for_reply(msg::source(), b"for_reply", 0, 0)
                .expect("Failed to send message")
                .await
                .expect("Received error reply");
        }
        HandleAction::InHandleSignal => {
            unsafe {
                SIGNAL_SET_HOOK = true;
            }

            gstd::msg::send_bytes_for_reply(msg::source(), b"for_reply", 0, 0)
                .expect("Failed to send message")
                .await
                .expect("Received error reply");

            panic!()
        }
    }
}

fn my_handle_reply() {
    unsafe {
        if REPLY_SET_HOOK {
            // should panic in this entrypoint
            critical::set_hook(move || {
                msg::send_bytes(INITIATOR, b"from_handle_reply", 0).unwrap();
            });
        }
    }
}

fn my_handle_signal() {
    unsafe {
        if SIGNAL_SET_HOOK {
            // should panic in this entrypoint
            critical::set_hook(move || {
                msg::send_bytes(INITIATOR, b"from_handle_signal", 0).unwrap();
            });
        }
    }
}
