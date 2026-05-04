// This file is part of Gear.

// Copyright (C) 2023-2025 Gear Technologies Inc.
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

// This program recursively composes itself with another program (the other program
// being applied to the input data first): `c(f) = (c(f) . f) x`.
// Every call to the auto_composer program increments the internal `ITER` counter.
// As soon as the counter reaches the `MAX_ITER`, the recursion stops.
// Effectively, this procedure executes a composition of `MAX_ITER` programs `f`
// where the output of the previous call is fed to the input of the next call.

use gstd::{String, debug, exec, msg, prelude::*};

static mut DEBUG: DebugInfo = DebugInfo { me: String::new() };
static mut STATE: State = State { intrinsic: 0 };

struct DebugInfo {
    me: String,
}

struct State {
    intrinsic: u64,
}

impl State {
    fn new(value: u64) -> Self {
        Self { intrinsic: value }
    }

    unsafe fn unchecked_mul(&mut self, other: u64) -> u64 {
        let z: u64 = self
            .intrinsic
            .checked_mul(other)
            .expect("Multiplication overflow");
        debug!(
            "[0x{} mul_by_const::unchecked_mul] Calculated {} x {other} == {z}",
            static_ref!(DEBUG).me,
            self.intrinsic
        );
        z
    }
}

#[unsafe(no_mangle)]
extern "C" fn handle() {
    let x: u64 = msg::load().expect("Expecting a u64 number");

    msg::reply(unsafe { static_mut!(STATE).unchecked_mul(x) }, 0).unwrap();
}

#[unsafe(no_mangle)]
extern "C" fn init() {
    let val: u64 = msg::load().expect("Expecting a u64 number");
    unsafe {
        STATE = State::new(val);
        DEBUG = DebugInfo {
            me: hex::encode(exec::program_id()),
        };
    }
    msg::reply_bytes([], 0).unwrap();
    debug!(
        "[0x{} mul_by_const::init] Program initialized with input {val}",
        unsafe { static_ref!(DEBUG).me.as_str() },
    );
}
