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

static mut COUNTER: i32 = 0;
static mut DESTINATION: ActorId = ActorId::zero();

#[unsafe(no_mangle)]
extern "C" fn init() {
    let destination = msg::load().expect("Failed to load destination");
    unsafe { DESTINATION = destination };
}

#[gstd::async_main]
async fn main() {
    let payload = msg::load_bytes().expect("Failed to load payload");

    if payload == b"async" {
        unsafe { COUNTER += 1 };

        let _ = msg::send_bytes_for_reply(unsafe { DESTINATION }, "PING", 0, 0)
            .expect("Failed to send message")
            .await
            .expect("Received error reply");

        msg::reply(unsafe { COUNTER }, 0).expect("Failed to send reply");

        unsafe { COUNTER = 0 };
    }
}
