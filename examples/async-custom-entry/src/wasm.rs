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

static mut USER: ActorId = ActorId::zero();

#[gstd::async_init(handle_reply = my_handle_reply, handle_signal = my_handle_signal)]
async fn init() {
    gstd::Config::set_system_reserve(10_000_000_000).expect("Failed to set system reserve");

    unsafe { USER = msg::source() }
}

#[gstd::async_main]
async fn main() {
    #[allow(clippy::empty_loop)]
    loop {}
}

fn my_handle_reply() {
    unsafe {
        msg::send_bytes(USER, b"my_handle_reply", 0).unwrap();
    }
}

fn my_handle_signal() {
    unsafe {
        msg::send_bytes(USER, b"my_handle_signal", 0).unwrap();
    }
}
