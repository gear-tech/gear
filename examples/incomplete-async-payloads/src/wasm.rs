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

//! This contract gets an address of a ping program when initialized. It can be sent one of four
//! commands.
//!
//! [`HandleStore`](Command::HandleStore) will create a new [`MessageHandle`], push some data to it,
//! ping the ping program, then push some more data and commit the reply.
//!
//! [`ReplyStore`](Command::ReplyStore) will push some data to the default reply, ping the ping
//! program, then push some more data and commit the reply.
//!
//! [`Handle`](Command::Handle) will create a new [`MessageHandle`] and push some data to it, then
//! commit the reply.
//!
//! [`Reply`](Command::Reply) will push some data to the default reply, then commit the reply.
//!
//! [`MessageHandle`]: msg::MessageHandle

use crate::Command;
use gstd::{
    msg::{self, MessageHandle},
    prelude::*,
    ActorId,
};

static mut DESTINATION: ActorId = ActorId::zero();

#[no_mangle]
extern "C" fn init() {
    let destination = msg::load().expect("Failed to load destination");
    unsafe { DESTINATION = destination };
}

async fn ping() {
    msg::send_bytes_for_reply(unsafe { DESTINATION }, "PING", 0, 0)
        .expect("Failed to send message")
        .await
        .expect("Received error reply");
}

#[gstd::async_main]
async fn main() {
    let command = msg::load().expect("Failed to load command");

    match command {
        Command::HandleStore => {
            let handle = MessageHandle::init().expect("Failed to init message");
            handle.push(b"STORED ").expect("Failed to push payload");
            ping().await;
            handle.push("COMMON").expect("Failed to push payload");
            handle
                .commit(msg::source(), 0)
                .expect("Failed to commit message");
        }
        Command::ReplyStore => {
            msg::reply_push(b"STORED ").expect("Failed to push reply payload");
            ping().await;
            msg::reply_push(b"REPLY").expect("Failed to push reply payload");
            msg::reply_commit(0).expect("Failed to commit reply");
        }
        Command::Handle => {
            let handle = MessageHandle::init().expect("Failed to init message");
            handle.push(b"OK PING").expect("Failed to push payload");
            handle
                .commit(msg::source(), 0)
                .expect("Failed to commit message");
        }
        Command::Reply => {
            msg::reply_push(b"OK REPLY").expect("Failed to push reply payload");
            msg::reply_commit(0).expect("Failed to commit reply");
        }
    }
}
