// This file is part of Gear.

// Copyright (C) 2023-2024 Gear Technologies Inc.
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

use gstd::{critical, debug, exec, msg, prelude::*, ActorId};

#[gstd::async_main]
async fn main() {
    let source = msg::source();

    gstd::msg::send_bytes_for_reply(source, b"for_reply", 0, 0)
        .expect("Failed to send message")
        .handle_reply(|| {
            debug!("reply message_id: {:?}", msg::id());
            debug!("reply payload: {:?}", msg::load_bytes());
            msg::send_bytes(msg::source(), b"saw_reply", 0);
        })
        .await
        .expect("Received error reply");
}
