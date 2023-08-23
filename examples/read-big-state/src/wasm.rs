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

use crate::State;
use gstd::{msg, prelude::*};

static mut STATE: Option<State> = None;

fn state_mut() -> &'static mut State {
    unsafe { STATE.get_or_insert_with(State::new) }
}

#[no_mangle]
extern "C" fn handle() {
    let strings = msg::load().expect("Failed to load state");
    state_mut().insert(strings);
}

#[no_mangle]
extern "C" fn state() {
    msg::reply(state_mut(), 0).expect("Error in reply of state");
}
