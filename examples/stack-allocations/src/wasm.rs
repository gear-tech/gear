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

//! This contract recursively calls payload stack allocated read or load,
//! depends on actions vector, which is set in init.
//! For each recursion step we call check_sum, which is sum of all payload bytes.
//! Then reply summary check_sum back to source account.

use crate::{Action, HandleData, ReplyData, Vec};
use gstd::msg;

struct State {
    actions: Vec<Action>,
}

static mut STATE: State = State {
    actions: Vec::new(),
};

fn do_actions(mut actions: Vec<Action>) -> u32 {
    let check_sum = match actions.pop() {
        Some(Action::Read) => msg::with_read_on_stack(|payload| {
            payload
                .map(|payload| payload.iter().fold(0u32, |acc, x| acc + *x as u32))
                .expect("Failed to read payload")
        }),
        Some(Action::Load) => {
            let HandleData { data } = msg::load().expect("Failed to load handle config");
            data.iter().fold(0, |acc, x| acc + *x as u32)
        }
        None => return 0,
    };
    check_sum + do_actions(actions)
}

#[no_mangle]
extern "C" fn handle() {
    let check_sum = do_actions(unsafe { STATE.actions.clone() });
    msg::reply(ReplyData { check_sum }, 0).expect("Failed to reply");
}

#[no_mangle]
extern "C" fn init() {
    unsafe {
        STATE.actions = msg::load().expect("Failed to load init config");
    }
}
