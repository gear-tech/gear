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

use gstd::{debug, exec, msg};

#[unsafe(no_mangle)]
extern "C" fn handle() {
    msg::with_read_on_stack_or_heap(|msg| {
        let available_value = exec::value_available();
        let value = msg::value();
        let payload = msg.expect("Failed to load payload bytes");
        debug!("inserted: {value}, total: {available_value}");

        if payload == b"smash" {
            debug!("smashing, total: {available_value}");
            msg::send(msg::source(), b"send", available_value).unwrap();
        } else if payload == b"smash_with_reply" {
            debug!("smashing with reply, total: {available_value}");
            msg::reply(b"reply", available_value).unwrap();
        }
    });
}
