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

use crate::InitAction;
use gstd::{exec, msg};

#[gstd::async_init]
async fn init() {
    let action = msg::load().unwrap();
    match action {
        InitAction::None => {}
        InitAction::Panic => {
            let _bytes = msg::send_for_reply(msg::source(), b"init", 0, 0)
                .unwrap()
                .await
                .unwrap();
            panic!();
        }
    }
}

#[gstd::async_main]
async fn main() {
    msg::send(msg::source(), b"handle_signal", 0).unwrap();
    exec::wait();
}
