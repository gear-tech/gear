// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! This program recursively calls payload stack allocated read or load,
//! depends on actions vector, which is set in init.
//! For each recursion step we call check_sum, which is sum of all payload bytes.
//! Then reply summary check_sum back to source account.

use crate::{Action, HandleData, ReplyData, Vec};
use gstd::{msg, prelude::*};

struct State {
    actions: Vec<Action>,
}

static mut STATE: State = State {
    actions: Vec::new(),
};

fn do_actions(mut actions: Vec<Action>) -> u32 {
    let check_sum = match actions.pop() {
        Some(Action::Read) => msg::with_read_on_stack_or_heap(|payload| {
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

#[unsafe(no_mangle)]
extern "C" fn handle() {
    let check_sum = do_actions(unsafe { static_mut!(STATE).actions.clone() });
    msg::reply(ReplyData { check_sum }, 0).expect("Failed to reply");
}

#[unsafe(no_mangle)]
extern "C" fn init() {
    unsafe {
        STATE.actions = msg::load().expect("Failed to load init config");
    }
}
