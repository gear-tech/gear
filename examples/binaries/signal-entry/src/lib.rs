// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
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

#![cfg_attr(not(feature = "std"), no_std)]

use gstd::{Decode, Encode};

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

#[derive(Debug, Encode, Decode)]
pub enum HandleAction {
    Simple,
    Wait,
    WaitAndPanic,
    Accumulate,
    OutOfGas,
}

#[cfg(not(feature = "std"))]
mod wasm {
    use super::*;
    use gstd::{exec, msg, prelude::*, MessageId};

    static mut HANDLE_MSG: MessageId = MessageId::new([0; 32]);
    static mut DO_PANIC: bool = false;

    #[no_mangle]
    unsafe extern "C" fn handle() {
        HANDLE_MSG = msg::id();

        let action: HandleAction = msg::load().unwrap();
        match action {
            HandleAction::Simple => {
                // must be unreserved as unused
                exec::system_reserve_gas(100).unwrap();
            }
            HandleAction::Wait => {
                exec::system_reserve_gas(5_000_000_000).unwrap();
                exec::wait();
            }
            HandleAction::WaitAndPanic => {
                if DO_PANIC {
                    panic!();
                }

                DO_PANIC = !DO_PANIC;

                exec::system_reserve_gas(200).unwrap();
                // used to found message id in test
                msg::send(msg::source(), 0, 0).unwrap();
                exec::wait();
            }
            HandleAction::Accumulate => {
                exec::system_reserve_gas(1000).unwrap();
                exec::system_reserve_gas(234).unwrap();
                exec::wait();
            }
            HandleAction::OutOfGas => {
                exec::system_reserve_gas(5_000_000_000).unwrap();
                loop {}
            }
        }
    }

    #[no_mangle]
    unsafe extern "C" fn handle_signal() {
        assert_eq!(msg::status_code().unwrap(), 1);
    }

    #[no_mangle]
    unsafe extern "C" fn handle_reply() {
        exec::wake(HANDLE_MSG);
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use gtest::{Program, System};

    #[test]
    fn signal_can_be_sent() {
        let system = System::new();
        system.init_logger();

        let program = Program::current(&system);

        let res = program.send_signal(0, 1);
        assert!(!res.main_failed());
    }
}
