// This file is part of Gear.

// Copyright (C) Gear Technologies Inc.
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

use crate::DELAY;
use gstd::{ActorId, MessageId, exec, msg, prelude::*};

static mut MID: Option<MessageId> = None;
static mut DONE: bool = false;
static mut MSG_DEST: Option<ActorId> = None;

fn send_delayed_to_self() -> bool {
    let to_self = msg::load_bytes().unwrap().as_slice() == b"self";
    if to_self {
        gstd::debug!("sending delayed message to self");
        msg::send_bytes_delayed(exec::program_id(), b"self", exec::value_available(), DELAY)
            .unwrap();
    }

    to_self
}

fn parse_msg_dest() -> Option<ActorId> {
    msg::load()
        .map(|dest: ActorId| {
            unsafe {
                MSG_DEST = Some(dest);
            }
            dest
        })
        .ok()
}

fn parse_delay() -> Option<u32> {
    msg::load().ok()
}

fn msg_dest() -> ActorId {
    match unsafe { MSG_DEST } {
        Some(dest) => dest,
        None => msg::source(),
    }
}

#[unsafe(no_mangle)]
extern "C" fn init() {
    // Send message to self
    if send_delayed_to_self() {
        return;
    }

    // Parse message destination and delay from init payload
    let delay = if parse_msg_dest().is_none() {
        parse_delay().unwrap_or(DELAY)
    } else {
        DELAY
    };

    gstd::debug!(
        "Init, sending delayed message to: {:?} with delay: {:?}",
        msg_dest(),
        delay
    );

    msg::send_bytes_delayed(msg_dest(), "Delayed hello!", exec::value_available(), delay).unwrap();
}

#[unsafe(no_mangle)]
extern "C" fn handle() {
    if send_delayed_to_self() {
        return;
    }

    let size = msg::size();

    if size == 0 {
        // Another case of delayed sending, representing possible panic case of
        // sending delayed gasless messages.
        msg::send_bytes_delayed(msg_dest(), [], exec::value_available() / 2, DELAY)
            .expect("Failed to send msg");

        msg::send_bytes_delayed(msg_dest(), [], exec::value_available() / 2, DELAY)
            .expect("Failed to send msg");

        return;
    }

    // Common delayed sender case.
    if let Some(message_id) = unsafe { static_mut!(MID).take() } {
        let delay: u32 = msg::load().unwrap();

        unsafe {
            DONE = true;
        }

        exec::wake_delayed(message_id, delay).expect("Failed to wake message");
    } else if unsafe { !DONE } {
        unsafe {
            MID = Some(msg::id());
        }

        exec::wait();
    }
}
