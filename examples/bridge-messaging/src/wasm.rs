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

//! Example showing usage of bridge through built-in actor.

use gstd::{msg, ActorId};

const BRIDGE_BUILTIN: ActorId = ActorId::new(hex_literal::hex!(
    "fcbde8c22642f74490deb7dfbe5d3f8a3b8b499e1fc6b33d3262d50fde0a3e55"
));

#[gstd::async_main]
async fn main() {
    let payload = msg::load_bytes().expect("Failed to load message payload");
    let result = msg::send_bytes_for_reply(BRIDGE_BUILTIN, &payload[..], 0, 0)
        .expect("Error sending message")
        .await;

    if let Ok(reply) = result {
        msg::reply_bytes(reply, 0).expect("Failed to send reply");
    }
}
