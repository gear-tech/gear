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

use crate::{HANDLE_REPLY_EXPECT, InputArgs};
use gstd::{ActorId, msg};

static mut DESTINATION: ActorId = ActorId::new([0u8; 32]);

#[unsafe(no_mangle)]
extern "C" fn init() {
    let args: InputArgs = msg::load().expect("Failed to decode `InputArgs'");
    unsafe { DESTINATION = args.destination.into() };
}

#[unsafe(no_mangle)]
extern "C" fn handle() {
    let payload = msg::load_bytes().expect("failed to load bytes");
    msg::send_bytes(unsafe { DESTINATION }, payload, msg::value()).expect("failed to send bytes");
}

#[unsafe(no_mangle)]
extern "C" fn handle_reply() {
    // Will panic here as replies denied in `handle_reply`.
    msg::reply_bytes([], 0).expect(HANDLE_REPLY_EXPECT);
}
