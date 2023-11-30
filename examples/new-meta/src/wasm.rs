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

//! This contract creates a [`Vec`] of wallets. The `handle` function gets one of the wallets
//! based on an id, and replies with it. The `state` function will return a copy of the vector
//! of wallets.

use crate::{MessageIn, MessageInitIn, MessageInitOut, MessageOut, Wallet};
use gstd::{msg, prelude::*};

// State
static mut WALLETS: Vec<Wallet> = Vec::new();

// Init function
#[no_mangle]
extern "C" fn init() {
    unsafe { WALLETS = Wallet::test_sequence() };

    if msg::size() == 0 {
        return;
    }

    let message_init_in: MessageInitIn = msg::load().unwrap();
    let message_init_out: MessageInitOut = message_init_in.into();

    msg::reply(message_init_out, 0).unwrap();
}

// Handle function
#[no_mangle]
extern "C" fn handle() {
    let message_in: MessageIn = msg::load().unwrap();

    let res = unsafe { &WALLETS }
        .iter()
        .find(|w| w.id.decimal == message_in.id.decimal)
        .map(Clone::clone);

    let message_out = MessageOut { res };

    msg::reply(message_out, 0).unwrap();
}

// State-sharing function
#[no_mangle]
extern "C" fn state() {
    msg::reply(unsafe { WALLETS.clone() }, 0).expect("Failed to share state");
}
