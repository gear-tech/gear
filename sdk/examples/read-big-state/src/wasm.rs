// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::State;
use gstd::{msg, prelude::*};

static mut STATE: Option<State> = None;

fn state_mut() -> &'static mut State {
    unsafe { static_mut!(STATE).get_or_insert_with(State::new) }
}

#[unsafe(no_mangle)]
extern "C" fn handle() {
    let strings = msg::load().expect("Failed to load state");
    state_mut().insert(strings);
}

#[unsafe(no_mangle)]
extern "C" fn state() {
    msg::reply(state_mut(), 0).expect("Error in reply of state");
}
