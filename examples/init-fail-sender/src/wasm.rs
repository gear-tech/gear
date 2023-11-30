// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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

//! This contract will get an address, then send an empty payload to it, then send another empty
//! payload, waiting for a reply, then postpone handling for a set number of blocks, given by
//! [`reply_duration()`](super::reply_duration), and then panicking.

use gstd::{msg, ActorId};

#[gstd::async_init]
async fn init() {
    let value_receiver: ActorId = msg::load().unwrap();

    msg::send_bytes_with_gas(value_receiver, [], 50_000, 1_000).unwrap();
    msg::send_bytes_with_gas_for_reply(msg::source(), [], 30_000, 0, 0)
        .unwrap()
        .exactly(Some(super::reply_duration()))
        .unwrap()
        .await
        .expect("Failed to send message");
    panic!();
}
