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

// This contract recursively composes itself with another contract (the other contract
// being applied to the input data first): `c(f) = (c(f) . f) x`.
// Every call to the auto_composer contract increments the internal `ITER` counter.
// As soon as the counter reaches the `MAX_ITER`, the recursion stops.
// Effectively, this procedure executes a composition of `MAX_ITER` contracts `f`
// where the output of the previous call is fed to the input of the next call.

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

extern crate alloc;
use alloc::vec::Vec;
use codec::{Decode, Encode, MaxEncodedLen};

#[derive(Encode, Decode)]
pub struct InitConfig {
    actions: Vec<Action>,
}

#[derive(Encode, Decode, Clone)]
pub enum Action {
    Read,
    Load,
}

#[derive(Encode, Decode)]
pub struct HandleData {
    data: [u8; 0x100],
}

#[derive(Encode, Decode, MaxEncodedLen)]
pub struct ReplyData {
    check_sum: u32,
}

#[cfg(not(feature = "std"))]
mod wasm {
    use crate::{Action, HandleData, ReplyData, Vec};
    use gstd::{msg, debug};

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
                let HandleData { data } =
                    msg::load_on_stack().expect("Failed to load handle config");
                data.iter().fold(0, |acc, x| acc + *x as u32)
            }
            None => return 0,
        };
        check_sum + do_actions(actions)
    }

    #[no_mangle]
    extern "C" fn handle() {
        let check_sum = do_actions(unsafe { STATE.actions.clone() });
        debug!("check_sum: {}", check_sum);
        msg::reply_on_stack(ReplyData { check_sum }, 0).expect("Failed to reply");
    }

    #[no_mangle]
    extern "C" fn init() {
        unsafe {
            STATE.actions = msg::load_on_stack().expect("Failed to load init config");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{HandleData, InitConfig, ReplyData, Action};
    use gtest::{Log, Program, System};

    // #[test]
    // fn program_can_be_initialized() {
    //     let system = System::new();
    //     system.init_logger();

    //     let program = Program::current(&system);

    //     let from = 42;

    //     let res = program.(from, b"init");
    //     let log = Log::builder().source(program.id()).dest(from);
    //     assert!(res.contains(&log));
    // }

    #[test]
    fn stress() {
        use Action::*;

        let from = 42;
        let system = System::new();
        system.init_logger();

        let program = Program::current_opt(&system);
        let res = program.send(from, InitConfig { actions: vec![Read, Load, Load, Read] });
        println!("{:?}", res);
        let res = program.send(from, HandleData { data: [1; 0x100] });
        println!("{:?}", u32::from_le_bytes(<[u8; 4]>::try_from(res.log()[0].payload()).expect("Reply must be 4 bytes long")));
    }
}
