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

use gstd::{ActorId, msg};

static mut DESTINATION: ActorId = ActorId::new([0u8; 32]);
static mut REPLY_DEPOSIT: u64 = 0;

#[gstd::async_main]
async fn main() {
    let input = msg::load_bytes().expect("Failed to load payload bytes");
    if let Ok(outcome) =
        msg::send_bytes_for_reply(unsafe { DESTINATION }, input, 0, unsafe { REPLY_DEPOSIT })
            .expect("Error sending message")
            .await
    {
        msg::reply_bytes(outcome, 0).expect("Failed to send reply");
    }
}

#[unsafe(no_mangle)]
extern "C" fn init() {
    let (destination, reply_deposit) = msg::load().expect("Expecting a program address");
    unsafe {
        DESTINATION = destination;
        REPLY_DEPOSIT = reply_deposit;
    }
    msg::reply((), 0).expect("Failed to send reply");
}
