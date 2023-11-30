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

//! This contract has an async `init` function, which sends a message to the source, awaiting a
//! reply, then exiting the program from within the init method.

use gstd::{
    exec, msg,
    prelude::{vec, *},
};

#[gstd::async_init]
async fn init() {
    msg::send_bytes_for_reply(msg::source(), vec![], 0, 0)
        .expect("send message failed")
        .await
        .expect("ran into error-reply");
    exec::exit(msg::source());
}
